use crate::engine::path::*;
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_sig_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(SIG_FILE_NAME)
}

pub fn get_sig_backup_path(game_path: &Path) -> PathBuf {
    get_pak_dir(game_path).join(SIG_BACKUP_NAME)
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

/// Restores a signature left behind by an older unsupported launcher method.
pub fn restore_sig(game_path: &Path) -> std::io::Result<bool> {
    let sig = get_sig_path(game_path);
    let backup = get_pak_dir(game_path).join(SIG_BACKUP_NAME);

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
