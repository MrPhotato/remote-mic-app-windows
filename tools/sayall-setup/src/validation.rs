use std::path::{Path, PathBuf};

const INVALID_DIRECTORY: &str = "请选择一个有效的本地安装文件夹。";

/// A deliberately narrow grammar for the final, unquoted NSIS /D argument.
/// UNC, device paths and relative paths are never forwarded to an installer.
pub fn directory_path(input: &str) -> Result<PathBuf, String> {
    let value = input.replace('/', "\\");
    let bytes = value.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || value.encode_utf16().count() > 200
        || value
            .chars()
            .any(|c| c.is_control() || "\"<>|?*".contains(c))
    {
        return Err(INVALID_DIRECTORY.into());
    }
    let tail = &value[3..];
    if tail.is_empty() || tail.contains(':') {
        return Err(INVALID_DIRECTORY.into());
    }
    for component in tail.split('\\') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with([' ', '.'])
            || reserved_component(component)
        {
            return Err(INVALID_DIRECTORY.into());
        }
    }
    Ok(PathBuf::from(value))
}

fn reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

pub fn same_directory(left: &Path, right: &Path) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

pub fn enforce_existing_directory(requested: &Path, existing: Option<&Path>) -> Result<(), String> {
    if existing.is_some_and(|path| !same_directory(requested, path)) {
        return Err("已安装的无线麦会沿用原来的位置，保留现有配置。".into());
    }
    Ok(())
}

pub fn reject_reparse_points(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        match std::fs::symlink_metadata(part) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err("安装位置不能是快捷链接或重定向的文件夹。".into());
                    }
                }
                if metadata.file_type().is_symlink() {
                    return Err("安装位置不能是快捷链接或重定向的文件夹。".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("无法检查安装位置，请选择其他文件夹。".into()),
        }
    }
    Ok(())
}

pub fn validate_target(path: &Path, existing: Option<&Path>) -> Result<(), String> {
    enforce_existing_directory(path, existing)?;
    reject_reparse_points(path)?;
    match std::fs::metadata(path) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err("安装位置已有同名文件，请选择其他文件夹。".into());
        }
        Ok(_) if existing.is_none() => {
            let mut entries = std::fs::read_dir(path)
                .map_err(|_| "无法读取安装位置，请选择其他文件夹。".to_owned())?;
            if entries.next().is_some() {
                return Err("请选择一个空文件夹，以免覆盖已有文件。".into());
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("无法检查安装位置，请选择其他文件夹。".into()),
    }
    Ok(())
}

pub fn installer_arguments(directory: &Path, existing: bool, desktop: bool) -> String {
    format!(
        "/S{} /SAYALL_DESKTOP={} /D={}",
        if existing { " /UPDATE" } else { "" },
        u8::from(desktop),
        directory.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_local_directories_with_spaces_and_chinese() {
        for path in [
            r"D:\应用\无线麦 SayAll",
            r"C:\Users\A B\SayAll",
            "D:/Apps/SayAll",
        ] {
            assert!(directory_path(path).is_ok(), "{path}");
        }
    }

    #[test]
    fn rejects_ambiguous_nsis_and_windows_paths() {
        for path in [
            "",
            r"C:\",
            "C:/",
            r"C:SayAll",
            r"\SayAll",
            r"\\server\share\SayAll",
            r"\\?\C:\SayAll",
            r"\\.\C:\SayAll",
            r"C:\Apps\..\SayAll",
            r"C:\Apps\.\SayAll",
            r"C:\Apps\\SayAll",
            r"C:\Apps\SayAll\",
            r"C:\Apps\SayAll ",
            r"C:\Apps.\SayAll",
            r"C:\Apps\SayAll:stream",
            "C:\\Apps\\SayAll\" /R",
            "C:\\Apps\\SayAll\n/R",
            r"C:\Apps\NUL",
            r"C:\Apps\con.txt",
            r"C:\Apps\COM9",
            r"C:\Apps\LPT¹.txt",
        ] {
            assert!(directory_path(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn existing_install_cannot_be_relocated_by_the_frontend() {
        let installed = Path::new(r"D:\Apps\SayAll");
        assert!(enforce_existing_directory(Path::new(r"d:\apps\sayall"), Some(installed)).is_ok());
        assert!(enforce_existing_directory(Path::new(r"E:\Apps\SayAll"), Some(installed)).is_err());
    }

    #[test]
    fn nsis_directory_is_raw_and_last_without_automatic_restart() {
        assert_eq!(
            installer_arguments(Path::new(r"D:\应用\无线麦 SayAll"), true, false),
            r"/S /UPDATE /SAYALL_DESKTOP=0 /D=D:\应用\无线麦 SayAll"
        );
        assert_eq!(
            installer_arguments(Path::new(r"D:\Apps\SayAll"), false, true),
            r"/S /SAYALL_DESKTOP=1 /D=D:\Apps\SayAll"
        );
    }

    #[test]
    fn fresh_install_refuses_an_unowned_nonempty_directory() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sayall-setup-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("user-document.txt");
        std::fs::write(&marker, b"untouched").unwrap();
        assert!(validate_target(&root, None).is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), b"untouched");
        // Retain the inert fixture for inspection; project policy uses the
        // Recycle Bin rather than permanently deleting user-side test files.
    }
}
