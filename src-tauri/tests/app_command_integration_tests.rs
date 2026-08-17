use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use wuwaid_launcher_lib::engine::atom_feed::{parse_atom_feed, ReleaseNoteEntry};
use wuwaid_launcher_lib::engine::downloader;
use wuwaid_launcher_lib::engine::installer::{
    deploy_resource_mount, probe_resource_mount, remove_all_owned_artifacts,
};
use wuwaid_launcher_lib::engine::pak;
use wuwaid_launcher_lib::engine::path::validate_game_path;
use wuwaid_launcher_lib::engine::signature;
fn release_like_pak(path: &std::path::Path) {
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

fn setup_mock_environment() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempdir().unwrap();
    let game_dir = tmp.path().join("WutheringWaves");
    let appdata_dir = tmp.path().join("AppData");

    fs::create_dir_all(&game_dir).unwrap();
    fs::create_dir_all(&appdata_dir).unwrap();

    let paks_dir = game_dir.join("Client").join("Content").join("Paks");
    let res_root = game_dir.join("Client").join("Saved").join("Resources");
    let v2_dir = res_root.join("2.6.0");
    let binaries_dir = game_dir.join("Client").join("Binaries").join("Win64");

    fs::create_dir_all(&paks_dir).unwrap();
    fs::create_dir_all(&v2_dir).unwrap();
    fs::create_dir_all(&binaries_dir).unwrap();
    fs::write(v2_dir.join("ResManifest"), b"manifest 2.6.0").unwrap();
    fs::write(binaries_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();

    let sig_path = signature::get_sig_path(&game_dir);
    fs::write(&sig_path, b"ORIGINAL_GAME_SIG").unwrap();

    (tmp, game_dir, appdata_dir)
}

#[test]
fn test_app_settings_persistence() {
    let (_tmp, _game_dir, appdata_dir) = setup_mock_environment();
    let settings_file = appdata_dir.join("settings.json");

    let sample_json = r#"{"gamePath":"C:\\Games\\WuWa","installMethod":"resource_mount","dx11":true}"#;
    fs::write(&settings_file, sample_json).unwrap();

    let read_back = fs::read_to_string(&settings_file).unwrap();
    assert_eq!(read_back, sample_json);
}

#[test]
fn test_app_patch_status_evaluation_all_methods() {
    let (_tmp, game_dir, _appdata_dir) = setup_mock_environment();
    assert!(validate_game_path(&game_dir).is_some());

    // 1. Initial State -> Not Installed
    let plan = probe_resource_mount(&game_dir).unwrap();
    assert!(!plan.pak_path.exists());
    assert!(!signature::get_signature_bypass_pak_path(&game_dir).exists());
    assert!(!signature::get_loader_pak_path(&game_dir).exists());

    // 2. Deploy Method 1 (Resource Mount)
    let mount_pak = game_dir.join("mock_mount.pak");
    release_like_pak(&mount_pak);
    assert!(deploy_resource_mount(&plan, &mount_pak, &game_dir).is_ok());
    assert!(plan.pak_path.exists() && plan.owner_marker_path.exists());

    // 3. Switch to Method 2 (Loader)
    remove_all_owned_artifacts(&game_dir);
    assert!(!plan.pak_path.exists());

    let loader_pak = signature::get_loader_pak_path(&game_dir);
    let loader_dll = signature::get_loader_dll_path(&game_dir);
    release_like_pak(&loader_pak);
    fs::write(&loader_dll, b"LOADER_DLL").unwrap();
    fs::write(
        signature::get_loader_marker_path(&game_dir),
        format!(
            "wuwaid-managed-loader:pak-sha256={};loader-sha256={}",
            downloader::compute_sha256(&loader_pak).unwrap(),
            downloader::compute_sha256(&loader_dll).unwrap()
        ),
    )
    .unwrap();
    assert!(loader_pak.exists() && loader_dll.exists());

    // 4. Switch to Method 3 (Sig Bypass)
    remove_all_owned_artifacts(&game_dir);
    assert!(!loader_pak.exists());
    assert!(!loader_dll.exists());

    let bypass_pak = signature::get_signature_bypass_pak_path(&game_dir);
    release_like_pak(&bypass_pak);
    fs::write(
        signature::get_signature_bypass_marker_path(&game_dir),
        format!(
            "wuwaid-managed-signature-bypass:sha256={}",
            downloader::compute_sha256(&bypass_pak).unwrap()
        ),
    )
    .unwrap();
    assert!(bypass_pak.exists());

    // 5. Cleanup
    remove_all_owned_artifacts(&game_dir);
    assert!(!bypass_pak.exists());
}

#[test]
fn test_app_signature_bypass_and_restore_full_lifecycle() {
    let (_tmp, game_dir, _appdata_dir) = setup_mock_environment();

    // 1. Initial State: active .sig exists
    let sig_path = signature::get_sig_path(&game_dir);
    let backup_path = signature::get_sig_backup_path(&game_dir);
    assert!(sig_path.exists());
    assert!(!backup_path.exists());

    // 2. Bypass on launch: moves active .sig to backup
    assert!(signature::bypass_sig(&game_dir).unwrap());
    assert!(!sig_path.exists());
    assert!(backup_path.exists());
    assert!(signature::is_sig_bypassed(&game_dir));

    // 3. Auto-restore on exit/timer: moves backup back to active .sig
    assert!(signature::restore_sig(&game_dir).unwrap());
    assert!(sig_path.exists());
    assert!(!backup_path.exists());
    assert!(!signature::is_sig_bypassed(&game_dir));
    assert_eq!(fs::read_to_string(&sig_path).unwrap(), "ORIGINAL_GAME_SIG");
}

#[test]
fn test_app_atom_release_notes_and_offline_cache() {
    let (_tmp, _game_dir, appdata_dir) = setup_mock_environment();
    let versions_file = appdata_dir.join("versions.json");

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
    <feed xmlns="http://www.w3.org/2005/Atom">
      <title>WuwaID Releases</title>
      <entry>
        <id>tag:github.com,2008:Repository/1228981298/v3.5.1-id.3</id>
        <updated>2026-07-18T12:54:15Z</updated>
        <title>Wuthering Waves Lokalisasi Bahasa Indonesia v.3.5.1-id.3</title>
        <content type="html">&lt;h1&gt;Judul Patch&lt;/h1&gt;&lt;p&gt;Catatan rilis resmi&lt;/p&gt;</content>
        <author><name>TitoTFP</name></author>
      </entry>
    </feed>"#;

    let entry: ReleaseNoteEntry = parse_atom_feed(xml).unwrap();
    assert_eq!(entry.tag, "v3.5.1-id.3");
    assert_eq!(entry.author, "TitoTFP");

    // Cache entry to versions.json
    let cache_json = serde_json::to_string(&entry).unwrap();
    fs::write(&versions_file, cache_json).unwrap();

    let cached_entry: ReleaseNoteEntry =
        serde_json::from_str(&fs::read_to_string(&versions_file).unwrap()).unwrap();
    assert_eq!(cached_entry.tag, "v3.5.1-id.3");
    assert_eq!(
        cached_entry.title,
        "Wuthering Waves Lokalisasi Bahasa Indonesia v.3.5.1-id.3"
    );
}

#[test]
fn test_app_method_switching_command_cleans_and_updates_cache() {
    let (_tmp, game_dir, appdata_dir) = setup_mock_environment();
    let versions_file = appdata_dir.join("versions.json");

    // Initially deploy Method 2
    let loader_pak = signature::get_loader_pak_path(&game_dir);
    let loader_dll = signature::get_loader_dll_path(&game_dir);
    release_like_pak(&loader_pak);
    fs::write(&loader_dll, b"LOADER_DLL").unwrap();
    fs::write(
        signature::get_loader_marker_path(&game_dir),
        format!(
            "wuwaid-managed-loader:pak-sha256={};loader-sha256={}",
            downloader::compute_sha256(&loader_pak).unwrap(),
            downloader::compute_sha256(&loader_dll).unwrap()
        ),
    )
    .unwrap();

    // Switch method command cleans previous artifacts
    remove_all_owned_artifacts(&game_dir);
    assert!(!loader_pak.exists());
    assert!(!loader_dll.exists());

    // Update versions.json with new method
    let mut map = serde_json::Map::new();
    map.insert(
        "_installMethod".to_string(),
        serde_json::Value::String("resource_mount".to_string()),
    );
    fs::write(&versions_file, serde_json::to_string(&map).unwrap()).unwrap();

    let read_back: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&versions_file).unwrap()).unwrap();
    assert_eq!(read_back["_installMethod"], "resource_mount");
}
