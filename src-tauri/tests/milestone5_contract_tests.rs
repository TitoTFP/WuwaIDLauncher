use std::io::{Cursor, Write};
use std::path::Path;

use wuwaid_launcher_lib::engine::{media, updater};

fn media_manifest(bgm: &[u8], video: &[u8]) -> media::AssetManifest {
    media::AssetManifest {
        update_date: None,
        assets: vec![
            media::AssetEntry {
                name: "bgm.mp3".to_string(),
                url: "https://example.com/bgm.mp3".to_string(),
                sha256: sha256(bgm),
            },
            media::AssetEntry {
                name: "bg-video.mp4".to_string(),
                url: "https://example.com/bg-video.mp4".to_string(),
                sha256: sha256(video),
            },
        ],
    }
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut buffer);
        for (name, contents) in entries {
            archive
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
    }
    buffer.into_inner()
}

#[test]
fn media_cache_is_ready_only_when_manifest_hashes_match() {
    let cache = tempfile::tempdir().unwrap();
    let bgm = b"valid audio";
    let video = b"valid video";
    let manifest = media_manifest(bgm, video);
    std::fs::write(cache.path().join("bgm.mp3"), bgm).unwrap();
    std::fs::write(cache.path().join("bg-video.mp4"), video).unwrap();

    assert!(media::validate_cached_media(cache.path(), &manifest).unwrap());
    std::fs::write(cache.path().join("bgm.mp3"), b"corrupt").unwrap();
    assert!(!media::validate_cached_media(cache.path(), &manifest).unwrap());
}

#[test]
fn cached_media_manifest_round_trips_atomically() {
    let cache = tempfile::tempdir().unwrap();
    let manifest = media_manifest(b"audio", b"video");
    media::write_cached_manifest(cache.path(), &manifest).unwrap();
    let loaded = media::read_cached_manifest(cache.path()).unwrap().unwrap();
    assert_eq!(loaded, manifest);
}

#[test]
fn update_checksum_and_archive_validation_rejects_bad_payloads() {
    let valid_hash = "a".repeat(64);
    let checksums = updater::parse_checksum_manifest(&format!(
        "{valid_hash} *WuwaIDLauncher.zip\ndef456 WuwaIDLauncher.exe\n"
    ));
    assert_eq!(checksums.get("WuwaIDLauncher.zip"), Some(&valid_hash));
    assert!(updater::is_valid_sha256(&valid_hash));
    assert!(!updater::is_valid_sha256("abc123"));
    assert!(updater::is_safe_download_url(
        "https://example.com/WuwaIDLauncher.zip"
    ));
    assert!(!updater::is_safe_download_url(
        "http://example.com/WuwaIDLauncher.zip"
    ));

    let valid = zip_with(&[("WuwaIDLauncher.exe", b"exe")]);
    assert!(updater::validate_update_archive(&valid, "WuwaIDLauncher.exe").is_ok());

    let missing = zip_with(&[("readme.txt", b"not executable")]);
    assert!(updater::validate_update_archive(&missing, "WuwaIDLauncher.exe").is_err());

    let traversal = zip_with(&[("../../outside.exe", b"unsafe")]);
    assert!(updater::validate_update_archive(&traversal, "WuwaIDLauncher.exe").is_err());
}

#[test]
fn update_archive_extraction_does_not_escape_staging_directory() {
    let staging = tempfile::tempdir().unwrap();
    let payload = zip_with(&[("WuwaIDLauncher.exe", b"exe")]);
    let extracted = updater::extract_zip_update(&payload, staging.path()).unwrap();
    assert!(extracted.starts_with(Path::new(staging.path())));
}

#[test]
fn update_archive_preserves_canonical_packaged_executable_name() {
    let staging = tempfile::tempdir().unwrap();
    let payload = zip_with(&[("WuwaIDLauncher.exe", b"exe")]);
    let extracted = updater::extract_zip_update(&payload, staging.path()).unwrap();
    assert_eq!(
        extracted.file_name().and_then(|name| name.to_str()),
        Some("WuwaIDLauncher.exe")
    );
    assert!(extracted.exists());
}

#[test]
fn update_archive_rejects_legacy_packaged_executable_name() {
    let staging = tempfile::tempdir().unwrap();
    let payload = zip_with(&[("wuwaid-launcher.exe", b"exe")]);
    assert!(updater::extract_zip_update(&payload, staging.path()).is_err());
}

#[test]
fn update_handoff_script_overwrites_without_rollback_steps() {
    let temp = tempfile::tempdir().unwrap();
    let staging = temp.path().join("staging");
    let current = temp.path().join("WuwaIDLauncher.exe");
    let handoff = temp.path().join("handoff.cmd");
    std::fs::create_dir_all(&staging).unwrap();
    let result = updater::create_update_handoff(&staging, &current, &handoff).unwrap();
    assert_eq!(result, handoff);
    let script = std::fs::read_to_string(handoff).unwrap();
    assert!(script.contains("copy /Y"));
    assert!(!script.contains(".old"));
    assert!(!script.contains("rollback"));
    assert!(!script.contains("move /Y"));
    assert!(script.contains("WuwaIDLauncher.exe"));
}
