use std::path::{Path, PathBuf};

pub const GAME_EXE_RELATIVE: &str = if cfg!(windows) {
    "Client\\Binaries\\Win64\\Client-Win64-Shipping.exe"
} else {
    "Client/Binaries/Win64/Client-Win64-Shipping.exe"
};

/// Entrypoint the official launcher starts. This root bootstrap hands off to
/// [`GAME_EXE_RELATIVE`], which aborts with `kuro: Use launcher to start game!`
/// unless a `-krqlv` tier argument is supplied. A bare file name needs no
/// `cfg!` separator split, unlike the two path constants below it.
pub const LAUNCH_EXE_RELATIVE: &str = "Wuthering Waves.exe";

/// Quality tiers in Kuro's own preference order (`bundleRecommendPriority`).
pub const QUALITY_LEVELS: [&str; 3] = ["UHD", "HD", "SD"];

/// Kuro's `defaultBundleName`, used when no tier is installed.
pub const DEFAULT_QUALITY_LEVEL: &str = "HD";

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

pub fn get_launch_exe(game_path: &Path) -> PathBuf {
    game_path.join(LAUNCH_EXE_RELATIVE)
}

pub fn get_quality_dir(game_path: &Path, quality_level: &str) -> PathBuf {
    game_path.join("Client").join("Content").join(quality_level)
}

/// Builds the `-krqlv` argument for a tier. Kuro's own launcher and Steam send
/// the lowercase token (`-krqlv=hd`) while the directory the game mounts is
/// uppercase (`Client/Content/HD/`), so the argument stays lowercase.
pub fn quality_level_argument(quality_level: &str) -> String {
    format!("-krqlv={}", quality_level.to_ascii_lowercase())
}

/// Quality tiers whose paks are actually present under `Client/Content/<TIER>/`,
/// in Kuro's preference order. `-krqlv` selects that directory, so a tier with
/// no installed paks cannot be launched.
pub fn installed_quality_levels(game_path: &Path) -> Vec<&'static str> {
    QUALITY_LEVELS
        .into_iter()
        .filter(|level| installed_quality_bytes(&get_quality_dir(game_path, level)) > 0)
        .collect()
}

/// Resolves the tier to launch with. An explicit preference wins when that tier
/// is installed; otherwise the most complete installed tier is used, falling
/// back to Kuro's `defaultBundleName` when nothing is detected.
pub fn resolve_quality_level(game_path: &Path, preference: &str) -> String {
    let installed = installed_quality_levels(game_path);
    if let Some(level) = canonical_quality_level(preference) {
        if installed.contains(&level) {
            return level.to_string();
        }
    }

    // `max_by_key` yields the *last* maximum, so iterate in reverse: a tie then
    // resolves to the tier Kuro prefers rather than to the lowest one.
    installed
        .into_iter()
        .rev()
        .max_by_key(|level| installed_quality_bytes(&get_quality_dir(game_path, level)))
        .unwrap_or(DEFAULT_QUALITY_LEVEL)
        .to_string()
}

pub fn canonical_quality_level(value: &str) -> Option<&'static str> {
    QUALITY_LEVELS
        .into_iter()
        .find(|level| level.eq_ignore_ascii_case(value.trim()))
}

fn installed_quality_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("pakchunk") && name.ends_with(".pak")
        })
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum()
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

    fn write_quality_pak(game_path: &Path, level: &str, bytes: usize) {
        let dir = get_quality_dir(game_path, level);
        create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("pakchunk1-{level}-WindowsNoEditor.pak")),
            vec![0u8; bytes],
        )
        .unwrap();
    }

    #[test]
    fn quality_level_argument_uses_the_lowercase_token() {
        assert_eq!(quality_level_argument("SD"), "-krqlv=sd");
        assert_eq!(quality_level_argument("HD"), "-krqlv=hd");
        assert_eq!(quality_level_argument("UHD"), "-krqlv=uhd");
    }

    #[test]
    fn quality_level_argument_selects_the_uppercase_directory() {
        let tmp = tempdir().unwrap();
        assert_eq!(
            get_quality_dir(tmp.path(), "HD"),
            tmp.path().join("Client").join("Content").join("HD")
        );
    }

    #[test]
    fn no_installed_tier_falls_back_to_kuro_default() {
        let tmp = tempdir().unwrap();
        assert!(installed_quality_levels(tmp.path()).is_empty());
        assert_eq!(
            resolve_quality_level(tmp.path(), "auto"),
            DEFAULT_QUALITY_LEVEL
        );
    }

    #[test]
    fn tier_directory_without_paks_is_not_offered() {
        let tmp = tempdir().unwrap();
        create_dir_all(get_quality_dir(tmp.path(), "UHD")).unwrap();
        assert!(installed_quality_levels(tmp.path()).is_empty());
        assert_eq!(
            resolve_quality_level(tmp.path(), "UHD"),
            DEFAULT_QUALITY_LEVEL
        );
    }

    #[test]
    fn auto_preference_uses_the_installed_tier() {
        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "HD", 64);
        assert_eq!(installed_quality_levels(tmp.path()), vec!["HD"]);
        assert_eq!(resolve_quality_level(tmp.path(), "auto"), "HD");
    }

    #[test]
    fn auto_preference_keeps_the_most_complete_install() {
        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "HD", 4096);
        write_quality_pak(tmp.path(), "UHD", 8);
        assert_eq!(resolve_quality_level(tmp.path(), "auto"), "HD");
    }

    #[test]
    fn explicit_installed_preference_wins_over_auto() {
        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "HD", 64);
        write_quality_pak(tmp.path(), "UHD", 4096);
        assert_eq!(resolve_quality_level(tmp.path(), "auto"), "UHD");
        assert_eq!(resolve_quality_level(tmp.path(), "HD"), "HD");
        assert_eq!(resolve_quality_level(tmp.path(), " uhd "), "UHD");
    }

    #[test]
    fn equal_installs_resolve_to_the_preferred_tier() {
        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "UHD", 64);
        write_quality_pak(tmp.path(), "HD", 64);
        write_quality_pak(tmp.path(), "SD", 64);
        // `max_by_key` without `.rev()` would pick the last maximum, i.e. `SD`.
        assert_eq!(resolve_quality_level(tmp.path(), "auto"), "UHD");

        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "HD", 64);
        write_quality_pak(tmp.path(), "SD", 64);
        assert_eq!(resolve_quality_level(tmp.path(), "auto"), "HD");
    }

    #[test]
    fn preference_without_installed_paks_is_ignored() {
        let tmp = tempdir().unwrap();
        write_quality_pak(tmp.path(), "HD", 64);
        assert_eq!(resolve_quality_level(tmp.path(), "UHD"), "HD");
        assert_eq!(resolve_quality_level(tmp.path(), "nonsense"), "HD");
    }

    #[test]
    fn only_known_tiers_are_canonical() {
        assert_eq!(canonical_quality_level("HD"), Some("HD"));
        assert_eq!(canonical_quality_level("auto"), None);
        assert_eq!(canonical_quality_level("4k"), None);
    }
}
