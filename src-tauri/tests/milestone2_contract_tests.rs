use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wuwaid_launcher_lib::engine::{
    installer,
    method::InstallMethod,
    pak,
    path,
    signature,
};

fn release_like_pak(path: &Path) {
    let bytes = pak::pack(
        "../../../",
        0,
        &[(
            "Content/Localization/id.txt".to_string(),
            b"Bahasa Indonesia".to_vec(),
        )],
    )
    .unwrap();
    fs::write(path, bytes).unwrap();
}

fn setup_game() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().to_path_buf();
    let exe_dir = game.join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&exe_dir).unwrap();
    fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();

    let paks = game.join("Client").join("Content").join("Paks");
    fs::create_dir_all(&paks).unwrap();
    fs::write(paks.join(path::SIG_FILE_NAME), b"ORIGINAL SIG").unwrap();

    let resource_version = game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.6.0");
    fs::create_dir_all(&resource_version).unwrap();
    fs::write(resource_version.join("ResManifest"), b"manifest").unwrap();

    (temp, game)
}

#[test]
fn preflight_normalizes_game_path_and_selects_method_target() {
    let (_temp, game) = setup_game();
    let nested = game
        .join("Client")
        .join("Binaries")
        .join("Win64")
        .to_string_lossy()
        .to_string();

    let normalized = installer::validate_installation_preconditions(
        &nested,
        InstallMethod::Loader,
    )
    .unwrap();
    assert_eq!(normalized, game);

    let invalid_dir = tempfile::tempdir().unwrap();
    let invalid = installer::validate_installation_preconditions(
        &invalid_dir.path().to_string_lossy(),
        InstallMethod::SignatureBypass,
    )
    .unwrap_err();
    assert!(invalid.contains("invalid_game_path"));
}

#[test]
fn preflight_rejects_resource_mount_without_resource_manifest() {
    let (_temp, game) = setup_game();
    fs::remove_dir_all(game.join("Client").join("Saved").join("Resources")).unwrap();

    let error = installer::validate_installation_preconditions(
        &game.to_string_lossy(),
        InstallMethod::ResourceMount,
    )
    .unwrap_err();
    assert!(error.contains("resource_not_ready"));
}

#[test]
fn loader_transaction_requires_loader_and_leaves_no_partial_artifacts() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    let error = installer::install_patch_transaction(
        &game,
        InstallMethod::Loader,
        &pak_source,
        None,
    )
    .unwrap_err();
    assert!(error.contains("loader_source_missing"));
    assert!(!signature::get_loader_pak_path(&game).exists());
    assert!(!signature::get_loader_dll_path(&game).exists());
    assert!(!signature::get_loader_marker_path(&game).exists());
    assert!(!game.join("Client").join("Binaries").join("Win64").join(".wuwaid-transaction").exists());
}

#[test]
fn transaction_switches_owned_artifacts_and_preserves_foreign_targets() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    let loader_source = game.join("source.dll");
    release_like_pak(&pak_source);
    fs::write(&loader_source, b"loader bytes").unwrap();

    installer::install_patch_transaction(
        &game,
        InstallMethod::Loader,
        &pak_source,
        Some(&loader_source),
    )
    .unwrap();
    assert!(installer::validate_installed_loader(&game).unwrap());

    installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &pak_source,
        None,
    )
    .unwrap();
    assert!(installer::validate_installed_signature_bypass(&game).unwrap());
    assert!(!signature::get_loader_pak_path(&game).exists());
    assert!(!signature::get_loader_dll_path(&game).exists());

    let foreign = signature::get_loader_dll_path(&game);
    fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    fs::write(&foreign, b"foreign loader").unwrap();
    let report = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(report.preserved.iter().any(|path| path.ends_with("winhttp.dll")));
    assert_eq!(fs::read(&foreign).unwrap(), b"foreign loader");

    let bypass = signature::get_signature_bypass_pak_path(&game);
    fs::write(&bypass, b"foreign pak").unwrap();
    let error = installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &pak_source,
        None,
    )
    .unwrap_err();
    assert!(error.contains("target_conflict"));
    assert_eq!(fs::read(&bypass).unwrap(), b"foreign pak");
}

#[test]
fn repeated_cleanup_is_idempotent_and_reports_owned_artifacts() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &pak_source,
        None,
    )
    .unwrap();
    let first = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(!first.removed.is_empty());
    assert!(!signature::get_signature_bypass_pak_path(&game).exists());

    let second = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(second.removed.is_empty());
    assert!(second.failures.is_empty());
}
