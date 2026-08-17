use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;
use wuwaid_launcher_lib::engine::installer::{
    deploy_resource_mount, probe_resource_mount, validate_pak_file,
};
use wuwaid_launcher_lib::engine::method::InstallMethod;
use wuwaid_launcher_lib::engine::pak;
use wuwaid_launcher_lib::engine::patch_status::{
    classify_installation, resolve_patch_status, LocalPatchState, PatchStatus,
};
use wuwaid_launcher_lib::engine::settings::{normalize_settings_json, LauncherSettings};
use wuwaid_launcher_lib::engine::signature;

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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn setup_mock_game() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempdir().unwrap();
    let game = tmp.path().join("Wuthering Waves");
    let paks = game.join("Client").join("Content").join("Paks");
    let resources = game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.6.0");
    let binaries = game.join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&paks).unwrap();
    fs::create_dir_all(&resources).unwrap();
    fs::create_dir_all(&binaries).unwrap();
    fs::write(resources.join("ResManifest"), b"manifest").unwrap();
    fs::write(binaries.join("Client-Win64-Shipping.exe"), b"game").unwrap();
    fs::write(signature::get_sig_path(&game), b"original signature").unwrap();
    (tmp, game)
}

#[test]
fn canonical_install_methods_migrate_legacy_ids_and_reject_unknown_values() {
    assert_eq!(
        InstallMethod::parse("resource_mount").unwrap(),
        InstallMethod::ResourceMount
    );
    assert_eq!(
        InstallMethod::parse("loader").unwrap(),
        InstallMethod::Loader
    );
    assert_eq!(
        InstallMethod::parse("signature_bypass").unwrap(),
        InstallMethod::SignatureBypass
    );

    assert_eq!(
        InstallMethod::parse("method3").unwrap(),
        InstallMethod::ResourceMount
    );
    assert_eq!(
        InstallMethod::parse("method2").unwrap(),
        InstallMethod::Loader
    );
    assert_eq!(
        InstallMethod::parse("method1").unwrap(),
        InstallMethod::SignatureBypass
    );
    assert!(InstallMethod::parse("unknown").is_err());
}

#[test]
fn settings_recover_invalid_json_and_normalize_partial_legacy_values() {
    let invalid = normalize_settings_json("{");
    assert!(invalid.repaired);
    assert_eq!(invalid.settings, LauncherSettings::default());
    assert!(!invalid.diagnostics.is_empty());

    let partial = normalize_settings_json(
        r#"{
            "gamePath": "C:/does/not/exist",
            "installMethod": "method1",
            "launcherVisualMode": "cinema",
            "dx11": "yes",
            "autoCheckUpdate": false,
            "bgmVolume": 4,
            "bgmEnabled": true
        }"#,
    );
    assert!(partial.repaired);
    assert_eq!(partial.settings.game_path, "");
    assert_eq!(
        partial.settings.install_method,
        InstallMethod::SignatureBypass
    );
    assert_eq!(partial.settings.launcher_visual_mode, "full");
    assert!(!partial.settings.dx11);
    assert!(!partial.settings.auto_check_update);
    assert_eq!(partial.settings.bgm_volume, 1.0);
    assert!(partial.settings.bgm_enabled);
}

#[test]
fn settings_persistence_keeps_valid_game_path_and_canonical_schema() {
    let (_tmp, game) = setup_mock_game();
    let raw = serde_json::json!({
        "gamePath": game,
        "installMethod": "method3",
        "launcherVisualMode": "light",
        "dx11": true,
        "autoCheckUpdate": false,
        "bgmVolume": 0.7,
        "bgmEnabled": false,
    })
    .to_string();

    let normalized = normalize_settings_json(&raw);
    assert!(!normalized.repaired);
    assert_eq!(
        normalized.settings.install_method,
        InstallMethod::ResourceMount
    );
    assert_eq!(normalized.settings.launcher_visual_mode, "light");

    let encoded = serde_json::to_string(&normalized.settings).unwrap();
    assert!(encoded.contains("resource_mount"));
    assert!(!encoded.contains("method3"));
    let round_trip = normalize_settings_json(&encoded);
    assert_eq!(round_trip.settings, normalized.settings);
}

#[test]
fn patch_status_distinguishes_missing_corrupt_owned_and_valid_installations() {
    let (_tmp, game) = setup_mock_game();

    assert_eq!(
        classify_installation(&game, InstallMethod::ResourceMount).unwrap(),
        LocalPatchState::NotInstalled
    );

    let resource_plan = probe_resource_mount(&game).unwrap();
    let source = game.join("source.pak");
    release_like_pak(&source);
    deploy_resource_mount(&resource_plan, &source, &game).unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::ResourceMount).unwrap(),
        LocalPatchState::Ready
    );

    fs::write(&resource_plan.pak_path, b"corrupt").unwrap();
    assert!(!validate_pak_file(&resource_plan.pak_path).unwrap());
    assert_eq!(
        classify_installation(&game, InstallMethod::ResourceMount).unwrap(),
        LocalPatchState::Invalid
    );

    fs::remove_file(&resource_plan.pak_path).unwrap();
    fs::remove_file(&resource_plan.sig_path).unwrap();
    fs::remove_file(&resource_plan.owner_marker_path).unwrap();
    fs::remove_file(&resource_plan.mount_path).unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::Loader).unwrap(),
        LocalPatchState::NotInstalled
    );

    let loader_pak = signature::get_loader_pak_path(&game);
    let loader_dll = signature::get_loader_dll_path(&game);
    release_like_pak(&loader_pak);
    fs::write(&loader_dll, b"loader").unwrap();
    fs::write(
        signature::get_loader_marker_path(&game),
        "wuwaid-managed-loader:pak-sha256=invalid;loader-sha256=invalid",
    )
    .unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::Loader).unwrap(),
        LocalPatchState::Invalid
    );

    fs::remove_file(signature::get_loader_marker_path(&game)).unwrap();
    let loader_pak_hash =
        wuwaid_launcher_lib::engine::downloader::compute_sha256(&loader_pak).unwrap();
    let loader_hash = wuwaid_launcher_lib::engine::downloader::compute_sha256(&loader_dll).unwrap();
    fs::write(
        signature::get_loader_marker_path(&game),
        format!("wuwaid-managed-loader:pak-sha256={loader_pak_hash};loader-sha256={loader_hash}"),
    )
    .unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::Loader).unwrap(),
        LocalPatchState::Ready
    );

    fs::remove_file(loader_pak).unwrap();
    fs::remove_file(loader_dll).unwrap();
    fs::remove_file(signature::get_loader_marker_path(&game)).unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::SignatureBypass).unwrap(),
        LocalPatchState::NotInstalled
    );
    let bypass_pak = signature::get_signature_bypass_pak_path(&game);
    release_like_pak(&bypass_pak);
    let bypass_hash = wuwaid_launcher_lib::engine::downloader::compute_sha256(&bypass_pak).unwrap();
    fs::write(
        signature::get_signature_bypass_marker_path(&game),
        format!("wuwaid-managed-signature-bypass:sha256={bypass_hash}"),
    )
    .unwrap();
    assert_eq!(
        classify_installation(&game, InstallMethod::SignatureBypass).unwrap(),
        LocalPatchState::Ready
    );
}

#[test]
fn patch_status_uses_current_and_latest_versions() {
    assert_eq!(
        resolve_patch_status(
            LocalPatchState::Ready,
            Some("v3.5.1-id.3"),
            Some("v3.5.1-id.4")
        ),
        PatchStatus::NeedsUpdate
    );
    assert_eq!(
        resolve_patch_status(
            LocalPatchState::Ready,
            Some("v3.5.1-id.4"),
            Some("v3.5.1-id.4")
        ),
        PatchStatus::Ready
    );
    assert_eq!(
        resolve_patch_status(LocalPatchState::Ready, None, Some("v3.5.1-id.4")),
        PatchStatus::NeedsUpdate
    );
    assert_eq!(
        resolve_patch_status(LocalPatchState::NotInstalled, Some("v1"), Some("v2")),
        PatchStatus::NotInstalled
    );
}
