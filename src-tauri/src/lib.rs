pub mod engine;

use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::http::{Request, Response};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const WUWAID_LATEST_DOWNLOAD_BASE_URL: &str =
    "https://github.com/TitoTFP/WuwaID/releases/latest/download/";
const WUWAID_LATEST_CHECKSUMS_URL: &str =
    "https://github.com/TitoTFP/WuwaID/releases/latest/download/SHA256sums.txt";

fn media_manifest_url() -> String {
    std::env::var("WUWAID_ASSETS_URL").unwrap_or_else(|_| engine::media::ASSETS_URL.to_string())
}

// -----------------------------------------------------------------------------
// App State & Paths
// -----------------------------------------------------------------------------

fn get_appdata_dir() -> PathBuf {
    if let Ok(e2e) = std::env::var("WUWAID_E2E_APPDATA") {
        if !e2e.is_empty() {
            return PathBuf::from(e2e);
        }
    }
    dirs_next_or_default()
}

fn dirs_next_or_default() -> PathBuf {
    if let Some(mut dir) = dirs_sys_local_appdata() {
        dir.push("WuwaIDLauncher");
        dir
    } else {
        PathBuf::from("WuwaIDLauncher")
    }
}

fn dirs_sys_local_appdata() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
    }
}

fn get_settings_path() -> PathBuf {
    get_appdata_dir().join("settings.json")
}

pub fn parse_range_header(range_header: &str, total_len: u64) -> Option<(u64, u64)> {
    if total_len == 0 {
        return None;
    }
    let range = range_header.trim().strip_prefix("bytes=")?;
    if range.contains(',') {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let end = end.trim();
    if start.trim().is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = total_len.saturating_sub(suffix.min(total_len));
        return Some((start, total_len - 1));
    }
    let start = start.trim().parse::<u64>().ok()?;
    if start >= total_len {
        return None;
    }
    let end = if end.is_empty() {
        total_len - 1
    } else {
        end.parse::<u64>().ok()?.min(total_len - 1)
    };
    (start <= end).then_some((start, end))
}

// -----------------------------------------------------------------------------
// LauncherBridge Tauri Commands (RPC interface)
// -----------------------------------------------------------------------------

#[tauri::command]
fn minimize_window<R: Runtime>(window: WebviewWindow<R>) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window<R: Runtime>(window: WebviewWindow<R>) {
    if engine::runtime::is_game_running() {
        let _ = window.hide();
    } else {
        window.app_handle().exit(0);
    }
}

#[tauri::command]
fn is_game_running() -> bool {
    engine::runtime::is_game_running()
}

#[tauri::command]
async fn browse_game_folder<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Pilih folder instalasi Wuthering Waves")
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });

    if let Ok(Some(folder)) = rx.await {
        let folder_path = folder.to_string();
        if let Some(valid) = engine::path::normalize_game_path(&folder_path) {
            return Ok(valid.to_string_lossy().to_string());
        }
        return Ok("?INVALID".to_string());
    }

    if let Some(detected) = engine::path::detect_game_path() {
        return Ok(detected.to_string_lossy().to_string());
    }

    Ok(String::new())
}

#[tauri::command]
fn save_settings(settings_json: String) -> Result<(), String> {
    let path = get_settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, settings_json).map_err(|e| format!("Failed to save settings: {}", e))?;
    log::info!("Settings saved to {:?}", path);
    Ok(())
}

#[tauri::command]
fn load_settings() -> Result<String, String> {
    let path = get_settings_path();
    if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {}", e))
    } else {
        Ok(String::new())
    }
}

pub fn app_version_value() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn get_app_version() -> String {
    app_version_value()
}

/// Serves cached media while keeping the filesystem outside the webview.
pub fn media_response(appdata: &Path, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    media_response_from_path(appdata, request)
}

fn media_protocol_handler<R: Runtime>(
    _context: tauri::UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    media_response_from_path(&get_appdata_dir(), &request)
}

#[cfg(test)]
fn registered_media_protocol_response(
    appdata: &Path,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    // Keep the test seam at the same callback boundary used by production
    // registration; the UriSchemeContext is intentionally unused by the
    // handler, so the response can be asserted without a desktop webview.
    media_response_from_path(appdata, &request)
}

fn media_response_from_path(appdata: &Path, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path_str = request.uri().path().trim_start_matches('/');
    if !matches!(path_str, "bgm.mp3" | "bg-video.mp4") {
        return Response::builder().status(404).body(vec![]).unwrap();
    }
    let file_path = appdata.join("Cache").join(path_str);
    if !file_path.is_file() {
        return Response::builder().status(404).body(vec![]).unwrap();
    }

    let mime = if path_str.ends_with(".mp4") {
        "video/mp4"
    } else if path_str.ends_with(".mp3") {
        "audio/mpeg"
    } else {
        "application/octet-stream"
    };
    let full_data = match std::fs::read(&file_path) {
        Ok(data) => data,
        Err(_) => return Response::builder().status(404).body(vec![]).unwrap(),
    };
    let total_len = full_data.len() as u64;

    if let Some(range_val) = request.headers().get("range").and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range_header(range_val, total_len) {
            return Response::builder()
                .status(206)
                .header("Content-Type", mime)
                .header("Content-Range", format!("bytes {}-{}/{}", start, end, total_len))
                .header("Content-Length", (end - start + 1).to_string())
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*")
                .body(full_data[start as usize..=end as usize].to_vec())
                .unwrap();
        }
        let mut response = Response::builder()
            .status(416)
            .header("Content-Range", format!("bytes */{}", total_len));
        if total_len == 0 {
            response = response.header("Content-Length", "0");
        }
        return response.body(vec![]).unwrap();
    }

    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Content-Length", total_len.to_string())
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .body(full_data)
        .unwrap()
}

#[tauri::command]
fn get_vh_version() -> String {
    let path = get_appdata_dir().join("versions.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.get("_vhVersion")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

#[tauri::command]
fn check_launcher_update<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let current_version = env!("CARGO_PKG_VERSION");
        if let Ok(Some(release)) = engine::updater::check_latest_release(current_version).await {
            if let Some(zip) = release.zip_url {
                let _ = app_handle.emit("onLauncherUpdateAvailable", serde_json::json!({
                    "version": release.version,
                    "tag": release.tag_name,
                    "body": release.body,
                    "zipUrl": zip
                }));
            }
        }
    });
}

#[tauri::command]
fn check_and_sync_media<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let cache_dir = get_appdata_dir().join("Cache");

        let _ = app_handle.emit("onMediaStatus", serde_json::json!({
            "status": "checking",
            "message": "Memeriksa aset media..."
        }));

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        match engine::media::fetch_manifest(&client, &media_manifest_url()).await {
            Ok(manifest) => {
                if let Some(ref update_date) = manifest.update_date {
                    let _ = app_handle.emit("onUpdateDate", update_date.clone());
                }

                let app_progress = app_handle.clone();
                let res = engine::media::sync_media(&cache_dir, &manifest, move |asset_name, p| {
                    let _ = app_progress.emit("onMediaProgress", serde_json::json!({
                        "percent": p.percent,
                        "text": format!("Mengunduh {}", asset_name),
                        "speed": p.speed_mbps,
                        "size": p.status
                    }));
                }).await;

                match res {
                    Ok(_) => {
                        let _ = app_handle.emit("onMediaReady", serde_json::json!({
                            "bgmUrl": "media://localhost/bgm.mp3",
                            "videoUrl": "media://localhost/bg-video.mp4"
                        }));
                        let _ = app_handle.emit("onMediaStatus", serde_json::json!({
                            "status": "ready",
                            "message": ""
                        }));
                    }
                    Err(e) => {
                        log::warn!("Media sync error: {}", e);
                        let _ = app_handle.emit("onMediaStatus", serde_json::json!({
                            "status": "error",
                            "message": e
                        }));
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to fetch media manifest: {}", e);
                let _ = app_handle.emit("onMediaStatus", serde_json::json!({
                    "status": "offline",
                    "message": e
                }));
            }
        }
    });
}

#[tauri::command]
fn get_vh_release_notes<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let versions_path = get_appdata_dir().join("versions.json");

        // 1. Check cached release notes first
        let mut had_cached = false;
        if let Ok(content) = std::fs::read_to_string(&versions_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(cached) = json.get("_cachedReleaseNotes") {
                    let _ = app_handle.emit("onVHReleaseNotes", cached.clone());
                    had_cached = true;
                }
            }
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        match engine::atom_feed::fetch_latest_release_notes(&client, engine::atom_feed::ATOM_FEED_URL).await {
            Ok(entry) => {
                let note_json = serde_json::json!({
                    "tag": entry.tag,
                    "date": entry.date,
                    "body": entry.body,
                    "title": entry.title,
                    "author": entry.author
                });

                // Persist to versions.json cache
                let mut map = if let Ok(c) = std::fs::read_to_string(&versions_path) {
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&c).unwrap_or_default()
                } else {
                    serde_json::Map::new()
                };
                map.insert("_cachedReleaseNotes".to_string(), note_json.clone());
                let _ = std::fs::write(&versions_path, serde_json::to_string(&map).unwrap_or_default());

                let _ = app_handle.emit("onVHReleaseNotes", note_json);
            }
            Err(e) => {
                log::warn!("Failed to fetch release notes: {}", e);
                if !had_cached {
                    // Fallback to ensure UI loading resolves
                    let _ = app_handle.emit("onVHReleaseNotes", serde_json::json!({
                        "tag": format!("v{}", env!("CARGO_PKG_VERSION")),
                        "date": "",
                        "body": "<p>Selamat datang di WuwaID Launcher. Catatan rilis daring tidak dapat dijangkau saat ini (offline).</p>",
                        "title": "WuwaID Launcher",
                        "author": "WuwaID Team"
                    }));
                }
            }
        }
    });
}

#[tauri::command]
fn perform_launcher_update<R: Runtime>(app: AppHandle<R>, version: String, zip_url: String) {
    log::info!("Perform launcher update requested: {} -> {}", version, zip_url);
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let temp_zip = get_appdata_dir().join("update.zip");
        let app_progress = app_handle.clone();

        let res = engine::downloader::download_file(&zip_url, &temp_zip, move |p| {
            let _ = app_progress.emit("onLauncherUpdateProgress", serde_json::json!({
                "percent": p.percent,
                "status": p.status
            }));
        }).await;

        if let Ok(()) = res {
            let staging = get_appdata_dir().join(".staging");
            if let Ok(zip_data) = std::fs::read(&temp_zip) {
                if let Ok(exe_path) = engine::updater::extract_zip_update(&zip_data, &staging) {
                    let _ = app_handle.emit("onLauncherUpdateRestarting", ());
                    log::info!("Update staged at {:?}", exe_path);
                }
            }
        }
    });
}

#[tauri::command]
fn check_patch_status<R: Runtime>(app: AppHandle<R>, game_path: String, install_method: String) {
    let p = Path::new(&game_path);
    if !game_path.is_empty() && engine::path::validate_game_path(p).is_some() {
        let is_installed = match install_method.as_str() {
            "method1" => engine::signature::get_method1_pak_path(p).exists(),
            "method2" => {
                engine::signature::get_method2_pak_path(p).exists()
                    && engine::signature::get_method2_loader_path(p).exists()
            }
            _ => {
                if let Ok(plan) = engine::installer::probe_resource_mount(p) {
                    plan.pak_path.exists() && plan.owner_marker_path.exists()
                } else {
                    false
                }
            }
        };

        let status = if is_installed { "ready" } else { "not_installed" };
        let _ = app.emit("onPatchStatus", serde_json::json!({
            "status": status,
            "gamePath": game_path,
            "installMethod": install_method
        }));
    } else {
        let _ = app.emit("onPatchStatus", serde_json::json!({
            "status": "not_installed",
            "gamePath": game_path,
            "installMethod": install_method
        }));
    }
}

#[tauri::command]
fn notify_ui_interactive<R: Runtime>(_app: AppHandle<R>) {
    log::info!("UI interactive milestone reached");
}

#[tauri::command]
fn reset_webview_cache<R: Runtime>(_app: AppHandle<R>) {
    log::info!("Reset webview cache requested");
    let cache_dir = get_appdata_dir().join("Cache");
    if cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&cache_dir);
    }
}

#[tauri::command]
fn get_log_upload_enabled() -> bool {
    true
}

#[tauri::command]
fn upload_logs<R: Runtime>(app: AppHandle<R>, game_path: String) {
    log::info!("Upload logs requested for path: {}", game_path);
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = app_handle.emit("onLogUploadStarted", ());
        let p = Path::new(&game_path);
        let appdata = get_appdata_dir();

        match engine::log_collector::collect_logs_to_zip(p, &appdata) {
            Ok(zip_bytes) => {
                let client_id = engine::telemetry::get_or_create_client_id(&appdata);
                match engine::log_collector::upload_logs_zip(zip_bytes, &client_id).await {
                    Ok(msg) => {
                        let _ = app_handle.emit("onLogUploadFinished", serde_json::json!({
                            "success": true,
                            "message": msg
                        }));
                    }
                    Err(e) => {
                        let _ = app_handle.emit("onLogUploadFinished", serde_json::json!({
                            "success": false,
                            "message": e
                        }));
                    }
                }
            }
            Err(e) => {
                let _ = app_handle.emit("onLogUploadFinished", serde_json::json!({
                    "success": false,
                    "message": e
                }));
            }
        }
    });
}

#[tauri::command]
fn start_installation<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    _vh_mode: String,
    backup: bool,
    install_method: String,
) {
    log::info!(
        "Start installation: path={}, method={}",
        game_path, install_method
    );

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let p = Path::new(&game_path);

        if backup {
            let _ = engine::signature::backup_sig(p);
        }

        // Clean artifacts from any previously installed methods before deploying
        engine::installer::remove_all_owned_artifacts(p);

        let _ = app_handle.emit("onProgressUpdate", serde_json::json!({
            "percent": 5,
            "status": "Memeriksa rilis mod terbaru..."
        }));

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let checksums = match client.get(WUWAID_LATEST_CHECKSUMS_URL).send().await {
            Ok(resp) if resp.status().is_success() => {
                let text = resp.text().await.unwrap_or_default();
                engine::downloader::parse_sha256sums(&text)
            }
            _ => std::collections::HashMap::new(),
        };

        let cache_pak = get_appdata_dir().join("Cache").join(engine::path::PAK_FILE_NAME);
        if let Some(parent) = cache_pak.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let target_asset_name = engine::path::PAK_FILE_NAME;
        let expected_hash = checksums.get(target_asset_name).cloned().unwrap_or_default();

        if expected_hash.is_empty() {
            log::error!("Checksum for {} not found in manifest", target_asset_name);
            let _ = app_handle.emit("onInstallError", format!("Checksum SHA-256 untuk file {} tidak ditemukan pada server release.", target_asset_name));
            return;
        }

        let mut need_download = true;
        if cache_pak.exists() && engine::downloader::verify_sha256(&cache_pak, &expected_hash).unwrap_or(false) {
            need_download = false;
        }

        if need_download {
            let pak_url = format!("{}{}", WUWAID_LATEST_DOWNLOAD_BASE_URL, target_asset_name);
            let app_progress = app_handle.clone();

            // Query HEAD metadata before streaming download and pass expected size
            let content_len = match engine::downloader::get_asset_content_length(&pak_url).await {
                Ok(len) => len,
                Err(e) => {
                    log::error!("Failed to fetch asset metadata: {}", e);
                    let _ = app_handle.emit("onInstallError", format!("Gagal memverifikasi metadata asset rilis: {}", e));
                    return;
                }
            };

            let dl_res = engine::downloader::download_file_with_expected_size(
                &pak_url,
                &cache_pak,
                Some(content_len),
                move |prog| {
                    let _ = app_progress.emit("onProgressUpdate", serde_json::json!({
                        "percent": (prog.percent as f32 * 0.85) as u8,
                        "status": format!("Mengunduh patch... {}", prog.status)
                    }));
                },
            ).await;

            if let Err(e) = dl_res {
                log::error!("Patch download failed: {}", e);
                let _ = app_handle.emit("onInstallError", format!("Gagal mengunduh patch mod: {}", e));
                return;
            }

            if !engine::downloader::verify_sha256(&cache_pak, &expected_hash).unwrap_or(false) {
                let _ = std::fs::remove_file(&cache_pak);
                let _ = app_handle.emit("onInstallError", "Integritas file patch gagal diverifikasi (SHA-256 mismatch).".to_string());
                return;
            }
        }

        let _ = app_handle.emit("onProgressUpdate", serde_json::json!({
            "percent": 90,
            "status": "Memasang file mod..."
        }));

        // Execute Deployment based on method
        match install_method.as_str() {
            "method1" => {
                // Method 3 (Sig Bypass in UI): Deploy to Client/Content/Paks
                let target_pak = engine::signature::get_method1_pak_path(p);
                if let Some(parent) = target_pak.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::copy(&cache_pak, &target_pak) {
                    let _ = app_handle.emit("onInstallError", format!("Gagal menyalin file mod PAK: {}", e));
                    return;
                }
                let _ = engine::signature::backup_sig(p);
            }
            "method2" => {
                // Method 2 (Loader): Deploy to Client/Binaries/Win64/wuwaIndonesia
                let target_pak = engine::signature::get_method2_pak_path(p);
                if let Some(parent) = target_pak.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::copy(&cache_pak, &target_pak) {
                    let _ = app_handle.emit("onInstallError", format!("Gagal menyalin file mod PAK: {}", e));
                    return;
                }
                let method2_marker = engine::signature::get_method2_marker_path(p);
                if let Err(e) = std::fs::write(&method2_marker, "wuwaid-managed-method2") {
                    let _ = std::fs::remove_file(&target_pak);
                    let _ = app_handle.emit("onInstallError", format!("Gagal menandai instalasi Method 2: {}", e));
                    return;
                }

                // Download / copy winhttp.dll loader with mandatory SHA-256 validation
                let loader_path = engine::signature::get_method2_loader_path(p);
                let loader_hash = checksums.get("winhttp.dll").cloned().unwrap_or_default();
                if loader_hash.is_empty() {
                    engine::installer::remove_all_owned_artifacts(p);
                    let _ = app_handle.emit("onInstallError", "Checksum SHA-256 untuk loader winhttp.dll tidak ditemukan pada manifest rilis.".to_string());
                    return;
                }

                let mut need_loader_dl = true;
                if loader_path.exists() && engine::downloader::verify_sha256(&loader_path, &loader_hash).unwrap_or(false) {
                    need_loader_dl = false;
                }

                if need_loader_dl {
                    let loader_url = format!("{}{}", WUWAID_LATEST_DOWNLOAD_BASE_URL, "winhttp.dll");
                    let loader_len = match engine::downloader::get_asset_content_length(&loader_url).await {
                        Ok(len) => len,
                        Err(e) => {
                            engine::installer::remove_all_owned_artifacts(p);
                            let _ = app_handle.emit("onInstallError", format!("Gagal memeriksa metadata loader winhttp.dll: {}", e));
                            return;
                        }
                    };
                    if let Err(e) = engine::downloader::download_file_with_expected_size(&loader_url, &loader_path, Some(loader_len), |_| {}).await {
                        engine::installer::remove_all_owned_artifacts(p);
                        let _ = app_handle.emit("onInstallError", format!("Gagal mengunduh loader winhttp.dll: {}", e));
                        return;
                    }
                    if !engine::downloader::verify_sha256(&loader_path, &loader_hash).unwrap_or(false) {
                        engine::installer::remove_all_owned_artifacts(p);
                        let _ = app_handle.emit("onInstallError", "Integritas hash winhttp.dll gagal diverifikasi (SHA-256 mismatch).".to_string());
                        return;
                    }
                }
            }
            _ => {
                // Method 1 (Resource Mount / method3 / default): Deploy to Saved/Resources
                match engine::installer::probe_resource_mount(p) {
                    Ok(plan) => {
                        if let Err(e) = engine::installer::deploy_resource_mount(&plan, &cache_pak, p) {
                            let _ = app_handle.emit("onInstallError", format!("Gagal deploy resource mount: {}", e));
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = app_handle.emit("onInstallError", e);
                        return;
                    }
                }
            }
        }

        // Save metadata to versions.json
        let mut ver_map = serde_json::Map::new();
        ver_map.insert("_vhVersion".to_string(), serde_json::Value::String("latest".to_string()));
        ver_map.insert("_installMethod".to_string(), serde_json::Value::String(install_method.clone()));
        let versions_path = get_appdata_dir().join("versions.json");
        let _ = std::fs::write(&versions_path, serde_json::to_string(&ver_map).unwrap_or_default());

        let _ = app_handle.emit("onProgressUpdate", serde_json::json!({
            "percent": 100,
            "status": "Instalasi selesai!"
        }));
        let _ = app_handle.emit("onInstallComplete", ());
    });
}

#[tauri::command]
fn check_game_folder_write_access(
    game_path: String,
    _install_method: String,
    _for_installation: bool,
) -> String {
    let p = Path::new(&game_path);
    if !p.exists() {
        return "invalid_path".to_string();
    }
    let test_file = p.join(".wuwaid_write_test");
    if std::fs::write(&test_file, b"test").is_ok() {
        let _ = std::fs::remove_file(test_file);
        "ok".to_string()
    } else {
        "needs_admin".to_string()
    }
}

#[tauri::command]
fn launch_game<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    dx11: bool,
    install_method: String,
) {
    log::info!("Launch game: path={}, dx11={}, method={}", game_path, dx11, install_method);
    let app_handle = app.clone();
    let p = PathBuf::from(game_path.clone());

    tauri::async_runtime::spawn(async move {
        let _ = app_handle.emit("onGameLaunchStarted", ());

        // If Method 3 (Sig Bypass / method1), bypass signature before launching
        if install_method == "method1" {
            let _ = engine::signature::bypass_sig(&p);
        }

        match engine::runtime::launch_game(&p, dx11) {
            Ok(mut child) => {
                let _ = app_handle.emit("onGameRuntimeState", serde_json::json!({
                    "active": true,
                    "origin": "launcher"
                }));

                // Auto-minimize window and trim memory
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
                engine::runtime::trim_memory_working_set();

                // Send telemetry launch event
                let appdata = get_appdata_dir();
                let client_id = engine::telemetry::get_or_create_client_id(&appdata);
                let _ = engine::telemetry::send_heartbeat(&client_id, env!("CARGO_PKG_VERSION"), &install_method, "launch").await;

                // If Method 3, schedule auto-restore after 150 seconds
                if install_method == "method1" {
                    let p_auto_restore = p.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(150)).await;
                        let _ = engine::signature::restore_sig(&p_auto_restore);
                    });
                }

                // Monitor process in background
                let app_for_monitor = app_handle.clone();
                let p_for_monitor = p.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = child.wait();

                    // Restore signature when game exits
                    let _ = engine::signature::restore_sig(&p_for_monitor);

                    let _ = app_for_monitor.emit("onGameRuntimeState", serde_json::json!({
                        "active": false,
                        "origin": "launcher"
                    }));
                    let _ = app_for_monitor.emit("onGameLaunchFinished", ());

                    // Show window back
                    if let Some(window) = app_for_monitor.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                });
            }
            Err(e) => {
                // If launch failed, restore signature immediately
                if install_method == "method1" {
                    let _ = engine::signature::restore_sig(&p);
                }
                let _ = app_handle.emit("onInstallError", e);
                let _ = app_handle.emit("onGameLaunchFinished", ());
            }
        }
    });
}

#[tauri::command]
fn force_quit_game() {
    engine::runtime::force_quit_game();
    log::info!("Force quit game requested");
}

#[tauri::command]
fn switch_method(game_path: String, new_method: String) -> Result<(), String> {
    let p = Path::new(&game_path);
    log::info!("Switching method for {} to {}", game_path, new_method);
    engine::installer::remove_all_owned_artifacts(p);

    let versions_path = get_appdata_dir().join("versions.json");
    if versions_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&versions_path) {
            if let Ok(mut json) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) {
                json.insert("_installMethod".to_string(), serde_json::Value::String(new_method));
                let _ = std::fs::write(&versions_path, serde_json::to_string(&json).unwrap_or_default());
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn uninstall(game_path: String) -> String {
    let p = Path::new(&game_path);
    engine::installer::remove_all_owned_artifacts(p);
    let versions_path = get_appdata_dir().join("versions.json");
    if versions_path.exists() {
        let _ = std::fs::remove_file(versions_path);
    }
    log::info!("Uninstall patch completed for: {}", game_path);
    "ok".to_string()
}

#[tauri::command]
fn restart_as_admin() {
    let _ = engine::elevation::restart_as_admin();
    log::info!("Restart as admin requested");
}

// -----------------------------------------------------------------------------
// Application Entrypoint
// -----------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .register_uri_scheme_protocol("media", media_protocol_handler)
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Tray icon setup
            let quit_i = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Buka Launcher", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Background telemetry heartbeat worker (every 5 minutes)
            let app_telemetry = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                let appdata = get_appdata_dir();
                let client_id = engine::telemetry::get_or_create_client_id(&appdata);

                loop {
                    interval.tick().await;
                    if engine::runtime::is_game_running() {
                        let _ = engine::telemetry::send_heartbeat(
                            &client_id,
                            env!("CARGO_PKG_VERSION"),
                            "active",
                            "heartbeat",
                        )
                        .await;
                    }
                    let _ = app_telemetry.emit("onHeartbeatTick", ());
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            minimize_window,
            close_window,
            is_game_running,
            browse_game_folder,
            save_settings,
            load_settings,
            get_app_version,
            get_vh_version,
            check_and_sync_media,
            check_launcher_update,
            get_vh_release_notes,
            perform_launcher_update,
            check_patch_status,
            switch_method,
            notify_ui_interactive,
            reset_webview_cache,
            get_log_upload_enabled,
            upload_logs,
            start_installation,
            check_game_folder_write_access,
            launch_game,
            force_quit_game,
            uninstall,
            restart_as_admin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running wuwaid launcher application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;
    use std::time::Duration;
    use tauri::{Emitter, Listener};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{assert_ipc_response, INVOKE_KEY};
    use tauri::webview::InvokeRequest;

    fn ipc_request(command: &str, args: serde_json::Value) -> InvokeRequest {
        InvokeRequest {
            cmd: command.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: tauri::Url::parse("tauri://localhost").unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    #[test]
    fn test_real_tauri_command_path_and_event_delivery() {
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![get_app_version, check_game_folder_write_access])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        assert_ipc_response(&window, ipc_request("get_app_version", serde_json::json!({})), Ok(env!("CARGO_PKG_VERSION").to_string()));
        let tmp = tempfile::tempdir().unwrap();
        assert_ipc_response(
            &window,
            ipc_request("check_game_folder_write_access", serde_json::json!({
                "gamePath": tmp.path().to_string_lossy(),
                "installMethod": "method3",
                "forInstallation": true,
            })),
            Ok("ok".to_string()),
        );

        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onMediaReady", move |event| {
            let _ = tx.send(event.payload().to_string());
        });
        app.emit("onMediaReady", serde_json::json!({
            "bgmUrl": "media://localhost/bgm.mp3",
            "videoUrl": "media://localhost/bg-video.mp4",
        })).unwrap();
        let payload = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap()["videoUrl"], "media://localhost/bg-video.mp4");
        app.unlisten(listener);
    }

    #[tokio::test]
    async fn test_media_command_failure_emits_status_without_ready() {
        let appdata = tempfile::tempdir().unwrap();
        let listener_addr = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener_addr.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener_addr.accept().unwrap();
            use std::io::{Read, Write};
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = br#"{"update_date":null,"assets":[]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });

        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());
        std::env::set_var("WUWAID_ASSETS_URL", format!("http://{address}/assets.json"));
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (status_tx, status_rx) = sync_channel(4);
        let (ready_tx, ready_rx) = sync_channel(1);
        let status_listener = app.listen_any("onMediaStatus", move |event| {
            let _ = status_tx.send(event.payload().to_string());
        });
        let ready_listener = app.listen_any("onMediaReady", move |_| {
            let _ = ready_tx.send(());
        });

        check_and_sync_media(app.handle().clone());
        let _ = status_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let error_status = status_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(error_status.contains("error"));
        assert!(ready_rx.recv_timeout(Duration::from_millis(100)).is_err());

        app.unlisten(status_listener);
        app.unlisten(ready_listener);
        std::env::remove_var("WUWAID_ASSETS_URL");
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn test_registered_media_response_supports_range_and_404() {
        let appdata = tempfile::tempdir().unwrap();
        let cache = appdata.path().join("Cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("bg-video.mp4"), b"0123456789").unwrap();

        let request = tauri::http::Request::builder()
            .uri("media://localhost/bg-video.mp4")
            .header("Range", "bytes=2-5")
            .body(Vec::new())
            .unwrap();
        let response = registered_media_protocol_response(appdata.path(), request);
        assert_eq!(response.status(), 206);
        assert_eq!(response.body(), b"2345");
        assert_eq!(response.headers().get("Content-Range").unwrap(), "bytes 2-5/10");

        let missing = tauri::http::Request::builder()
            .uri("media://localhost/missing.mp4")
            .body(Vec::new())
            .unwrap();
        assert_eq!(registered_media_protocol_response(appdata.path(), missing).status(), 404);

        let invalid_range = tauri::http::Request::builder()
            .uri("media://localhost/bg-video.mp4")
            .header("Range", "bytes=99-100")
            .body(Vec::new())
            .unwrap();
        let invalid_response = registered_media_protocol_response(appdata.path(), invalid_range);
        assert_eq!(invalid_response.status(), 416);
        assert_eq!(invalid_response.headers().get("Content-Range").unwrap(), "bytes */10");

        let traversal = tauri::http::Request::builder()
            .uri("media://localhost/../outside.mp4")
            .body(Vec::new())
            .unwrap();
        assert_eq!(registered_media_protocol_response(appdata.path(), traversal).status(), 404);
    }

    #[test]
    fn test_parse_range_header() {
        let total = 1000;

        assert_eq!(parse_range_header("bytes=0-499", total), Some((0, 499)));
        assert_eq!(parse_range_header("bytes=500-", total), Some((500, 999)));
        assert_eq!(parse_range_header("bytes=900-2000", total), Some((900, 999)));
        assert_eq!(parse_range_header("bytes=-100", total), Some((900, 999)));
        assert_eq!(parse_range_header("items=0-10", total), None);
        assert_eq!(parse_range_header("bytes=500-200", total), None);
        assert_eq!(parse_range_header("bytes=1000-1200", total), None);
        assert_eq!(parse_range_header("bytes=1-2,4-5", total), None);
        assert_eq!(parse_range_header("bytes=0-0", 0), None);
    }
}
