use crate::validation::{directory_path, reject_reparse_points, same_directory};
use std::path::{Path, PathBuf};
use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, HWND};
use windows::Win32::Storage::FileSystem::GetDriveTypeW;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    KEY_WOW64_64KEY, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FileOpenDialog, IFileOpenDialog, IShellItem,
    SHCreateItemFromParsingName, SHGetKnownFolderPath, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, KF_FLAG_DEFAULT, SIGDN_FILESYSPATH,
};

const PRODUCT_KEY: &str = r"Software\MrPhotato\Remote Coding";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Remote Coding";
const INVALID_REGISTRATION: &str =
    "现有安装信息不完整，请先在 Windows 应用列表中修复或卸载无线麦。";

#[derive(Clone, Debug)]
pub struct Registration {
    pub directory: PathBuf,
    pub version: String,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn read_registry(key_name: &str, value_name: &str) -> Result<Option<String>, String> {
    let mut key = HKEY::default();
    let name = wide(key_name);
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(name.as_ptr()),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut key,
        )
    };
    if result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND {
        return Ok(None);
    }
    if result.is_err() {
        return Err("无法读取已有安装信息，请稍后重试。".into());
    }
    let value = wide(value_name);
    let mut data = [0u16; 4096];
    let mut bytes = std::mem::size_of_val(&data) as u32;
    let mut kind = REG_VALUE_TYPE::default();
    let result = unsafe {
        let result = RegQueryValueExW(
            key,
            PCWSTR(value.as_ptr()),
            None,
            Some(&mut kind),
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        );
        let _ = RegCloseKey(key);
        result
    };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if result.is_err() || kind != REG_SZ || bytes == 0 || bytes % 2 != 0 {
        return Err(INVALID_REGISTRATION.into());
    }
    let count = bytes as usize / 2;
    if count > data.len() || data[count - 1] != 0 || data[..count - 1].contains(&0) {
        return Err(INVALID_REGISTRATION.into());
    }
    String::from_utf16(&data[..count - 1])
        .map(Some)
        .map_err(|_| INVALID_REGISTRATION.into())
}

fn required_registry(value_name: &str) -> Result<String, String> {
    read_registry(UNINSTALL_KEY, value_name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| INVALID_REGISTRATION.into())
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

/// Read only this product's current-user registration. Missing or inconsistent
/// ownership information is not treated as permission to overwrite a folder.
pub fn registration() -> Result<Option<Registration>, String> {
    let stored_path = read_registry(PRODUCT_KEY, "")?;
    let uninstall = read_registry(UNINSTALL_KEY, "UninstallString")?;
    if stored_path.is_none() && uninstall.is_none() {
        return Ok(None);
    }
    let directory = directory_path(stored_path.as_deref().ok_or(INVALID_REGISTRATION)?)?;
    let registered_location = directory_path(unquote(&required_registry("InstallLocation")?))?;
    let uninstall = uninstall.ok_or(INVALID_REGISTRATION)?;
    if required_registry("Publisher")? != "MrPhotato"
        || required_registry("DisplayName")? != "Remote Coding"
        || required_registry("MainBinaryName")? != "remote-coding.exe"
        || !same_directory(&directory, &registered_location)
        || !same_directory(
            Path::new(unquote(&uninstall)),
            &directory.join("uninstall.exe"),
        )
    {
        return Err(INVALID_REGISTRATION.into());
    }
    reject_reparse_points(&directory)?;
    for name in ["remote-coding.exe", "uninstall.exe"] {
        let file = directory.join(name);
        reject_reparse_points(&file)?;
        if !file.is_file() {
            return Err(INVALID_REGISTRATION.into());
        }
    }
    Ok(Some(Registration {
        directory,
        version: required_registry("DisplayVersion")?,
    }))
}

pub fn default_directory() -> Result<PathBuf, String> {
    unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None)
            .map_err(|_| "无法读取默认安装位置。".to_owned())?;
        let value = raw.to_string();
        CoTaskMemFree(Some(raw.0.cast()));
        let value = value.map_err(|_| "默认安装位置无效。".to_owned())?;
        directory_path(&format!("{value}\\SayAll"))
    }
}

pub fn check_local_writable_directory(path: &Path) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    let text = path.to_string_lossy();
    let root = wide(&text[..3]); // directory_path has already validated C:\ syntax.
                                 // Fixed and removable disks are local; network mappings and optical media
                                 // are not suitable installation destinations for this per-user installer.
    if !matches!(unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) }, 2 | 3 | 6) {
        return Err("请选择本地磁盘上的文件夹。".into());
    }
    let parent = path
        .ancestors()
        .find(|part| part.exists())
        .filter(|part| part.is_dir())
        .ok_or("安装位置的上级文件夹不存在。")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let probe = parent.join(format!(
        ".sayall-setup-write-{}-{nonce}",
        std::process::id()
    ));
    // This exclusively-created empty probe disappears when its handle closes.
    // It never overwrites or deletes a pre-existing file.
    let _probe = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .custom_flags(0x0400_0000) // FILE_FLAG_DELETE_ON_CLOSE
        .open(probe)
        .map_err(|_| "当前用户无法写入这个位置，请选择其他文件夹。".to_owned())?;
    Ok(())
}

pub fn browse_directory(initial: PathBuf, owner: isize) -> Result<Option<String>, String> {
    // The calling command runs on a dedicated blocking thread. No application
    // window focus manipulation, synthetic input, or global hotkey is involved.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|_| "无法打开文件夹选择器，请直接输入安装位置。".to_owned())?;
    let result = (|| unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|_| "无法打开文件夹选择器，请直接输入安装位置。".to_owned())?;
        let title = wide("选择无线麦的安装位置");
        dialog
            .SetTitle(PCWSTR(title.as_ptr()))
            .map_err(|_| "无法设置文件夹选择器。".to_owned())?;
        let options = dialog
            .GetOptions()
            .map_err(|_| "无法设置文件夹选择器。".to_owned())?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|_| "无法设置文件夹选择器。".to_owned())?;
        if let Some(existing) = initial.ancestors().find(|path| path.is_dir()) {
            let path = wide(&existing.to_string_lossy());
            if let Ok(item) =
                SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(path.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&item);
            }
        }
        if let Err(error) = dialog.Show(Some(HWND(owner as *mut _))) {
            if error.code() == HRESULT(0x8007_04C7_u32 as i32) {
                return Ok(None);
            }
            return Err("文件夹选择器未能打开，请直接输入安装位置。".into());
        }
        let item = dialog
            .GetResult()
            .map_err(|_| "无法读取所选文件夹。".to_owned())?;
        let raw = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| "无法读取所选文件夹。".to_owned())?;
        let path = raw.to_string();
        CoTaskMemFree(Some(raw.0.cast()));
        path.map(Some).map_err(|_| "所选文件夹的路径无效。".into())
    })();
    unsafe { CoUninitialize() };
    result
}
