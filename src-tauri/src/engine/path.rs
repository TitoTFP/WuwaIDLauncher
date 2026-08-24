use std::path::{Path, PathBuf};

pub const GAME_EXE_RELATIVE: &str = if cfg!(windows) {
    "Client\\Binaries\\Win64\\Client-Win64-Shipping.exe"
} else {
    "Client/Binaries/Win64/Client-Win64-Shipping.exe"
};

pub const PAK_FOLDER_RELATIVE: &str = if cfg!(windows) {
    "Client\\Content\\Paks"
} else {
    "Client/Content/Paks"
};

pub const PAK_FILE_NAME: &str = "pakchunk0-ID-WindowsNoEditor_1000_P.pak";
pub const SIG_FILE_NAME: &str = "pakchunk7-WindowsNoEditor.sig";
pub const SIG_BACKUP_NAME: &str = "pakchunk7-WindowsNoEditor_backup.sig";
pub const WINHTTP_LOADER_NAME: &str = "winhttp.dll";
pub const MOD_FOLDER_NAME: &str = "wuwaIndonesia";

/// Validates whether a candidate directory is a valid Wuthering Waves game directory.
/// Checks for the existence of `Client/Binaries/Win64/Client-Win64-Shipping.exe`.
pub fn validate_game_path(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    let direct_exe = dir.join(GAME_EXE_RELATIVE);
    if direct_exe.is_file() {
        return Some(dir.to_path_buf());
    }

    let sub_game_dir = dir.join("Wuthering Waves Game");
    let sub_exe = sub_game_dir.join(GAME_EXE_RELATIVE);
    if sub_exe.is_file() {
        return Some(sub_game_dir);
    }

    None
}

/// Normalizes a path string, checking parent and child directories for the game exe.
pub fn normalize_game_path(input_path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(input_path);
    if let Some(valid) = validate_game_path(&p) {
        return Some(valid);
    }

    // Traverse upwards to see if user selected a child folder (e.g. Client/Binaries)
    let mut curr = p.parent();
    while let Some(parent) = curr {
        if let Some(valid) = validate_game_path(parent) {
            return Some(valid);
        }
        curr = parent.parent();
    }

    None
}

pub fn get_pak_dir(game_path: &Path) -> PathBuf {
    game_path.join(PAK_FOLDER_RELATIVE)
}

pub fn get_binary_dir(game_path: &Path) -> PathBuf {
    game_path.join("Client").join("Binaries").join("Win64")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, File};
    use tempfile::tempdir;

    #[test]
    fn test_validate_valid_game_path() {
        let tmp = tempdir().unwrap();
        let exe_dir = tmp.path().join("Client").join("Binaries").join("Win64");
        create_dir_all(&exe_dir).unwrap();
        File::create(exe_dir.join("Client-Win64-Shipping.exe")).unwrap();

        let result = validate_game_path(tmp.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), tmp.path());
    }

    #[test]
    fn test_validate_nested_game_path() {
        let tmp = tempdir().unwrap();
        let nested = tmp.path().join("Wuthering Waves Game");
        let exe_dir = nested.join("Client").join("Binaries").join("Win64");
        create_dir_all(&exe_dir).unwrap();
        File::create(exe_dir.join("Client-Win64-Shipping.exe")).unwrap();

        let result = validate_game_path(tmp.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), nested);
    }

    #[test]
    fn test_validate_invalid_path() {
        let tmp = tempdir().unwrap();
        assert!(validate_game_path(tmp.path()).is_none());
    }

    #[test]
    fn test_validate_rejects_directory_named_as_game_executable() {
        let tmp = tempdir().unwrap();
        let exe_dir = tmp.path().join("Client").join("Binaries").join("Win64");
        create_dir_all(exe_dir.join("Client-Win64-Shipping.exe")).unwrap();

        assert!(validate_game_path(tmp.path()).is_none());
    }
}
