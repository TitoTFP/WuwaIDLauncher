use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use wuwaid_launcher_lib::engine::media::{sync_media, AssetEntry, AssetManifest};

#[tokio::test]
async fn failed_media_sync_emits_status_without_media_ready() {
    let temp = tempdir().unwrap();
    let statuses = Arc::new(Mutex::new(Vec::<String>::new()));
    let ready = Arc::new(Mutex::new(false));
    let status_sink = Arc::clone(&statuses);
    let ready_sink = Arc::clone(&ready);

    let manifest = AssetManifest {
        update_date: None,
        assets: vec![AssetEntry {
            name: "bgm.mp3".to_string(),
            url: "http://127.0.0.1:1/missing.mp3".to_string(),
            sha256: "00".repeat(32),
        }],
    };

    match sync_media(temp.path(), &manifest, |_, _| {}).await {
        Ok(_) => *ready_sink.lock().unwrap() = true,
        Err(error) => status_sink.lock().unwrap().push(format!("error:{error}")),
    }

    let statuses = statuses.lock().unwrap();
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].starts_with("error:"));
    assert!(!*ready.lock().unwrap());
    assert!(!temp.path().join("bgm.mp3").exists());
}

#[tokio::test]
async fn missing_required_media_emits_error_without_media_ready() {
    let temp = tempdir().unwrap();
    let statuses = Arc::new(Mutex::new(Vec::<String>::new()));
    let ready = Arc::new(Mutex::new(false));
    let status_sink = Arc::clone(&statuses);
    let ready_sink = Arc::clone(&ready);

    let manifest = AssetManifest {
        update_date: None,
        assets: Vec::new(),
    };

    match sync_media(temp.path(), &manifest, |_, _| {}).await {
        Ok(_) => *ready_sink.lock().unwrap() = true,
        Err(error) => status_sink.lock().unwrap().push(format!("error:{error}")),
    }

    assert_eq!(statuses.lock().unwrap().len(), 1);
    assert!(!*ready.lock().unwrap());
}
