use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;
use sha2::Digest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wuwaid_launcher_lib::engine::downloader::{
    download_file_with_expected_size, get_asset_content_length, verify_sha256,
};
use wuwaid_launcher_lib::engine::media::{sync_media, AssetEntry, AssetManifest};

/// Spawns an in-process mock HTTP router that serves path-mapped payloads.
async fn spawn_mock_router(
    routes: HashMap<String, (Vec<u8>, Option<usize>)>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 2048];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);

            let is_head = req.starts_with("HEAD");
            eprintln!("DEBUG_MOCK_REQ:\n{}\nis_head: {}", req, is_head);
            let mut path = "/".to_string();
            if let Some(first_line) = req.lines().next() {
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    path = parts[1].to_string();
                }
            }

            if let Some((body, content_len_override)) = routes.get(&path) {
                let len = content_len_override.unwrap_or(body.len());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                    len
                );
                let _ = socket.write_all(response.as_bytes()).await;
                if !is_head {
                    let _ = socket.write_all(body).await;
                }
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            } else {
                let not_found = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(not_found.as_bytes()).await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            }
        }
    });

    (url, handle)
}

#[tokio::test]
async fn test_mock_http_download_success_and_sha_verification() {
    let payload = b"WUWA_ID_TEST_PAYLOAD_CONTENT_OK";
    let hash = hex::encode(sha2::Sha256::digest(payload)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/asset.pak".to_string(), (payload.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let download_url = format!("{}/asset.pak", base_url);

    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("asset.pak");

    // 1. Verify HEAD metadata query
    let meta_len = get_asset_content_length(&download_url).await.unwrap();
    assert_eq!(meta_len, payload.len() as u64);

    // 2. Stream download with exact expected size
    let downloaded_bytes = download_file_with_expected_size(
        &download_url,
        &dest,
        Some(meta_len),
        |p| {
            assert!(p.percent <= 100);
        },
    )
    .await
    .unwrap();

    assert_eq!(downloaded_bytes, payload.len() as u64);
    assert!(dest.exists());

    // 3. Verify SHA-256
    assert!(verify_sha256(&dest, &hash).unwrap());
}

#[tokio::test]
async fn test_mock_http_content_length_mismatch_fails_and_cleans_up() {
    let payload = b"SHORT_DATA";
    let mut routes = HashMap::new();
    // Mock server advertises 1000 bytes, but sends only 10 bytes
    routes.insert("/mismatch.pak".to_string(), (payload.to_vec(), Some(1000)));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let download_url = format!("{}/mismatch.pak", base_url);

    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("mismatch.pak");

    let res = download_file_with_expected_size(&download_url, &dest, Some(1000), |_| {}).await;
    assert!(res.is_err());
    assert!(!dest.exists());
    assert!(!dest.with_extension("tmp_download").exists());
}

#[tokio::test]
async fn test_mock_http_media_sync_with_full_hash_validation() {
    let audio_data = b"MOCK_BGM_AUDIO_DATA_VALID";
    let video_data = b"MOCK_BG_VIDEO_DATA_VALID";

    let audio_hash = hex::encode(sha2::Sha256::digest(audio_data)).to_lowercase();
    let video_hash = hex::encode(sha2::Sha256::digest(video_data)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/bgm.mp3".to_string(), (audio_data.to_vec(), None));
    routes.insert("/bg-video.mp4".to_string(), (video_data.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let audio_url = format!("{}/bgm.mp3", base_url);
    let video_url = format!("{}/bg-video.mp4", base_url);

    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path();

    let manifest = AssetManifest {
        update_date: Some("2026-08-28T04:00:00Z".to_string()),
        assets: vec![
            AssetEntry {
                name: "bgm.mp3".to_string(),
                url: audio_url,
                sha256: audio_hash,
            },
            AssetEntry {
                name: "bg-video.mp4".to_string(),
                url: video_url,
                sha256: video_hash,
            },
        ],
    };

    let sync_res = sync_media(cache_dir, &manifest, |_, _| {}).await.unwrap();
    assert!(PathBuf::from(&sync_res.bgm_url).exists());
    assert!(PathBuf::from(&sync_res.video_url).exists());
}

#[tokio::test]
async fn test_corrupted_cached_media_is_rejected_and_re_downloaded_before_ready() {
    let audio_data = b"MOCK_GENUINE_BGM_AUDIO";
    let video_data = b"MOCK_GENUINE_BG_VIDEO";

    let audio_hash = hex::encode(sha2::Sha256::digest(audio_data)).to_lowercase();
    let video_hash = hex::encode(sha2::Sha256::digest(video_data)).to_lowercase();

    let mut routes = HashMap::new();
    routes.insert("/bgm.mp3".to_string(), (audio_data.to_vec(), None));
    routes.insert("/bg-video.mp4".to_string(), (video_data.to_vec(), None));

    let (base_url, _server) = spawn_mock_router(routes).await;
    let audio_url = format!("{}/bgm.mp3", base_url);
    let video_url = format!("{}/bg-video.mp4", base_url);

    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path();

    // Create a corrupted bgm.mp3 in the cache directory
    let corrupted_bgm = cache_dir.join("bgm.mp3");
    std::fs::write(&corrupted_bgm, b"CORRUPTED_GARBAGE_DATA").unwrap();
    assert!(corrupted_bgm.exists());

    let manifest = AssetManifest {
        update_date: None,
        assets: vec![
            AssetEntry {
                name: "bgm.mp3".to_string(),
                url: audio_url,
                sha256: audio_hash.clone(),
            },
            AssetEntry {
                name: "bg-video.mp4".to_string(),
                url: video_url,
                sha256: video_hash.clone(),
            },
        ],
    };

    // sync_media must reject/remove the corrupted file and re-download the genuine file
    let sync_res = sync_media(cache_dir, &manifest, |_, _| {}).await.unwrap();
    assert!(PathBuf::from(&sync_res.bgm_url).exists());
    assert!(verify_sha256(&PathBuf::from(&sync_res.bgm_url), &audio_hash).unwrap());
    assert!(verify_sha256(&PathBuf::from(&sync_res.video_url), &video_hash).unwrap());
}
