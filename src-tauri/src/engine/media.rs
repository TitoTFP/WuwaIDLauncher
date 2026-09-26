use crate::engine::downloader::{
    download_file, read_response_body_limited, replace_file_atomically, verify_sha256,
    DownloadProgress,
};
use crate::engine::theme::ThemeDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The two assets the launcher always needs. Every media helper derives its
/// work from this list so a new asset cannot be added in one place only.
pub const MEDIA_ASSET_NAMES: [&str; 2] = ["bgm.mp3", "bg-video.mp4"];

pub const ASSETS_URL: &str =
    "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/assets.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

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
    /// Optional remote theme. Honoured only when the manifest is signed.
    #[serde(default)]
    pub theme: Option<ThemeDefinition>,
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

pub fn required_asset<'a>(
    manifest: &'a AssetManifest,
    name: &str,
) -> Result<&'a AssetEntry, String> {
    manifest
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("Manifest tidak memuat aset wajib {name}"))
}

/// Rejects anything that is not an official asset under the launcher repository
/// (or a loopback fixture during tests), so a manifest can never point the
/// downloader at an arbitrary host.
pub fn validate_asset_url(asset: &AssetEntry, suffixes: (&str, &str)) -> Result<(), String> {
    let url = reqwest::Url::parse(&asset.url)
        .map_err(|_| format!("URL media {} tidak valid.", asset.name))?;
    let has_safe_authority = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    let is_official_asset = url.scheme() == "https"
        && url.host_str() == Some("raw.githubusercontent.com")
        && url.port().is_none()
        && url.path().starts_with("/TitoTFP/WuwaID/")
        && url.path().ends_with(suffixes.0);
    let is_loopback_test_asset = url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
        && url.path().ends_with(suffixes.1);
    if !has_safe_authority || (!is_official_asset && !is_loopback_test_asset) {
        return Err(format!(
            "URL media {} bukan asset GitHub resmi yang diizinkan.",
            asset.name
        ));
    }
    Ok(())
}

fn validate_media_asset_url(asset: &AssetEntry) -> Result<(), String> {
    let suffixes = match asset.name.as_str() {
        "bgm.mp3" => ("/Audio/bgm.mp3", "/bgm.mp3"),
        "bg-video.mp4" => ("/Video/bg-video.mp4", "/bg-video.mp4"),
        _ => return Err(format!("Nama aset media tidak dikenal: {}", asset.name)),
    };
    validate_asset_url(asset, suffixes)
}

/// Returns the raw manifest bytes alongside the parsed form. Signature
/// verification must run against these exact bytes, before anything in the
/// manifest is trusted.
pub async fn fetch_manifest_bytes(
    client: &reqwest::Client,
    url: &str,
) -> Result<(Vec<u8>, AssetManifest), String> {
    let resp = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|e| format!("Gagal mengambil manifest assets: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Server response error: {}", resp.status()));
    }

    if resp
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return Err("Manifest assets terlalu besar.".to_string());
    }
    let body = read_response_body_limited(resp, MAX_MANIFEST_BYTES)
        .await
        .map_err(|error| format!("Gagal membaca body manifest assets: {error}"))?;
    let text = String::from_utf8(body.to_vec())
        .map_err(|e| format!("Body manifest assets bukan UTF-8 valid: {}", e))?;

    Ok((body, parse_manifest(&text)?))
}

pub async fn fetch_manifest(client: &reqwest::Client, url: &str) -> Result<AssetManifest, String> {
    fetch_manifest_bytes(client, url)
        .await
        .map(|(_, manifest)| manifest)
}
pub fn get_cached_media_paths(cache_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let path_for = |name: &str| {
        let path = cache_dir.join(name);
        path.is_file().then_some(path)
    };
    (
        path_for(MEDIA_ASSET_NAMES[0]),
        path_for(MEDIA_ASSET_NAMES[1]),
    )
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
    replace_file_atomically(&temp, &path)
        .map_err(|error| format!("Gagal mengaktifkan manifest media cache: {error}"))
}

pub fn validate_cached_media(cache_dir: &Path, manifest: &AssetManifest) -> Result<bool, String> {
    for name in MEDIA_ASSET_NAMES {
        let Ok(asset) = required_asset(manifest, name) else {
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
    replace_file_atomically(candidate, destination)
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

    // Three passes so error precedence stays structural: a missing asset is
    // reported before a bad checksum or a rejected URL.
    let mut required: Vec<(&str, &AssetEntry)> = Vec::with_capacity(MEDIA_ASSET_NAMES.len());
    for name in MEDIA_ASSET_NAMES {
        required.push((name, required_asset(manifest, name)?));
    }
    for (name, asset) in &required {
        if asset.sha256.trim().is_empty() {
            return Err(format!("SHA-256 checksum wajib dicantumkan untuk {name}"));
        }
        validate_media_asset_url(asset)?;
    }

    let on_progress = Arc::new(on_progress);
    let mut resolved: BTreeMap<&str, String> = BTreeMap::new();

    for (name, asset) in required {
        let dest = cache_dir.join(name);
        if !dest.is_file() || !verify_sha256(&dest, &asset.sha256).unwrap_or(false) {
            let asset_name = asset.name.clone();
            let cb = Arc::clone(&on_progress);
            let candidate = cache_dir.join(format!(".{name}.candidate"));
            let _ = std::fs::remove_file(&candidate);
            download_file(&asset.url, &candidate, move |p| {
                cb(&asset_name, p);
            })
            .await?;

            if !verify_sha256(&candidate, &asset.sha256).unwrap_or(false) {
                let _ = std::fs::remove_file(&candidate);
                return Err(format!(
                    "Integritas hash SHA-256 untuk aset {name} tidak valid. File dibersihkan."
                ));
            }
            replace_verified_asset(&candidate, &dest)?;
        }

        resolved.insert(name, dest.to_string_lossy().to_string());
    }

    let (Some(bgm_local), Some(video_local)) = (
        resolved.get(MEDIA_ASSET_NAMES[0]),
        resolved.get(MEDIA_ASSET_NAMES[1]),
    ) else {
        return Err("Aset media tidak lengkap setelah sinkronisasi.".to_string());
    };

    write_cached_manifest(cache_dir, manifest)?;

    Ok(MediaReadyPayload {
        bgm_url: bgm_local.clone(),
        video_url: video_local.clone(),
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

        let manifest = match parse_manifest(json) {
            Ok(manifest) => manifest,
            Err(error) => panic!("manifest fixture invalid: {error}"),
        };
        assert_eq!(
            manifest.update_date.as_deref(),
            Some("2026-08-20T03:00:00Z")
        );
        assert_eq!(manifest.assets.len(), 2);
        assert_eq!(manifest.assets[0].name, "bgm.mp3");
        assert_eq!(manifest.assets[1].name, "bg-video.mp4");
    }

    #[test]
    fn test_get_cached_media_paths() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let cache_dir = temp.path();

        let (bgm, vid) = get_cached_media_paths(cache_dir);
        assert!(bgm.is_none());
        assert!(vid.is_none());

        let bgm_path = cache_dir.join("bgm.mp3");
        std::fs::write(&bgm_path, b"dummy audio")?;

        let (bgm2, vid2) = get_cached_media_paths(cache_dir);
        assert!(bgm2.is_some());
        assert!(vid2.is_none());

        std::fs::remove_file(&bgm_path)?;
        std::fs::create_dir(&bgm_path)?;
        let (bgm3, vid3) = get_cached_media_paths(cache_dir);
        assert!(bgm3.is_none());
        assert!(vid3.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_media_rejects_untrusted_asset_url() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let manifest = AssetManifest {
            update_date: None,
            theme: None,
            assets: vec![
                AssetEntry {
                    name: "bgm.mp3".to_string(),
                    url: "https://example.com/bgm.mp3".to_string(),
                    sha256: "fca7653b0ffd03d38a70661f6373277927e4dd77466d4666b479972fb463a92d".to_string(),
                },
                AssetEntry {
                    name: "bg-video.mp4".to_string(),
                    url: "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/Video/bg-video.mp4".to_string(),
                    sha256: "2d01c99d9fc568ae0ae6046423b081d2ee5ea56b5cf47922913fe0c23bacd953".to_string(),
                },
            ],
        };

        let result = sync_media(temp.path(), &manifest, |_, _| {}).await;
        assert!(matches!(result, Err(error) if error.contains("URL media bgm.mp3")));
        Ok(())
    }

    #[tokio::test]
    async fn test_media_rejects_empty_sha256() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let cache_dir = temp.path();

        let manifest = AssetManifest {
            update_date: None,
            theme: None,
            assets: vec![
                AssetEntry {
                    name: "bgm.mp3".to_string(),
                    url: "https://example.com/bgm.mp3".to_string(),
                    sha256: "".to_string(),
                },
                AssetEntry {
                    name: "bg-video.mp4".to_string(),
                    url: "https://example.com/bg-video.mp4".to_string(),
                    sha256: "2d01c99d9fc568ae0ae6046423b081d2ee5ea56b5cf47922913fe0c23bacd953"
                        .to_string(),
                },
            ],
        };

        let res = sync_media(cache_dir, &manifest, |_, _| {}).await;
        assert!(matches!(
            res,
            Err(error) if error.contains("SHA-256 checksum wajib")
        ));
        Ok(())
    }

    #[tokio::test]
    async fn test_media_rejects_missing_video_asset() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let cache_dir = temp.path();

        let manifest = AssetManifest {
            update_date: None,
            theme: None,
            assets: vec![AssetEntry {
                name: "bgm.mp3".to_string(),
                url: "https://example.com/bgm.mp3".to_string(),
                sha256: "fca7653b0ffd03d38a70661f6373277927e4dd77466d4666b479972fb463a92d"
                    .to_string(),
            }],
        };

        let res = sync_media(cache_dir, &manifest, |_, _| {}).await;
        assert!(res.is_err());
        assert!(matches!(
            res,
            Err(error) if error.contains("Manifest tidak memuat aset wajib bg-video.mp4")
        ));
        Ok(())
    }
}
