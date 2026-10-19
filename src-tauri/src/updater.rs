//! Explicit signed upgrades from this product's public GitHub Releases.
//! Download and signature verification finish before any application cleanup.
//! Windows installation uses Tauri's passive NSIS upgrade and automatic restart.

use crate::release_check::{resolve_install_release, InstallRelease};
use crate::AppState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Error as UpdaterError, UpdaterExt};

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const PROGRESS_EMIT_MIN_INTERVAL_BYTES: u64 = 128 * 1024;
const PROGRESS_EVENT: &str = "app-update-progress";
static INSTALL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// One operation owns download, cleanup and installer launch. A failed attempt
/// releases the gate so the user can retry; a successful Windows install exits.
struct InstallGuard<'a>(&'a AtomicBool);
impl<'a> InstallGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self(flag))
            .map_err(|_| {
                note("install.rejected reason=already_running".to_owned());
                "正在更新，请稍候".to_owned()
            })
    }
}
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdatePreferences {
    pub include_prereleases: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum UpdatePhase {
    Preparing,
    Downloading,
    Verifying,
    Installing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateProgress {
    phase: UpdatePhase,
    downloaded: u64,
    content_length: Option<u64>,
    finished: bool,
}

#[derive(Debug, Default)]
struct DownloadProgressState {
    downloaded: u64,
    content_length: Option<u64>,
    last_emitted: u64,
}

fn note(detail: String) {
    #[cfg(windows)]
    sayall_windows::gatt_note(format!("updater.{detail}"));
    #[cfg(not(windows))]
    let _ = detail;
}

fn install_error_message(error: &UpdaterError) -> String {
    match error {
        UpdaterError::Minisign(_) => "更新包校验失败，已停止安装".to_owned(),
        UpdaterError::Network(_) | UpdaterError::Reqwest(_) => {
            "下载更新失败，请检查网络后重试".to_owned()
        }
        _ => "下载或安装暂时失败，请稍后重试".to_owned(),
    }
}

fn install_after_cleanup(
    cleanup: impl FnOnce() -> bool,
    install: impl FnOnce() -> Result<(), UpdaterError>,
) -> Result<(), String> {
    if !cleanup() {
        note("install.fail stage=cleanup error_domain=platform error_code=cleanup_unconfirmed installer_started=false".to_owned());
        return Err("语音连接尚未安全关闭，已停止升级；请退出并重新打开无线麦".to_owned());
    }
    note("install.launch cleanup_confirmed=true restart=true".to_owned());
    install().map_err(|_| {
        note("install.fail stage=installer_launch error_domain=updater error_code=launch_failed cleanup_started=true".to_owned());
        "无法启动安装，请重新打开无线麦后重试".to_owned()
    })
}

fn emit_progress(
    app: &AppHandle,
    phase: UpdatePhase,
    downloaded: u64,
    content_length: Option<u64>,
) {
    let _ = app.emit(
        PROGRESS_EVENT,
        AppUpdateProgress {
            phase,
            downloaded,
            content_length,
            finished: matches!(phase, UpdatePhase::Verifying | UpdatePhase::Installing),
        },
    );
}

/// GitHub's authenticated asset URL may redirect to its asset CDN. Never follow
/// a redirect into another repository, a generic web host, HTTP or credentials.
fn trusted_redirect(url: &reqwest::Url, manifest: &reqwest::Url, installer: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    if url == manifest || url == installer {
        return true;
    }
    match url.host_str() {
        Some("release-assets.githubusercontent.com") => {
            url.path().starts_with("/github-production-release-asset/")
        }
        Some("objects.githubusercontent.com") => {
            url.path().starts_with("/github-production-release-asset-")
        }
        _ => false,
    }
}

fn validate_manifest(
    update: &tauri_plugin_updater::Update,
    release: &InstallRelease,
) -> Result<(), String> {
    validate_manifest_fields(
        &update.version,
        &update.download_url,
        &update.signature,
        release,
    )
}

fn validate_manifest_fields(
    version: &str,
    download_url: &reqwest::Url,
    signature: &str,
    release: &InstallRelease,
) -> Result<(), String> {
    if version != release.version
        || download_url != &release.installer_url
        || signature.trim().is_empty()
    {
        note("install.fail stage=manifest_validation error_domain=updater error_code=manifest_mismatch retryable=false".to_owned());
        return Err("更新文件与所选版本不一致，已停止安装".to_owned());
    }
    Ok(())
}

fn build_updater(
    app: &AppHandle,
    release: &InstallRelease,
) -> Result<tauri_plugin_updater::Updater, String> {
    let manifest = release.manifest_url.clone();
    let installer = release.installer_url.clone();
    let selected_version = release.version.clone();
    let executable =
        std::env::current_exe().map_err(|_| "无法确认安装位置，已停止升级".to_owned())?;
    let install_directory = executable
        .parent()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "无法确认安装位置，已停止升级".to_owned())?;
    if executable.file_name() != Some(std::ffi::OsStr::new("remote-coding.exe"))
        || !install_directory.join("uninstall.exe").is_file()
    {
        note("install.rejected reason=installed_application_required".to_owned());
        return Err("请先安装无线麦，再使用一键更新".to_owned());
    }
    // NSIS requires /D to be the final argument with an unquoted raw path, even
    // when it contains spaces. Tauri appends installer_arg after its own flags.
    let mut directory_argument = std::ffi::OsString::from("/D=");
    directory_argument.push(install_directory);
    // No environment override, runtime key replacement or insecure transport.
    // The configured public key verifies exactly the bytes that install() uses.
    app.updater_builder()
        .timeout(CHECK_TIMEOUT)
        .clear_installer_args()
        .installer_arg(directory_argument)
        .restart_after_install(true)
        .version_comparator(move |current, remote| {
            remote.version.to_string() == selected_version && remote.version > current
        })
        .configure_client(move |client| {
            let manifest = manifest.clone();
            let installer = installer.clone();
            client.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() <= 5 && trusted_redirect(attempt.url(), &manifest, &installer) {
                    attempt.follow()
                } else {
                    attempt.error("untrusted update redirect")
                }
            }))
        })
        .endpoints(vec![release.manifest_url.clone()])
        .and_then(|builder| builder.build())
        .map_err(|_| {
            note("install.fail stage=build error_domain=updater error_code=configuration_invalid retryable=false".to_owned());
            "更新功能暂时不可用，请稍后重试".to_owned()
        })
}

/// Compatibility command uses the same read-only selection as the current UI.
#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdateInfo, String> {
    let info = crate::release_check::check_github_release(app).await?;
    Ok(AppUpdateInfo {
        current_version: info.current_version,
        available: info.status == crate::release_check::ReleaseStatus::Available,
        version: info.version,
        notes: info.notes,
        date: info.published_at,
    })
}

#[tauri::command]
pub fn get_app_update_preferences(
    state: State<'_, AppState>,
) -> Result<AppUpdatePreferences, String> {
    let include_prereleases = state
        .settings
        .load()
        .map_err(|_| "读取更新设置失败，请稍后重试".to_owned())?
        .check_prerelease_updates;
    Ok(AppUpdatePreferences {
        include_prereleases,
    })
}

#[tauri::command]
pub async fn set_app_update_preferences(
    include_prereleases: bool,
    state: State<'_, AppState>,
) -> Result<AppUpdatePreferences, String> {
    let settings = state.settings.clone();
    tauri::async_runtime::spawn_blocking(move || {
        settings.save_check_prerelease_updates(include_prereleases)
    })
    .await
    .map_err(|_| "保存更新设置失败，请稍后重试".to_owned())?
    .map_err(|_| "保存更新设置失败，请稍后重试".to_owned())?;
    note(format!(
        "preference.save result=ok include_prereleases={include_prereleases}"
    ));
    Ok(AppUpdatePreferences {
        include_prereleases,
    })
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, expected_version: String) -> Result<(), String> {
    let _guard = InstallGuard::acquire(&INSTALL_IN_PROGRESS)?;
    let started = Instant::now();
    note("install.start source=explicit_click".to_owned());
    emit_progress(&app, UpdatePhase::Preparing, 0, None);
    let release =
        resolve_install_release(&app.package_info().version.to_string(), &expected_version).await?;
    let mut update = build_updater(&app, &release)?.check().await.map_err(|_| {
        note("install.fail stage=manifest_request error_domain=updater error_code=manifest_unavailable retryable=true".to_owned());
        "暂时无法读取更新文件，请稍后重试".to_owned()
    })?.ok_or_else(|| "更新文件与所选版本不一致，请重新检查更新".to_owned())?;
    validate_manifest(&update, &release)?;
    note(format!(
        "install.manifest_validated version={} expected_bytes={}",
        release.version, release.installer_size
    ));
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let progress = Arc::new(Mutex::new(DownloadProgressState::default()));
    let chunk_progress = Arc::clone(&progress);
    let finish_progress = Arc::clone(&progress);
    let chunk_app = app.clone();
    let finish_app = app.clone();
    emit_progress(
        &app,
        UpdatePhase::Downloading,
        0,
        Some(release.installer_size),
    );
    let bytes = update.download(
        move |length, total| {
            let Ok(mut progress) = chunk_progress.lock() else { return; };
            progress.downloaded += length as u64;
            progress.content_length = total;
            if progress.downloaded.saturating_sub(progress.last_emitted) >= PROGRESS_EMIT_MIN_INTERVAL_BYTES {
                progress.last_emitted = progress.downloaded;
                emit_progress(&chunk_app, UpdatePhase::Downloading, progress.downloaded, total);
            }
        },
        move || {
            if let Ok(progress) = finish_progress.lock() {
                emit_progress(&finish_app, UpdatePhase::Verifying, progress.downloaded, progress.content_length);
                note(format!("install.download_done bytes={} took_ms={}", progress.downloaded, started.elapsed().as_millis()));
            }
        },
    ).await.map_err(|error| {
        let stage = if matches!(error, UpdaterError::Minisign(_)) { "signature" } else { "download" };
        note(format!("install.fail stage={stage} error_domain=updater error_code=download_or_verify_failed cleanup_started=false took_ms={}", started.elapsed().as_millis()));
        install_error_message(&error)
    })?;
    if bytes.len() as u64 != release.installer_size {
        note("install.fail stage=asset_size error_domain=updater error_code=size_mismatch cleanup_started=false".to_owned());
        return Err("更新包大小校验失败，已停止安装".to_owned());
    }
    note(format!(
        "install.signature_verified version={} bytes={}",
        release.version,
        bytes.len()
    ));
    emit_progress(
        &app,
        UpdatePhase::Installing,
        bytes.len() as u64,
        Some(bytes.len() as u64),
    );
    let cleanup_app = app.clone();
    // install() extracts first, then launches NSIS. On Windows it exits only after
    // ShellExecuteW succeeds. Failure is returned rather than claiming an upgrade.
    tauri::async_runtime::spawn_blocking(move || install_after_cleanup(
        || crate::shutdown_platform_for_update(&cleanup_app),
        || update.restart_after_install(true).install(bytes),
    ))
        .await.map_err(|_| {
            note("install.fail stage=cleanup error_domain=runtime error_code=worker_failed cleanup_started=true".to_owned());
            "关闭语音连接失败，已停止升级；请退出并重新打开无线麦".to_owned()
        })??;
    note("install.returned installer_requested=true".to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> InstallRelease {
        InstallRelease {
            version: "0.3.11".to_owned(),
            manifest_url: "https://github.com/MrPhotato/remote-mic-app-windows/releases/download/v0.3.11/latest.json".parse().unwrap(),
            installer_url: "https://github.com/MrPhotato/remote-mic-app-windows/releases/download/v0.3.11/SayAll-Windows-0.3.11-x64-setup.exe".parse().unwrap(),
            installer_size: 42,
        }
    }

    #[test]
    fn simultaneous_install_is_rejected_and_failure_releases_gate() {
        let flag = AtomicBool::new(false);
        let first = InstallGuard::acquire(&flag).unwrap();
        assert!(InstallGuard::acquire(&flag).is_err());
        drop(first);
        assert!(InstallGuard::acquire(&flag).is_ok());
    }

    #[test]
    fn incomplete_cleanup_never_starts_the_installer() {
        let result = install_after_cleanup(|| false, || panic!("installer must not run"));
        assert!(result.unwrap_err().contains("已停止升级"));
    }

    #[test]
    fn cleanup_precedes_installer_and_launch_failure_is_not_reported_as_success() {
        let cleaned = std::cell::Cell::new(false);
        let result = install_after_cleanup(
            || {
                cleaned.set(true);
                true
            },
            || {
                assert!(cleaned.get());
                Err(UpdaterError::Io(std::io::Error::other("launch failed")))
            },
        );
        assert_eq!(result.unwrap_err(), "无法启动安装，请重新打开无线麦后重试");
    }

    #[test]
    fn manifest_must_match_exact_selected_version_url_and_signature() {
        let release = release();
        assert!(validate_manifest_fields(
            &release.version,
            &release.installer_url,
            "signed",
            &release
        )
        .is_ok());
        assert!(
            validate_manifest_fields("0.3.12", &release.installer_url, "signed", &release).is_err()
        );
        assert!(
            validate_manifest_fields(&release.version, &release.installer_url, " ", &release)
                .is_err()
        );
        for url in [
            "https://example.com/installer.exe",
            "https://github.com/GetSayAll/remote-mic-app-windows/releases/download/v0.3.11/SayAll-Windows-0.3.11-x64-setup.exe",
            "https://github.com/MrPhotato/remote-mic-app-windows/releases/download/v0.3.10/SayAll-Windows-0.3.11-x64-setup.exe",
        ] {
            assert!(validate_manifest_fields(&release.version, &url.parse().unwrap(), "signed", &release).is_err());
        }
    }

    #[test]
    fn redirects_only_allow_exact_selected_assets_and_github_asset_cdn() {
        let release = release();
        for url in [release.manifest_url.as_str(), release.installer_url.as_str(), "https://release-assets.githubusercontent.com/github-production-release-asset/1/asset?token=x", "https://objects.githubusercontent.com/github-production-release-asset-2e65be/asset"] {
            assert!(trusted_redirect(&url.parse().unwrap(), &release.manifest_url, &release.installer_url));
        }
        for url in ["http://release-assets.githubusercontent.com/github-production-release-asset/1/x", "https://release-assets.githubusercontent.com.evil.example/github-production-release-asset/1/x", "https://example.com/x", "https://github.com/Other/repository/releases/download/v0.3.11/latest.json", "https://user:pass@release-assets.githubusercontent.com/github-production-release-asset/1/x", "https://release-assets.githubusercontent.com/not-an-asset", "https://release-assets.githubusercontent.com:444/github-production-release-asset/1/x"] {
            assert!(!trusted_redirect(&url.parse().unwrap(), &release.manifest_url, &release.installer_url));
        }
    }

    #[test]
    fn progress_reports_verification_separately_from_installation() {
        let json = serde_json::to_value(AppUpdateProgress {
            phase: UpdatePhase::Verifying,
            downloaded: 4096,
            content_length: Some(4096),
            finished: true,
        })
        .unwrap();
        assert_eq!(json["phase"], "verifying");
        assert_eq!(json["contentLength"], 4096);
        assert_eq!(json["finished"], true);
    }

    #[test]
    fn download_errors_do_not_expose_transport_details() {
        assert_eq!(
            install_error_message(&UpdaterError::Network("private details".into())),
            "下载更新失败，请检查网络后重试"
        );
    }

    #[test]
    #[ignore = "read-only validation against the published v0.3.5 manifest; never installs"]
    fn live_published_manifest_matches_selected_release() {
        tauri::async_runtime::block_on(async {
            let release = resolve_install_release("0.3.4", "0.3.5").await.unwrap();
            let manifest = release.manifest_url.clone();
            let installer = release.installer_url.clone();
            let client = reqwest::Client::builder()
                .timeout(CHECK_TIMEOUT)
                .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                    if attempt.previous().len() <= 5
                        && trusted_redirect(attempt.url(), &manifest, &installer)
                    {
                        attempt.follow()
                    } else {
                        attempt.error("untrusted update redirect")
                    }
                }))
                .build()
                .unwrap();
            let mut response = client
                .get(release.manifest_url.clone())
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.unwrap() {
                assert!(bytes.len() + chunk.len() <= 256 * 1024);
                bytes.extend_from_slice(&chunk);
            }
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let target = &json["platforms"]["windows-x86_64"];
            validate_manifest_fields(
                json["version"].as_str().unwrap(),
                &target["url"].as_str().unwrap().parse().unwrap(),
                target["signature"].as_str().unwrap(),
                &release,
            )
            .unwrap();
            println!("live_manifest_check version={} expected_bytes={} result=passed install_attempted=false", release.version, release.installer_size);
        });
    }
}
