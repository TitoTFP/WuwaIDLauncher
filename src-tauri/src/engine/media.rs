use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::engine::downloader::{download_file, verify_sha256, DownloadProgress};

pub const ASSETS_URL: &str = "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/assets.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetEntry {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetManifest {
    pub update_date: Option<String>,
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaReadyPayload {
    pub bgm_url: String,
    pub video_url: String,
}

pub fn parse_manifest(json_str: &str) -> Result<AssetManifest, String> {
    serde_json::from_str::<AssetManifest>(json_str)
        .map_err(|e| format!("Gagal mem-parsing assets.json manifest: {}", e))
}

pub async fn fetch_manifest(client: &reqwest::Client, url: &str) -> Result<AssetManifest, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("Gagal mengambil manifest assets: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Server response error: {}", resp.status()));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("Gagal membaca body manifest assets: {}", e))?;

    parse_manifest(&text)
}

pub fn get_cached_media_paths(cache_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let bgm = cache_dir.join("bgm.mp3");
    let video = cache_dir.join("bg-video.mp4");

    let bgm_opt = bgm.is_file().then_some(bgm);
    let video_opt = video.is_file().then_some(video);

    (bgm_opt, video_opt)
}

pub fn cached_manifest_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("assets-manifest.json")
}

pub fn read_cached_manifest(cache_dir: &Path) -> Result<Option<AssetManifest>, String> {
    let path = cached_manifest_path(cache_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .map_err(|error| format!("Gagal membaca manifest media cache: {error}"))?;
    serde_json::from_str(&data)
        .map(Some)
        .map_err(|error| format!("Manifest media cache tidak valid: {error}"))
}

pub fn write_cached_manifest(cache_dir: &Path, manifest: &AssetManifest) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|error| format!("Gagal membuat folder cache media: {error}"))?;
    let path = cached_manifest_path(cache_dir);
    let temp = path.with_extension("tmp");
    let data = serde_json::to_vec(manifest)
        .map_err(|error| format!("Gagal menyusun manifest media cache: {error}"))?;
    std::fs::write(&temp, data)
        .map_err(|error| format!("Gagal menulis manifest media cache: {error}"))?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Gagal mengganti manifest media cache: {error}"))?;
    }
    std::fs::rename(&temp, &path)
        .map_err(|error| format!("Gagal mengaktifkan manifest media cache: {error}"))
}

pub fn validate_cached_media(
    cache_dir: &Path,
    manifest: &AssetManifest,
) -> Result<bool, String> {
    for name in ["bgm.mp3", "bg-video.mp4"] {
        let Some(asset) = manifest.assets.iter().find(|asset| asset.name == name) else {
            return Ok(false);
        };
        if asset.sha256.trim().is_empty() {
            return Ok(false);
        }
        let path = cache_dir.join(name);
        if !path.is_file() || !verify_sha256(&path, &asset.sha256).unwrap_or(false) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn replace_verified_asset(candidate: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() && !destination.is_file() {
        return Err(format!("Target media bukan file: {:?}", destination));
    }
    let backup = destination.with_extension("previous");
    if backup.exists() {
        let _ = std::fs::remove_file(&backup);
    }
    if destination.exists() {
        std::fs::rename(destination, &backup)
            .map_err(|error| format!("Gagal menyimpan media lama: {error}"))?;
    }
    if let Err(error) = std::fs::rename(candidate, destination) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, destination);
        }
        return Err(format!("Gagal mengaktifkan media baru: {error}"));
    }
    let _ = std::fs::remove_file(backup);
    Ok(())
}

pub async fn sync_media<F>(
    cache_dir: &Path,
    manifest: &AssetManifest,
    on_progress: F,
) -> Result<MediaReadyPayload, String>
where
    F: Fn(&str, DownloadProgress) + Send + Sync + 'static,
{
    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(cache_dir);
    }

    // Strictly verify manifest contains both required media assets
    let bgm_entry = manifest
        .assets
        .iter()
        .find(|a| a.name == "bgm.mp3")
        .ok_or_else(|| "Manifest tidak memuat aset wajib bgm.mp3".to_string())?;

    let video_entry = manifest
        .assets
        .iter()
        .find(|a| a.name == "bg-video.mp4")
        .ok_or_else(|| "Manifest tidak memuat aset wajib bg-video.mp4".to_string())?;

    if bgm_entry.sha256.trim().is_empty() {
        return Err("SHA-256 checksum wajib dicantumkan untuk bgm.mp3".to_string());
    }
    if video_entry.sha256.trim().is_empty() {
        return Err("SHA-256 checksum wajib dicantumkan untuk bg-video.mp4".to_string());
    }

    let on_progress = Arc::new(on_progress);
    let mut bgm_local = String::new();
    let mut video_local = String::new();

    for asset in &[bgm_entry, video_entry] {
        let dest = cache_dir.join(&asset.name);
        let mut needs_download = true;

        if dest.exists() {
            if verify_sha256(&dest, &asset.sha256).unwrap_or(false) {
                needs_download = false;
            }
        }

        if needs_download {
            let asset_name = asset.name.clone();
            let cb = Arc::clone(&on_progress);
            let candidate = cache_dir.join(format!(".{}.candidate", asset.name));
            let _ = std::fs::remove_file(&candidate);
            download_file(&asset.url, &candidate, move |p| {
                cb(&asset_name, p);
            })
            .await?;

            if !verify_sha256(&candidate, &asset.sha256).unwrap_or(false) {
                let _ = std::fs::remove_file(&candidate);
                return Err(format!(
                    "Integritas hash SHA-256 untuk aset {} tidak valid. File dibersihkan.",
                    asset.name
                ));
            }
            replace_verified_asset(&candidate, &dest)?;
        }

        let local_str = dest.to_string_lossy().to_string();
        if asset.name == "bgm.mp3" {
            bgm_local = local_str;
        } else if asset.name == "bg-video.mp4" {
            video_local = local_str;
        }
    }

    if bgm_local.is_empty() || video_local.is_empty() {
        return Err("Aset media tidak lengkap setelah sinkronisasi.".to_string());
    }

    write_cached_manifest(cache_dir, manifest)?;

    Ok(MediaReadyPayload {
        bgm_url: bgm_local,
        video_url: video_local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest_valid() {
        let json = r#"{
            "update_date": "2026-08-20T03:00:00Z",
            "assets": [
                {
                    "name": "bgm.mp3",
                    "url": "https://example.com/bgm.mp3",
                    "sha256": "fca7653b0ffd03d38a70661f6373277927e4dd77466d4666b479972fb463a92d"
                },
                {
                    "name": "bg-video.mp4",
                    "url": "https://example.com/bg-video.mp4",
                    "sha256": "2d01c99d9fc568ae0ae6046423b081d2ee5ea56b5cf47922913fe0c23bacd953"
                }
            ]
        }"#;

        let manifest = parse_manifest(json).unwrap();
        assert_eq!(manifest.update_date.as_deref(), Some("2026-08-20T03:00:00Z"));
        assert_eq!(manifest.assets.len(), 2);
        assert_eq!(manifest.assets[0].name, "bgm.mp3");
        assert_eq!(manifest.assets[1].name, "bg-video.mp4");
    }

    #[test]
    fn test_get_cached_media_paths() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path();

        let (bgm, vid) = get_cached_media_paths(cache_dir);
        assert!(bgm.is_none());
        assert!(vid.is_none());

        let bgm_path = cache_dir.join("bgm.mp3");
        std::fs::write(&bgm_path, b"dummy audio").unwrap();

        let (bgm2, vid2) = get_cached_media_paths(cache_dir);
        assert!(bgm2.is_some());
        assert!(vid2.is_none());

        std::fs::remove_file(&bgm_path).unwrap();
        std::fs::create_dir(&bgm_path).unwrap();
        let (bgm3, vid3) = get_cached_media_paths(cache_dir);
        assert!(bgm3.is_none());
        assert!(vid3.is_none());
    }

    #[tokio::test]
    async fn test_media_rejects_empty_sha256() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path();

        let manifest = AssetManifest {
            update_date: None,
            assets: vec![
                AssetEntry {
                    name: "bgm.mp3".to_string(),
                    url: "https://example.com/bgm.mp3".to_string(),
                    sha256: "".to_string(),
                },
                AssetEntry {
                    name: "bg-video.mp4".to_string(),
                    url: "https://example.com/bg-video.mp4".to_string(),
                    sha256: "2d01c99d9fc568ae0ae6046423b081d2ee5ea56b5cf47922913fe0c23bacd953".to_string(),
                }
            ],
        };

        let res = sync_media(cache_dir, &manifest, |_, _| {}).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("SHA-256 checksum wajib"));
    }

    #[tokio::test]
    async fn test_media_rejects_missing_video_asset() {
        let temp = tempfile::tempdir().unwrap();
        let cache_dir = temp.path();

        let manifest = AssetManifest {
            update_date: None,
            assets: vec![
                AssetEntry {
                    name: "bgm.mp3".to_string(),
                    url: "https://example.com/bgm.mp3".to_string(),
                    sha256: "fca7653b0ffd03d38a70661f6373277927e4dd77466d4666b479972fb463a92d".to_string(),
                }
            ],
        };

        let res = sync_media(cache_dir, &manifest, |_, _| {}).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Manifest tidak memuat aset wajib bg-video.mp4"));
    }
}
