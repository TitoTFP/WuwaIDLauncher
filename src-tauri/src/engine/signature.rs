use crate::engine::path::*;
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_sig_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(SIG_FILE_NAME)
}

pub fn get_sig_backup_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(SIG_BACKUP_NAME)
}

pub fn get_signature_bypass_pak_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(PAK_FILE_NAME)
}

pub fn get_loader_folder(game_path: &Path) -> PathBuf {
    get_binary_dir(game_path).join(MOD_FOLDER_NAME)
}

pub fn get_loader_pak_path(game_path: &Path) -> PathBuf {
    get_loader_folder(game_path).join(PAK_FILE_NAME)
}

pub fn get_loader_dll_path(game_path: &Path) -> PathBuf {
    get_binary_dir(game_path).join(WINHTTP_LOADER_NAME)
}

pub fn get_loader_marker_path(game_path: &Path) -> PathBuf {
    get_loader_folder(game_path).join(".wuwaid-managed-loader")
}

pub fn get_signature_bypass_marker_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(".wuwaid-managed-signature-bypass")
}

/// Creates a backup of the original signature file.
pub fn backup_sig(game_path: &Path) -> std::io::Result<bool> {
    let sig = get_sig_path(game_path);
    let backup = get_sig_backup_path(game_path);

    if sig.exists() && !backup.exists() {
        fs::copy(&sig, &backup)?;
        return Ok(true);
    }
    Ok(false)
}

/// Bypasses signature checking by moving the current .sig file to backup.
pub fn bypass_sig(game_path: &Path) -> std::io::Result<bool> {
    let sig = get_sig_path(game_path);
    let backup = get_sig_backup_path(game_path);

    // Match the release launcher: discard a stale backup when both files are
    // present, then move the current official signature into the backup.
    let _ = restore_sig(game_path)?;
    if sig.exists() {
        fs::rename(&sig, &backup)?;
        return Ok(true);
    }
    Ok(backup.exists())
}

/// Restores the original signature file from backup.
pub fn restore_sig(game_path: &Path) -> std::io::Result<bool> {
    let sig = get_sig_path(game_path);
    let backup = get_sig_backup_path(game_path);

    if backup.exists() {
        if sig.exists() {
            // An active signature is already restored; never overwrite it
            // with an older backup.
            fs::remove_file(&backup)?;
        } else {
            fs::rename(&backup, &sig)?;
        }
        return Ok(true);
    }
    Ok(false)
}

pub fn is_sig_bypassed(game_path: &Path) -> bool {
    let sig = get_sig_path(game_path);
    let backup = get_sig_backup_path(game_path);
    !sig.exists() && backup.exists()
}

pub fn delete_sig_backup(game_path: &Path) -> std::io::Result<()> {
    let backup = get_sig_backup_path(game_path);
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

pub fn delete_legacy_files(game_path: &Path) {
    let pak_dir = get_pak_dir(game_path);
    let bin_dir = get_binary_dir(game_path);

    // Delete legacy pak files
    let legacy_pak = pak_dir.join("WuWaID_99_P.pak");
    if legacy_pak.exists() {
        let _ = fs::remove_file(legacy_pak);
    }

    // Delete legacy mod folder
    let legacy_folder = bin_dir.join("wuwaVietHoa");
    if legacy_folder.exists() {
        let _ = fs::remove_dir_all(legacy_folder);
    }

    // Delete version loader
    let version_dll = bin_dir.join("version.dll");
    if version_dll.exists() {
        let _ = fs::remove_file(version_dll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_backup_and_restore_sig() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path();
        let pak_dir = get_pak_dir(game_dir);
        fs::create_dir_all(&pak_dir).unwrap();

        let sig_path = get_sig_path(game_dir);
        fs::write(&sig_path, b"SIG_DATA_ORIGINAL").unwrap();

        // Backup
        let res = backup_sig(game_dir).unwrap();
        assert!(res);
        assert!(get_sig_backup_path(game_dir).exists());

        // Restore
        let res2 = restore_sig(game_dir).unwrap();
        assert!(res2);
        assert!(sig_path.exists());
        assert!(!get_sig_backup_path(game_dir).exists());
    }

    #[test]
    fn test_bypass_and_restore_lifecycle() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path();
        let pak_dir = get_pak_dir(game_dir);
        fs::create_dir_all(&pak_dir).unwrap();

        let sig_path = get_sig_path(game_dir);
        fs::write(&sig_path, b"SIG_DATA_ORIGINAL").unwrap();

        // 1. Bypass (removes active .sig and preserves backup)
        assert!(bypass_sig(game_dir).unwrap());
        assert!(!sig_path.exists());
        assert!(get_sig_backup_path(game_dir).exists());
        assert!(is_sig_bypassed(game_dir));

        // 2. Restore (restores active .sig and removes backup)
        assert!(restore_sig(game_dir).unwrap());
        assert!(sig_path.exists());
        assert_eq!(fs::read_to_string(&sig_path).unwrap(), "SIG_DATA_ORIGINAL");
        assert!(!is_sig_bypassed(game_dir));
    }

    #[test]
    fn test_bypass_moves_current_sig_over_stale_backup() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path();
        let pak_dir = get_pak_dir(game_dir);
        fs::create_dir_all(&pak_dir).unwrap();

        let sig_path = get_sig_path(game_dir);
        let backup_path = get_sig_backup_path(game_dir);
        fs::write(&sig_path, b"CURRENT_SIG").unwrap();
        fs::write(&backup_path, b"STALE_SIG").unwrap();

        assert!(bypass_sig(game_dir).unwrap());
        assert!(!sig_path.exists());
        assert_eq!(fs::read(&backup_path).unwrap(), b"CURRENT_SIG");
    }

    #[test]
    fn test_restore_does_not_overwrite_active_sig_when_both_exist() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path();
        let pak_dir = get_pak_dir(game_dir);
        fs::create_dir_all(&pak_dir).unwrap();

        let sig_path = get_sig_path(game_dir);
        let backup_path = get_sig_backup_path(game_dir);
        fs::write(&sig_path, b"ACTIVE_SIG").unwrap();
        fs::write(&backup_path, b"STALE_SIG").unwrap();

        assert!(restore_sig(game_dir).unwrap());
        assert_eq!(fs::read(&sig_path).unwrap(), b"ACTIVE_SIG");
        assert!(!backup_path.exists());
    }

    #[test]
    fn test_delete_legacy_files() {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path();
        let pak_dir = get_pak_dir(game_dir);
        let bin_dir = get_binary_dir(game_dir);
        fs::create_dir_all(&pak_dir).unwrap();
        fs::create_dir_all(&bin_dir).unwrap();

        let legacy_pak = pak_dir.join("WuWaID_99_P.pak");
        let version_dll = bin_dir.join("version.dll");
        fs::write(&legacy_pak, b"legacy").unwrap();
        fs::write(&version_dll, b"dll").unwrap();

        delete_legacy_files(game_dir);
        assert!(!legacy_pak.exists());
        assert!(!version_dll.exists());
    }
}
