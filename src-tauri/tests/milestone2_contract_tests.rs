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
    let mount_dir = resource_version.join("Mount");
    let official_dir = resource_version.join("Lang_en").join("Base");
    fs::create_dir_all(&mount_dir).unwrap();
    fs::create_dir_all(&official_dir).unwrap();
    fs::write(resource_version.join("ResManifest"), b"manifest").unwrap();
    let official_pak = official_dir.join("pakchunk10-WindowsNoEditor.pak");
    let official_sig = official_dir.join("pakchunk10-WindowsNoEditor.sig");
    fs::write(&official_pak, b"OFFICIAL_RESOURCE_PAK").unwrap();
    fs::write(&official_sig, b"OFFICIAL_RESOURCE_SIG").unwrap();
    fs::write(
        mount_dir.join("MountLang_en.txt"),
        format!(
            "::Mount::\nLang_en/Base/pakchunk10-WindowsNoEditor,4,{},{},,\n::Del::\n",
            installer::compute_sha1(&official_pak).unwrap(),
            installer::compute_sha1(&official_sig).unwrap(),
        ),
    )
    .unwrap();

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
fn signature_bypass_migrates_legacy_pak_without_marker() {
    let (_temp, game) = setup_game();
    let source = game.join("new.pak");
    let legacy = signature::get_signature_bypass_pak_path(&game);
    release_like_pak(&source);
    release_like_pak(&legacy);

    installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &source,
        None,
    )
    .expect("canonical legacy target must be replaceable");

    assert!(installer::validate_installed_signature_bypass(&game).unwrap());
    assert!(!signature::get_signature_bypass_marker_path(&game).exists());
    assert_eq!(fs::read(&legacy).unwrap(), fs::read(&source).unwrap());
}

#[test]
fn transaction_switches_methods_and_replaces_canonical_targets() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    let loader_source = game.join("source.dll");
    let unrelated = game.join("Client").join("Content").join("Paks").join("user-file.txt");
    release_like_pak(&pak_source);
    fs::write(&loader_source, b"loader bytes").unwrap();
    fs::write(&unrelated, b"do not touch").unwrap();

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
    assert!(report.preserved.is_empty(), "unexpected preserved paths: {report:?}");
    assert!(!foreign.exists());
    assert_eq!(fs::read(&unrelated).unwrap(), b"do not touch");

    let bypass = signature::get_signature_bypass_pak_path(&game);
    fs::write(&bypass, b"foreign pak").unwrap();
    installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &pak_source,
        None,
    )
    .unwrap();
    assert!(installer::validate_installed_signature_bypass(&game).unwrap());
    assert_eq!(fs::read(&bypass).unwrap(), fs::read(&pak_source).unwrap());
}

#[test]
fn cleanup_removes_resource_artifacts_across_all_versions() {
    let (_temp, game) = setup_game();
    let old_version = game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.5.0");
    let old_patch = old_version.join("Patch").join(installer::PATCH_FOLDER_NAME);
    let old_mount = old_version.join("Mount");
    fs::create_dir_all(&old_patch).unwrap();
    fs::create_dir_all(&old_mount).unwrap();
    let old_pak = old_patch.join(installer::PATCH_PAK_FILE_NAME);
    let old_sig = old_patch.join(installer::PATCH_SIG_FILE_NAME);
    let old_marker = old_patch.join(installer::OWNER_MARKER_FILE_NAME);
    let old_mount_file = old_mount.join(installer::MOUNT_FILE_NAME);
    fs::write(&old_pak, b"old patch").unwrap();
    fs::write(&old_sig, b"old signature").unwrap();
    fs::write(&old_marker, b"legacy marker").unwrap();
    fs::write(&old_mount_file, b"old mount").unwrap();

    let report = installer::cleanup_owned_artifacts(&game).unwrap();

    assert!(report.failures.is_empty(), "unexpected failures: {report:?}");
    assert!(report.preserved.is_empty(), "unexpected preserved paths: {report:?}");
    assert!(!old_pak.exists());
    assert!(!old_sig.exists());
    assert!(!old_marker.exists());
    assert!(!old_mount_file.exists());
    assert!(game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.6.0")
        .join("ResManifest")
        .exists());
}

#[test]
fn non_file_canonical_target_blocks_install_and_rolls_back() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    let loader_dll = signature::get_loader_dll_path(&game);
    fs::create_dir(&loader_dll).unwrap();

    let error = installer::install_patch_transaction(
        &game,
        InstallMethod::SignatureBypass,
        &pak_source,
        None,
    )
    .unwrap_err();

    assert!(error.contains("cleanup_partial_failure"));
    assert!(error.contains("preserved"));
    assert!(!signature::get_signature_bypass_pak_path(&game).exists());
    assert!(loader_dll.is_dir());
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
