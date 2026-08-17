use std::path::Path;

pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token = windows::Win32::Foundation::HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
                let mut elevation = TOKEN_ELEVATION::default();
                let mut returned = 0u32;
                let size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;

                let success = GetTokenInformation(
                    token,
                    TokenElevation,
                    Some(&mut elevation as *mut _ as *mut _),
                    size,
                    &mut returned,
                )
                .is_ok();

                let _ = CloseHandle(token);
                if success {
                    return elevation.TokenIsElevated != 0;
                }
            }
        }
        false
    }

    #[cfg(not(windows))]
    {
        false
    }
}

pub fn restart_as_admin() -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let verb: Vec<u16> = std::ffi::OsStr::new("runas")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let file: Vec<u16> = current_exe
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            let res = ShellExecuteW(
                None,
                PCWSTR::from_raw(verb.as_ptr()),
                PCWSTR::from_raw(file.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            );

            if (res.0 as usize) > 32 {
                std::process::exit(0);
            } else {
                Err(format!(
                    "ShellExecuteW failed with code: {}",
                    res.0 as usize
                ))
            }
        }
    }

    #[cfg(not(windows))]
    {
        log::info!(
            "Restart as admin requested on non-windows for {:?}",
            current_exe
        );
        Ok(())
    }
}

pub fn check_write_permission(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let test_path = path.join(".wuwaid_perm_probe");
    if std::fs::write(&test_path, b"probe").is_ok() {
        let _ = std::fs::remove_file(test_path);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_elevated_smoke() {
        let _ = is_elevated();
    }

    #[test]
    fn test_check_write_permission() {
        let tmp = tempdir().unwrap();
        assert!(check_write_permission(tmp.path()));
    }
}
