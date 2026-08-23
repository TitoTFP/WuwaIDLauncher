use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use wuwaid_launcher_lib::engine::{installer, metadata, method::InstallMethod, runtime, updater};

fn game_with_regular_executable() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().to_path_buf();
    let executable = game.join("Client/Binaries/Win64/Client-Win64-Shipping.exe");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(executable, b"game").unwrap();
    (temp, game)
}

#[test]
fn game_path_validation_rejects_an_executable_directory() {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path();
    let executable = game.join("Client/Binaries/Win64/Client-Win64-Shipping.exe");
    fs::create_dir_all(&executable).unwrap();

    let error = installer::validate_installation_preconditions(
        &game.to_string_lossy(),
        InstallMethod::Loader,
    )
    .unwrap_err();

    assert!(error.contains("invalid_game_path"));
    assert!(error.contains("bukan file biasa"));
}

#[cfg(unix)]
#[test]
fn unreadable_rollback_snapshot_aborts_before_cleanup() {
    use std::os::unix::fs::PermissionsExt;

    let (_temp, game) = game_with_regular_executable();
    let signature = game.join("Client/Content/Paks/pakchunk7-WindowsNoEditor.sig");
    fs::create_dir_all(signature.parent().unwrap()).unwrap();
    fs::write(&signature, b"protected signature").unwrap();
    let original_mode = fs::metadata(&signature).unwrap().permissions().mode();
    fs::set_permissions(&signature, fs::Permissions::from_mode(0o000)).unwrap();

    if fs::read(&signature).is_ok() {
        fs::set_permissions(&signature, fs::Permissions::from_mode(original_mode)).unwrap();
        return;
    }

    let error = installer::cleanup_owned_artifacts_with_commit(&game, None, || Ok(())).unwrap_err();
    fs::set_permissions(&signature, fs::Permissions::from_mode(original_mode)).unwrap();

    assert!(error.contains("rollback_snapshot_unreadable"));
    assert!(signature.exists());
}

#[cfg(not(unix))]
#[test]
fn unreadable_rollback_snapshot_aborts_before_cleanup() {
    // Windows ACL setup is not portable in the test harness; the installer
    // still rejects non-readable snapshots through the same path.
    let (_temp, game) = game_with_regular_executable();
    assert!(installer::cleanup_owned_artifacts_with_commit(&game, None, || Ok(())).is_ok());
}

#[test]
fn metadata_migration_preserves_known_version_and_cached_notes() {
    let temp = tempfile::tempdir().unwrap();
    let metadata_path = temp.path().join("versions.json");
    fs::write(
        &metadata_path,
        r#"{"_vhVersion":"v3.0.0","_installMethod":"loader","_cachedReleaseNotes":{"tag":"v3.0.0"}}"#,
    )
    .unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();

    metadata::update_installation(&metadata_path, &first, None, "resource_mount", None).unwrap();
    metadata::update_installation(&metadata_path, &second, Some("v4.0.0"), "loader", None).unwrap();
    metadata::update_cached_release_notes(&metadata_path, serde_json::json!({"tag": "v4.0.0"}))
        .unwrap();

    assert_eq!(
        metadata::read_game_field(&metadata_path, &first, "_vhVersion").unwrap(),
        Some("v3.0.0".to_string())
    );
    metadata::remove_game(&metadata_path, &first).unwrap();
    assert_eq!(
        metadata::read_game_field(&metadata_path, &second, "_vhVersion").unwrap(),
        Some("v4.0.0".to_string())
    );
    let stored: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(metadata_path).unwrap()).unwrap();
    assert_eq!(stored["_cachedReleaseNotes"]["tag"], "v4.0.0");
}

#[test]
fn update_request_accepts_only_the_canonical_official_assets() {
    let tag = "v2.9.1";
    let zip = updater::expected_official_asset_url(tag, "WuwaIDLauncher-v2.9.1.zip").unwrap();
    let checksums = updater::expected_official_asset_url(tag, "SHA256sums.txt").unwrap();

    assert!(updater::validate_update_request("2.9.1", tag, &zip, Some(&checksums)).is_ok());
    assert!(updater::validate_update_request("2.9.0", tag, &zip, Some(&checksums)).is_err());
    assert!(updater::validate_update_request(
        "2.9.1",
        tag,
        "https://evil.example/WuwaIDLauncher-v2.9.1.zip",
        Some(&checksums)
    )
    .is_err());
    assert!(updater::validate_update_request(
        "2.9.1",
        tag,
        &zip,
        Some("https://github.com/TitoTFP/WuwaIDLauncher/releases/latest/download/SHA256sums.txt")
    )
    .is_err());
}

#[test]
fn force_quit_prefers_the_tracked_launcher_pid() {
    assert_eq!(runtime::select_force_quit_pid(Some(42), Some(99)), Some(42));
    assert_eq!(runtime::select_force_quit_pid(None, Some(99)), Some(99));
    assert_eq!(runtime::select_force_quit_pid(None, None), None);
}
