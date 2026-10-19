//! Explicit, read-only checks against this product's public GitHub Releases.
//! This does not enable the legacy updater or download/install any asset.

use semver::Version;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const RELEASES_API: &str = "https://api.github.com/repos/MrPhotato/remote-mic-app-windows/releases";
const REPOSITORY_URL: &str = "https://github.com/MrPhotato/remote-mic-app-windows";
const PER_PAGE: usize = 100;
const MAX_PAGES: usize = 5;
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStatus {
    Available,
    UpToDate,
    NoRelease,
}

impl ReleaseStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::UpToDate => "up_to_date",
            Self::NoRelease => "no_release",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubReleaseInfo {
    pub current_version: String,
    pub status: ReleaseStatus,
    pub version: Option<String>,
    pub release_url: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    state: String,
    size: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum CheckError {
    CurrentVersion,
    Client,
    Timeout,
    Network,
    RateLimited,
    Http(u16),
    ResponseTooLarge,
    ResponseFormat,
    Pagination,
    PageLimit,
    UntrustedRelease,
    InstallerIncomplete,
    UntrustedInstaller,
    UnpublishedRelease,
    ConflictingVersions,
}

impl CheckError {
    fn code(&self) -> &'static str {
        match self {
            Self::CurrentVersion => "invalid_current_version",
            Self::Client => "client_configuration",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::RateLimited => "rate_limited",
            Self::Http(_) => "http_status",
            Self::ResponseTooLarge => "response_too_large",
            Self::ResponseFormat => "response_format",
            Self::Pagination => "pagination_invalid",
            Self::PageLimit => "pagination_limit",
            Self::UntrustedRelease => "release_url_invalid",
            Self::InstallerIncomplete => "installer_incomplete",
            Self::UntrustedInstaller => "installer_url_invalid",
            Self::UnpublishedRelease => "publication_missing",
            Self::ConflictingVersions => "version_conflict",
        }
    }

    fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Network | Self::RateLimited | Self::Http(_)
        )
    }

    fn message(&self) -> &'static str {
        match self {
            Self::Timeout => "检查更新超时，请稍后重试",
            Self::Network => "无法连接 GitHub，请检查网络后重试",
            Self::RateLimited => "GitHub 请求次数受限，请稍后重试",
            Self::Http(_) => "GitHub 暂时无法提供版本信息，请稍后重试",
            Self::InstallerIncomplete | Self::UnpublishedRelease => {
                "最新版本的发布信息或安装包尚不完整，请稍后重试"
            }
            Self::PageLimit => "发布记录过多，暂时无法确认最新版本",
            Self::CurrentVersion => "无法识别当前应用版本",
            Self::Client => "暂时无法建立安全连接，请稍后重试",
            _ => "GitHub 发布信息校验未通过，暂时无法确认最新版本",
        }
    }
}

fn note(operation_id: u64, detail: String) {
    sayall_windows::gatt_note(format!(
        "github_release_check operation_id={operation_id} {detail}"
    ));
}

fn version_precedence(version: &Version) -> Version {
    // SemVer build metadata does not make an otherwise identical version newer.
    let mut precedence = version.clone();
    precedence.build = semver::BuildMetadata::EMPTY;
    precedence
}

fn select_release(current: &str, releases: &[Release]) -> Result<GithubReleaseInfo, CheckError> {
    let current_version = Version::parse(current).map_err(|_| CheckError::CurrentVersion)?;
    let mut highest: Option<(&Release, Version)> = None;
    let mut highest_conflicts = false;
    for release in releases.iter().filter(|release| !release.draft) {
        let tag = release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name);
        let Ok(version) = Version::parse(tag) else {
            // Named non-version releases (e.g. documentation) are not app updates.
            continue;
        };
        match highest.as_ref() {
            None => highest = Some((release, version)),
            Some((previous, previous_version)) => {
                match version_precedence(&version).cmp(&version_precedence(previous_version)) {
                    std::cmp::Ordering::Greater => {
                        highest = Some((release, version));
                        highest_conflicts = false;
                    }
                    std::cmp::Ordering::Equal if previous.tag_name != release.tag_name => {
                        highest_conflicts = true;
                    }
                    _ => {}
                }
            }
        }
    }
    if highest_conflicts {
        return Err(CheckError::ConflictingVersions);
    }

    let Some((release, version)) = highest else {
        return Ok(GithubReleaseInfo {
            current_version: current.to_owned(),
            status: ReleaseStatus::NoRelease,
            version: None,
            release_url: None,
            notes: None,
            published_at: None,
            prerelease: false,
        });
    };

    // Validate the highest version even when it is older/equal to the installed one.
    // Falling back to an older complete release would hide a broken new publication.
    if release.html_url != format!("{REPOSITORY_URL}/releases/tag/{}", release.tag_name) {
        return Err(CheckError::UntrustedRelease);
    }
    if release
        .published_at
        .as_ref()
        .is_none_or(|date| date.trim().is_empty())
    {
        return Err(CheckError::UnpublishedRelease);
    }
    let installer_name = format!("SayAll-Windows-{version}-x64-setup.exe");
    let installers: Vec<_> = release
        .assets
        .iter()
        .filter(|asset| asset.name == installer_name)
        .collect();
    if installers.len() != 1 || installers[0].state != "uploaded" || installers[0].size == 0 {
        return Err(CheckError::InstallerIncomplete);
    }
    let expected_download = format!(
        "{REPOSITORY_URL}/releases/download/{}/{installer_name}",
        release.tag_name
    );
    if installers[0].browser_download_url != expected_download {
        return Err(CheckError::UntrustedInstaller);
    }

    Ok(GithubReleaseInfo {
        current_version: current.to_owned(),
        status: if version_precedence(&version) > version_precedence(&current_version) {
            ReleaseStatus::Available
        } else {
            ReleaseStatus::UpToDate
        },
        version: Some(version.to_string()),
        release_url: Some(release.html_url.clone()),
        notes: release.body.clone(),
        published_at: release.published_at.clone(),
        prerelease: release.prerelease,
    })
}

fn has_next_page(link: Option<&str>, count: usize, page: usize) -> Result<bool, CheckError> {
    let mut next_count = 0;
    if let Some(link) = link {
        for entry in link.split(',') {
            let Some((target, attributes)) = entry.trim().split_once(';') else {
                return Err(CheckError::Pagination);
            };
            if !attributes
                .split(';')
                .any(|attribute| attribute.trim() == "rel=\"next\"")
            {
                continue;
            }
            let target = target
                .trim()
                .strip_prefix('<')
                .and_then(|value| value.strip_suffix('>'))
                .ok_or(CheckError::Pagination)?;
            let url = reqwest::Url::parse(target).map_err(|_| CheckError::Pagination)?;
            let mut expected = reqwest::Url::parse(RELEASES_API).unwrap();
            expected.set_query(url.query());
            let mut query: Vec<_> = url.query_pairs().collect();
            query.sort();
            if url != expected
                || query
                    != vec![
                        ("page".into(), (page + 1).to_string().into()),
                        ("per_page".into(), PER_PAGE.to_string().into()),
                    ]
            {
                return Err(CheckError::Pagination);
            }
            next_count += 1;
        }
    }
    if next_count > 1 || count > PER_PAGE {
        return Err(CheckError::Pagination);
    }
    // A full page without a Link header is not proof that the list is exhausted.
    let more = next_count == 1 || count == PER_PAGE;
    if more && page >= MAX_PAGES {
        return Err(CheckError::PageLimit);
    }
    Ok(more)
}

fn http_error(status: u16, rate_remaining: Option<&str>, retry_after: bool) -> Option<CheckError> {
    match status {
        200 => None,
        429 => Some(CheckError::RateLimited),
        403 if rate_remaining == Some("0") || retry_after => Some(CheckError::RateLimited),
        code => Some(CheckError::Http(code)),
    }
}

fn network_error(error: reqwest::Error) -> CheckError {
    if error.is_timeout() {
        CheckError::Timeout
    } else {
        CheckError::Network
    }
}

async fn fetch_releases(operation_id: u64) -> Result<Vec<Release>, CheckError> {
    let started = Instant::now();
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        return Err(CheckError::Client);
    }
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("SayAll-Windows-Release-Check")
        .build()
        .map_err(|_| CheckError::Client)?;
    let mut releases = Vec::new();
    for page in 1..=MAX_PAGES {
        let remaining = CHECK_TIMEOUT
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(CheckError::Timeout)?;
        note(operation_id, format!("phase=request_started page={page}"));
        let mut response = client
            .get(format!("{RELEASES_API}?per_page={PER_PAGE}&page={page}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .timeout(remaining)
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status().as_u16();
        if let Some(error) = http_error(
            status,
            response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok()),
            response.headers().contains_key("retry-after"),
        ) {
            note(
                operation_id,
                format!("phase=request_rejected page={page} http_status={status}"),
            );
            return Err(error);
        }
        let link = response
            .headers()
            .get(reqwest::header::LINK)
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()
            .map_err(|_| CheckError::Pagination)?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PAGE_BYTES as u64)
        {
            return Err(CheckError::ResponseTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            if chunk.len() > MAX_PAGE_BYTES.saturating_sub(body.len()) {
                return Err(CheckError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        let entries: Vec<Release> =
            serde_json::from_slice(&body).map_err(|_| CheckError::ResponseFormat)?;
        let more = has_next_page(link.as_deref(), entries.len(), page)?;
        note(
            operation_id,
            format!(
                "phase=page_validated page={page} release_count={} has_more={more} took_ms={}",
                entries.len(),
                started.elapsed().as_millis()
            ),
        );
        releases.extend(entries);
        if !more {
            return Ok(releases);
        }
    }
    Err(CheckError::PageLimit)
}

async fn check_version(current: &str, operation_id: u64) -> Result<GithubReleaseInfo, CheckError> {
    Version::parse(current).map_err(|_| CheckError::CurrentVersion)?;
    let releases = fetch_releases(operation_id).await?;
    select_release(current, &releases)
}

#[tauri::command]
pub async fn check_github_release(app: tauri::AppHandle) -> Result<GithubReleaseInfo, String> {
    let operation_id = NEXT_OPERATION.fetch_add(1, Ordering::Relaxed);
    let started = Instant::now();
    let current = app.package_info().version.to_string();
    note(
        operation_id,
        "phase=started source=manual repository=product_public include_prereleases=true".to_owned(),
    );
    match check_version(&current, operation_id).await {
        Ok(info) => {
            note(
                operation_id,
                format!(
                    "phase=completed terminal_result=passed status={} prerelease={} took_ms={}",
                    info.status.label(),
                    info.prerelease,
                    started.elapsed().as_millis()
                ),
            );
            Ok(info)
        }
        Err(error) => {
            note(
                operation_id,
                format!(
                    "phase=completed terminal_result=failed error_domain=release_check error_code={} reason={} retryable={} took_ms={}",
                    error.code(),
                    error.code(),
                    error.retryable(),
                    started.elapsed().as_millis()
                ),
            );
            Err(error.message().to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        let tag = format!("v{version}");
        let installer = format!("SayAll-Windows-{version}-x64-setup.exe");
        Release {
            tag_name: tag.clone(),
            html_url: format!("{REPOSITORY_URL}/releases/tag/{tag}"),
            draft: false,
            prerelease: true,
            published_at: Some("2026-10-19T00:00:00Z".to_owned()),
            body: Some("版本说明".to_owned()),
            assets: vec![Asset {
                name: installer.clone(),
                browser_download_url: format!(
                    "{REPOSITORY_URL}/releases/download/{tag}/{installer}"
                ),
                state: "uploaded".to_owned(),
                size: 42,
            }],
        }
    }

    #[test]
    fn preview_is_available_and_selection_uses_semver_not_date_or_list_order() {
        let result = select_release(
            "0.3.4",
            &[release("0.3.9"), release("0.3.10"), release("0.3.5")],
        )
        .unwrap();
        assert_eq!(result.status, ReleaseStatus::Available);
        assert_eq!(result.version.as_deref(), Some("0.3.10"));
        assert!(result.prerelease);
    }

    #[test]
    fn installed_equal_or_newer_is_never_downgraded() {
        for installed in ["0.3.5", "0.4.0", "0.3.5+local"] {
            assert_eq!(
                select_release(installed, &[release("0.3.5")])
                    .unwrap()
                    .status,
                ReleaseStatus::UpToDate
            );
        }
    }

    #[test]
    fn semantic_prerelease_is_older_than_final_and_build_metadata_is_ignored() {
        let mut stable = release("0.4.0");
        stable.prerelease = false;
        let result = select_release("0.4.0-rc.1", &[release("0.4.0-rc.2"), stable]).unwrap();
        assert_eq!(result.version.as_deref(), Some("0.4.0"));
        assert_eq!(result.status, ReleaseStatus::Available);
        assert!(!result.prerelease);
        assert_eq!(
            select_release("0.4.0+one", &[release("0.4.0+two")])
                .unwrap()
                .status,
            ReleaseStatus::UpToDate
        );
    }

    #[test]
    fn drafts_and_nonversion_tags_are_ignored_and_empty_is_distinct() {
        let mut draft = release("9.0.0");
        draft.draft = true;
        let mut named = release("8.0.0");
        named.tag_name = "documentation".to_owned();
        let result = select_release("0.3.5", &[draft, named]).unwrap();
        assert_eq!(result.status, ReleaseStatus::NoRelease);
        assert!(result.version.is_none());
        assert!(result.release_url.is_none());
    }

    #[test]
    fn invalid_current_version_is_an_error() {
        assert_eq!(
            select_release("unknown", &[]).unwrap_err(),
            CheckError::CurrentVersion
        );
    }

    #[test]
    fn latest_incomplete_release_does_not_fall_back_or_claim_current() {
        for installed in ["0.3.4", "0.3.6", "0.4.0"] {
            let mut incomplete = release("0.3.6");
            incomplete.assets.clear();
            assert_eq!(
                select_release(installed, &[incomplete, release("0.3.5")]).unwrap_err(),
                CheckError::InstallerIncomplete
            );
        }
    }

    #[test]
    fn wrong_name_empty_pending_and_duplicate_installers_are_rejected() {
        for case in 0..4 {
            let mut value = release("0.3.5");
            match case {
                0 => value.assets[0].name = "other.exe".to_owned(),
                1 => value.assets[0].size = 0,
                2 => value.assets[0].state = "new".to_owned(),
                _ => value.assets.push(release("0.3.5").assets.remove(0)),
            }
            assert_eq!(
                select_release("0.3.4", &[value]).unwrap_err(),
                CheckError::InstallerIncomplete
            );
        }
    }

    #[test]
    fn release_and_installer_urls_must_be_exact_same_repository_and_tag() {
        for bad in [
            "http://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.5",
            "https://github.com/GetSayAll/remote-mic-app-windows/releases/tag/v0.3.5",
            "https://github.com/MrPhotato/remote-mic-app-windows/releases/tag/v0.3.5?next=evil",
        ] {
            let mut value = release("0.3.5");
            value.html_url = bad.to_owned();
            assert_eq!(
                select_release("0.3.4", &[value]).unwrap_err(),
                CheckError::UntrustedRelease
            );
        }
        for bad in ["https://example.com/installer.exe", "https://github.com/GetSayAll/remote-mic-app-windows/releases/download/v0.3.5/SayAll-Windows-0.3.5-x64-setup.exe", "https://github.com/MrPhotato/remote-mic-app-windows/releases/download/v0.3.4/SayAll-Windows-0.3.5-x64-setup.exe"] {
            let mut value = release("0.3.5");
            value.assets[0].browser_download_url = bad.to_owned();
            assert_eq!(select_release("0.3.4", &[value]).unwrap_err(), CheckError::UntrustedInstaller);
        }
    }

    #[test]
    fn missing_publication_and_conflicting_versions_are_errors() {
        let mut unpublished = release("0.3.5");
        unpublished.published_at = None;
        assert_eq!(
            select_release("0.3.4", &[unpublished]).unwrap_err(),
            CheckError::UnpublishedRelease
        );
        assert_eq!(
            select_release("0.3.4", &[release("0.3.5+one"), release("0.3.5+two")]).unwrap_err(),
            CheckError::ConflictingVersions
        );
        assert_eq!(
            select_release(
                "0.3.4",
                &[release("0.3.5+one"), release("0.3.5+two"), release("0.3.6")]
            )
            .unwrap()
            .version
            .as_deref(),
            Some("0.3.6")
        );
    }

    #[test]
    fn pagination_checks_all_pages_and_fails_at_incomplete_limit() {
        let link = format!("<{RELEASES_API}?per_page=100&page=2>; rel=\"next\"");
        assert!(has_next_page(Some(&link), 100, 1).unwrap());
        assert!(has_next_page(None, 100, 1).unwrap());
        assert!(!has_next_page(None, 99, 1).unwrap());
        assert!(!has_next_page(None, 0, MAX_PAGES).unwrap());
        assert_eq!(
            has_next_page(None, 100, MAX_PAGES).unwrap_err(),
            CheckError::PageLimit
        );
        assert_eq!(
            has_next_page(None, 101, 1).unwrap_err(),
            CheckError::Pagination
        );
    }

    #[test]
    fn pagination_never_follows_external_repeated_or_malformed_targets() {
        for link in [
            format!("<https://example.com/releases?page=2&per_page=100>; rel=\"next\""),
            format!("<{RELEASES_API}?page=1&per_page=100>; rel=\"next\""),
            format!("<{RELEASES_API}?page=2&per_page=100&token=x>; rel=\"next\""),
            "malformed".to_owned(),
        ] {
            assert_eq!(
                has_next_page(Some(&link), 100, 1).unwrap_err(),
                CheckError::Pagination
            );
        }
    }

    #[test]
    fn http_failures_and_rate_limits_never_mean_up_to_date() {
        assert_eq!(http_error(200, None, false), None);
        assert_eq!(
            http_error(403, Some("0"), false),
            Some(CheckError::RateLimited)
        );
        assert_eq!(http_error(403, None, true), Some(CheckError::RateLimited));
        assert_eq!(http_error(429, None, false), Some(CheckError::RateLimited));
        for status in [301, 403, 404, 500, 503] {
            assert_eq!(
                http_error(status, None, false),
                Some(CheckError::Http(status))
            );
        }
    }

    #[test]
    fn serializes_frontend_contract() {
        let info = select_release("0.3.4", &[release("0.3.5")]).unwrap();
        let value = serde_json::to_value(info).unwrap();
        assert_eq!(value["currentVersion"], "0.3.4");
        assert_eq!(value["status"], "available");
        assert_eq!(value["version"], "0.3.5");
        assert!(value["releaseUrl"].as_str().unwrap().ends_with("/v0.3.5"));
        assert!(value["publishedAt"].is_string());
        assert_eq!(value.as_object().unwrap().len(), 7);
    }

    #[test]
    #[ignore = "opt-in read-only check against the public GitHub Releases API"]
    fn live_public_release_check() {
        let info = tauri::async_runtime::block_on(check_version("0.3.4", 0)).unwrap();
        assert_eq!(info.status, ReleaseStatus::Available);
        assert!(Version::parse(info.version.as_deref().unwrap()).unwrap() >= Version::new(0, 3, 5));
        assert!(info
            .release_url
            .as_deref()
            .unwrap()
            .starts_with(&format!("{REPOSITORY_URL}/releases/tag/")));
        println!(
            "release_check_live status={} version={} prerelease={}",
            info.status.label(),
            info.version.unwrap(),
            info.prerelease
        );
    }
}
