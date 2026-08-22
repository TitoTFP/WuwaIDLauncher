pub mod engine;

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::http::{Request, Response};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

const WUWAID_LATEST_DOWNLOAD_BASE_URL: &str =
    "https://github.com/TitoTFP/WuwaID/releases/latest/download/";
const WUWAID_LATEST_CHECKSUMS_URL: &str =
    "https://github.com/TitoTFP/WuwaID/releases/latest/download/SHA256sums.txt";
const SUPPORT_URL: &str = "https://trakteer.id/TitoTFP";
const LAUNCHER_UPDATE_RESTART_DELAY_SECONDS: u64 = 12;

fn launcher_update_restart_countdown() -> impl Iterator<Item = u64> {
    (1..=LAUNCHER_UPDATE_RESTART_DELAY_SECONDS).rev()
}

fn tray_notification_body() -> &'static str {
    "Launcher berjalan di system tray. Klik ikon tray untuk membukanya kembali."
}

fn notify_tray_minimized<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app
        .notification()
        .builder()
        .title("WuwaID Launcher")
        .body(tray_notification_body())
        .show()
    {
        log::warn!("Tray notification tidak dapat ditampilkan: {error}");
    }
}

fn media_manifest_url() -> String {
    std::env::var("WUWAID_ASSETS_URL").unwrap_or_else(|_| engine::media::ASSETS_URL.to_string())
}

fn media_url(asset_name: &str) -> String {
    if cfg!(windows) {
        format!("http://media.localhost/{asset_name}")
    } else {
        format!("media://localhost/{asset_name}")
    }
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

fn restore_legacy_signature_from_settings() {
    let settings_path = get_settings_path();
    let raw = match std::fs::read_to_string(&settings_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            log::warn!("Tidak dapat membaca settings untuk migrasi signature legacy: {error}");
            return;
        }
    };
    let value = match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) => value,
        Err(error) => {
            log::warn!("Settings tidak valid untuk migrasi signature legacy: {error}");
            return;
        }
    };
    let Some(game_path) = value
        .get("gamePath")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };

    let normalized = match engine::installer::validate_signature_restore_path(game_path) {
        Ok(path) => path,
        Err(error) => {
            log::warn!(
                "Path game tidak aman untuk migrasi signature legacy ({}): {error}",
                game_path
            );
            return;
        }
    };

    match engine::signature::restore_sig(&normalized) {
        Ok(true) => log::info!(
            "Signature legacy dipulihkan dari backup saat lifecycle launcher: {}",
            normalized.display()
        ),
        Ok(false) => {}
        Err(error) => log::warn!(
            "Signature legacy tidak dapat dipulihkan untuk {}: {error}",
            normalized.display()
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowMinimizeAction {
    Minimize,
    Hide,
}

fn window_minimize_action(tray_mode: bool) -> WindowMinimizeAction {
    if tray_mode {
        WindowMinimizeAction::Hide
    } else {
        WindowMinimizeAction::Minimize
    }
}

#[derive(Default)]
struct RuntimeCoordinator {
    launcher_pid: Mutex<Option<u32>>,
    force_quit_requested: Mutex<bool>,
    tray_mode: Mutex<bool>,
}

fn coordinator_launcher_pid<R: Runtime>(app: &AppHandle<R>) -> Option<u32> {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.launcher_pid.lock().ok().and_then(|value| *value))
}

fn set_launcher_process<R: Runtime>(app: &AppHandle<R>, pid: Option<u32>) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.launcher_pid.lock() {
            *value = pid;
        }
        if pid.is_some() {
            if let Ok(mut value) = state.force_quit_requested.lock() {
                *value = false;
            }
        }
    }
}

fn set_tray_mode<R: Runtime>(app: &AppHandle<R>, tray_mode: bool) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.tray_mode.lock() {
            *value = tray_mode;
        }
    }
}

fn is_tray_mode<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.tray_mode.lock().ok().map(|value| *value))
        .unwrap_or(false)
}

fn configure_webview_memory_target<R: Runtime>(_app: &AppHandle<R>) {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        };
        use windows_core::Interface;

        let Some(window) = _app.get_webview_window("main") else {
            return;
        };
        if let Err(error) = window.with_webview(|webview| {
            let result = unsafe {
                webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_19>())
                    .and_then(|core| {
                        core.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW)
                    })
            };
            if let Err(error) = result {
                log::debug!("WebView2 low-memory target tidak tersedia: {error}");
            }
        }) {
            log::debug!("Konfigurasi target memori WebView2 gagal: {error}");
        }
    }
}

fn suspend_webview<R: Runtime + 'static>(_app: &AppHandle<R>) {
    #[cfg(windows)]
    {
        suspend_webview_attempt(_app.clone(), 0);
    }
}

#[cfg(windows)]
fn suspend_webview_attempt<R: Runtime + 'static>(app: AppHandle<R>, attempt: u8) {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::ICoreWebView2_3, TrySuspendCompletedHandler,
    };
    use windows_core::Interface;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Err(error) = window.with_webview(move |webview| {
        let result = unsafe {
            webview
                .controller()
                .SetIsVisible(false)
                .and_then(|_| webview.controller().CoreWebView2())
                .and_then(|core| core.cast::<ICoreWebView2_3>())
                .and_then(|core| {
                    let retry_app = app.clone();
                    let callback =
                        TrySuspendCompletedHandler::create(Box::new(move |result, suspended| {
                            match result {
                                Ok(()) if suspended => {
                                    log::debug!("WebView2 berhasil disuspend");
                                }
                                Ok(()) if attempt < 3 => {
                                    log::debug!(
                                        "WebView2 menolak suspend; menjadwalkan percobaan ulang"
                                    );
                                    let retry_app = retry_app.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(Duration::from_millis(150)).await;
                                        let Some(window) = retry_app.get_webview_window("main")
                                        else {
                                            return;
                                        };
                                        if matches!(window.is_visible(), Ok(false)) {
                                            suspend_webview_attempt(retry_app, attempt + 1);
                                        }
                                    });
                                }
                                Ok(()) => {
                                    log::debug!("WebView2 menolak suspend setelah percobaan ulang");
                                }
                                Err(error) => {
                                    log::debug!("WebView2 suspend gagal: {error}");
                                }
                            }
                            Ok(())
                        }));
                    core.TrySuspend(&callback)
                })
        };
        if let Err(error) = result {
            log::debug!("Permintaan suspend WebView2 gagal: {error}");
        }
    }) {
        log::debug!("WebView2 tidak dapat disuspend: {error}");
    }
}

fn resume_webview<R: Runtime>(_app: &AppHandle<R>) {
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use windows_core::Interface;

        let Some(window) = _app.get_webview_window("main") else {
            return;
        };
        if let Err(error) = window.with_webview(|webview| {
            let result = unsafe {
                webview
                    .controller()
                    .SetIsVisible(true)
                    .and_then(|_| webview.controller().CoreWebView2())
                    .and_then(|core| core.cast::<ICoreWebView2_3>())
                    .and_then(|core| core.Resume())
            };
            if let Err(error) = result {
                log::debug!("Resume WebView2 tidak diperlukan atau gagal: {error}");
            }
        }) {
            log::debug!("WebView2 tidak dapat di-resume: {error}");
        }
    }
}

fn request_close<R: Runtime>(app: &AppHandle<R>) {
    set_tray_mode(app, false);
    app.exit(0);
}

fn mark_force_quit_requested<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.force_quit_requested.lock() {
            *value = true;
        }
    }
}

fn take_force_quit_requested<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| {
            state.force_quit_requested.lock().ok().map(|mut value| {
                let requested = *value;
                *value = false;
                requested
            })
        })
        .unwrap_or(false)
}

fn save_launch_evidence(mut evidence: engine::runtime::LaunchEvidence) -> Option<PathBuf> {
    let diagnostics_dir = get_appdata_dir().join("Diagnostics");
    if std::fs::create_dir_all(&diagnostics_dir).is_err() {
        return None;
    }

    let stem = format!(
        "launch-{}-{}",
        evidence.started_at_ms,
        evidence
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    let path = diagnostics_dir.join(format!("{stem}.json"));
    evidence.evidence_path = Some(path.clone());
    let Ok(serialized) = serde_json::to_vec_pretty(&evidence) else {
        return None;
    };
    if std::fs::write(&path, serialized).is_err() {
        return None;
    }
    Some(path)
}

fn launch_error_message(mut evidence: engine::runtime::LaunchEvidence) -> String {
    let path = save_launch_evidence(evidence.clone());
    evidence.evidence_path = path;
    evidence.user_message()
}

fn finish_launch_lifecycle<R: Runtime>(app: &AppHandle<R>) {
    set_launcher_process(app, None);
    set_tray_mode(app, false);
    emit_runtime_state(
        app,
        engine::runtime::RuntimeState {
            active: false,
            origin: engine::runtime::ProcessOrigin::Launcher,
        },
    );
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        resume_webview(app);
    }
    let _ = app.emit("onGameLaunchFinished", ());
}

fn emit_runtime_state<R: Runtime>(app: &AppHandle<R>, state: engine::runtime::RuntimeState) {
    let origin = match state.origin {
        engine::runtime::ProcessOrigin::Launcher => "launcher",
        engine::runtime::ProcessOrigin::External => "external",
    };
    let _ = app.emit(
        "onGameRuntimeState",
        serde_json::json!({"active": state.active, "origin": origin}),
    );
}

fn spawn_runtime_monitor<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut previous = None;
        loop {
            interval.tick().await;
            if app.get_webview_window("main").is_none() {
                break;
            }
            let detected_pid = engine::runtime::find_game_process_id();
            let state = engine::runtime::reconcile_runtime_state(
                coordinator_launcher_pid(&app),
                detected_pid,
            );
            if previous != Some(state) {
                emit_runtime_state(&app, state);
                previous = Some(state);
            }
            if detected_pid.is_none() && coordinator_launcher_pid(&app).is_some() {
                set_launcher_process(&app, None);
            }
        }
    });
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
    let app = window.app_handle();
    match window_minimize_action(is_tray_mode(app)) {
        WindowMinimizeAction::Minimize => {
            let _ = window.minimize();
        }
        WindowMinimizeAction::Hide => {
            set_tray_mode(app, true);
            let _ = window.hide();
            suspend_webview(app);
            notify_tray_minimized(app);
        }
    }
}

#[tauri::command]
fn close_window<R: Runtime>(window: WebviewWindow<R>) {
    request_close(window.app_handle());
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
    let normalized = engine::settings::normalize_settings_json(&settings_json);
    let path = get_settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    for diagnostic in &normalized.diagnostics {
        log::warn!("Settings normalized while saving: {}", diagnostic);
    }
    let serialized = serde_json::to_string(&normalized.settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    std::fs::write(&path, serialized).map_err(|e| format!("Failed to save settings: {e}"))?;
    log::info!("Settings saved to {:?}", path);
    Ok(())
}

#[tauri::command]
fn load_settings() -> Result<engine::settings::SettingsLoadResult, String> {
    let path = get_settings_path();
    let result = if path.exists() {
        let raw =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {e}"))?;
        engine::settings::normalize_settings_json(&raw)
    } else {
        engine::settings::SettingsLoadResult {
            settings: engine::settings::LauncherSettings::default(),
            repaired: false,
            diagnostics: Vec::new(),
        }
    };

    if result.repaired || !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized = serde_json::to_string(&result.settings)
            .map_err(|e| format!("Failed to serialize default settings: {e}"))?;
        std::fs::write(&path, serialized).map_err(|e| format!("Failed to repair settings: {e}"))?;
    }
    Ok(result)
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
    let total_len = match std::fs::metadata(&file_path) {
        Ok(metadata) => metadata.len(),
        Err(_) => return Response::builder().status(404).body(vec![]).unwrap(),
    };

    if let Some(range_val) = request.headers().get("range").and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range_header(range_val, total_len) {
            let Ok(data) = read_media_range(&file_path, start, end) else {
                return Response::builder().status(404).body(vec![]).unwrap();
            };
            return Response::builder()
                .status(206)
                .header("Content-Type", mime)
                .header(
                    "Content-Range",
                    format!("bytes {}-{}/{}", start, end, total_len),
                )
                .header("Content-Length", (end - start + 1).to_string())
                .header("Accept-Ranges", "bytes")
                .header("Access-Control-Allow-Origin", "*")
                .body(data)
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

    let full_data = if total_len == 0 {
        Vec::new()
    } else {
        let Ok(data) = read_media_range(&file_path, 0, total_len - 1) else {
            return Response::builder().status(404).body(vec![]).unwrap();
        };
        data
    };
    Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Content-Length", total_len.to_string())
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .body(full_data)
        .unwrap()
}

fn read_media_range(path: &Path, start: u64, end: u64) -> Result<Vec<u8>, std::io::Error> {
    if end < start {
        return Ok(Vec::new());
    }
    let length = end - start + 1;
    let capacity = usize::try_from(length).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "media range too large")
    })?;
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut data = Vec::with_capacity(capacity);
    file.take(length).read_to_end(&mut data)?;
    if data.len() != capacity {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "media range shorter than requested",
        ));
    }
    Ok(data)
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

fn get_installed_patch_version() -> Option<String> {
    let path = get_appdata_dir().join("versions.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|json| {
            json.get("_vhVersion")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .filter(|version| !version.trim().is_empty())
}

fn get_launcher_release_notes_path() -> PathBuf {
    get_appdata_dir().join("launcher-release-notes.json")
}

fn launcher_release_note_payload(release: &engine::updater::ReleaseInfo) -> serde_json::Value {
    serde_json::json!({
        "tag": release.tag_name,
        "date": release.date,
        "body": release.body,
        "title": release.title,
        "author": release.author
    })
}

fn validate_launcher_release_note_payload(payload: serde_json::Value) -> Option<serde_json::Value> {
    serde_json::from_value::<engine::atom_feed::ReleaseNoteEntry>(payload)
        .ok()
        .and_then(|entry| engine::atom_feed::validate_release_note(&entry).ok())
        .and_then(|entry| serde_json::to_value(entry).ok())
}

#[tauri::command]
fn get_launcher_release_notes<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let cache_path = get_launcher_release_notes_path();
        let mut had_cached = false;

        if let Ok(content) = std::fs::read_to_string(&cache_path) {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(payload) = validate_launcher_release_note_payload(payload) {
                    let _ = app_handle.emit("onLauncherReleaseNotes", payload);
                    had_cached = true;
                }
            }
        }

        match engine::updater::fetch_latest_release().await {
            Ok(release) => {
                let Some(payload) =
                    validate_launcher_release_note_payload(launcher_release_note_payload(&release))
                else {
                    log::warn!("Latest launcher release notes rejected by validation");
                    return;
                };

                if let Some(parent) = cache_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(content) = serde_json::to_string(&payload) {
                    let _ = std::fs::write(&cache_path, content);
                }
                let _ = app_handle.emit("onLauncherReleaseNotes", payload);
            }
            Err(error) => {
                if !had_cached {
                    log::debug!("Launcher release notes unavailable: {error}");
                }
            }
        }
    });
}

async fn get_latest_patch_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    engine::atom_feed::fetch_latest_release_notes(&client, engine::atom_feed::ATOM_FEED_URL)
        .await
        .ok()
        .map(|entry| entry.tag)
        .filter(|version| !version.trim().is_empty())
}

#[tauri::command]
fn check_launcher_update<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let current_version = env!("CARGO_PKG_VERSION");
        match engine::updater::check_latest_release(current_version).await {
            Ok(Some(release)) => {
                if let (Some(zip), Some(checksums)) = (release.zip_url, release.checksums_url) {
                    if !engine::updater::is_safe_download_url(&zip)
                        || !engine::updater::is_safe_download_url(&checksums)
                    {
                        let _ = app_handle.emit(
                            "onLauncherUpdateError",
                            "Asset update memakai URL yang tidak aman.".to_string(),
                        );
                        return;
                    }
                    let _ = app_handle.emit(
                        "onLauncherUpdateAvailable",
                        serde_json::json!({
                            "version": release.version,
                            "tag": release.tag_name,
                            "body": release.body,
                            "zipUrl": zip,
                            "checksumsUrl": checksums
                        }),
                    );
                } else {
                    let _ = app_handle.emit(
                        "onLauncherUpdateError",
                        "Update launcher ditemukan tetapi ZIP atau checksum asset tidak tersedia."
                            .to_string(),
                    );
                }
            }
            Ok(None) => {
                let _ = app_handle.emit(
                    "onLauncherUpdateStatus",
                    serde_json::json!({
                        "kind": "ok",
                        "message": "Launcher sudah menggunakan versi terbaru."
                    }),
                );
            }
            Err(error) => {
                let _ = app_handle.emit(
                    "onLauncherUpdateError",
                    format!("Gagal memeriksa update launcher: {error}"),
                );
            }
        }
    });
}

#[tauri::command]
fn open_support() -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", SUPPORT_URL])
            .spawn()
            .map_err(|error| format!("Gagal membuka browser dukungan: {error}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(SUPPORT_URL)
            .spawn()
            .map_err(|error| format!("Gagal membuka browser dukungan: {error}"))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(SUPPORT_URL)
            .spawn()
            .map_err(|error| format!("Gagal membuka browser dukungan: {error}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Membuka browser dukungan tidak didukung pada platform ini.".to_string())
}

#[tauri::command]
fn check_and_sync_media<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let cache_dir = get_appdata_dir().join("Cache");
        let cached_valid = engine::media::read_cached_manifest(&cache_dir)
            .ok()
            .flatten()
            .map(|manifest| {
                engine::media::validate_cached_media(&cache_dir, &manifest).unwrap_or(false)
            })
            .unwrap_or(false);

        let _ = app_handle.emit(
            "onMediaStatus",
            serde_json::json!({
                "status": "checking",
                "message": "Memeriksa aset media..."
            }),
        );
        if cached_valid {
            let _ = app_handle.emit(
                "onMediaReady",
                serde_json::json!({
                    "bgmUrl": media_url("bgm.mp3"),
                    "videoUrl": media_url("bg-video.mp4")
                }),
            );
        }

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
                    let _ = app_progress.emit(
                        "onMediaProgress",
                        serde_json::json!({
                            "percent": p.percent,
                            "text": format!("Mengunduh {}", asset_name),
                            "speed": p.speed_mbps,
                            "size": p.status
                        }),
                    );
                })
                .await;

                match res {
                    Ok(_) => {
                        let _ = app_handle.emit(
                            "onMediaReady",
                            serde_json::json!({
                                "bgmUrl": media_url("bgm.mp3"),
                                "videoUrl": media_url("bg-video.mp4")
                            }),
                        );
                        let _ = app_handle.emit(
                            "onMediaStatus",
                            serde_json::json!({
                                "status": "ready",
                                "message": ""
                            }),
                        );
                    }
                    Err(e) => {
                        log::warn!("Media sync error: {}", e);
                        let status = if cached_valid { "offline" } else { "error" };
                        let message = if cached_valid {
                            format!(
                                "Media baru gagal diverifikasi; memakai cache valid. Detail: {e}"
                            )
                        } else {
                            e
                        };
                        let _ = app_handle.emit(
                            "onMediaStatus",
                            serde_json::json!({
                                "status": status,
                                "message": message
                            }),
                        );
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to fetch media manifest: {}", e);
                let _ = app_handle.emit(
                    "onMediaStatus",
                    serde_json::json!({
                        "status": "offline",
                        "message": if cached_valid {
                            format!("Tidak terhubung; media cache tetap digunakan. Detail: {e}")
                        } else {
                            e
                        }
                    }),
                );
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
                    if let Some(entry) =
                        serde_json::from_value::<engine::atom_feed::ReleaseNoteEntry>(
                            cached.clone(),
                        )
                        .ok()
                        .and_then(|entry| engine::atom_feed::validate_release_note(&entry).ok())
                    {
                        let _ = app_handle.emit(
                            "onVHReleaseNotes",
                            serde_json::to_value(entry).unwrap_or_default(),
                        );
                        had_cached = true;
                    } else {
                        log::warn!("Cached release notes rejected by validation");
                    }
                }
            }
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        match engine::atom_feed::fetch_latest_release_notes(
            &client,
            engine::atom_feed::ATOM_FEED_URL,
        )
        .await
        {
            Ok(entry) => {
                let entry = match engine::atom_feed::validate_release_note(&entry) {
                    Ok(entry) => entry,
                    Err(error) => {
                        log::warn!("Fetched release notes rejected by validation: {error}");
                        return;
                    }
                };
                let note_json = serde_json::json!({
                    "tag": entry.tag,
                    "date": entry.date,
                    "body": entry.body,
                    "title": entry.title,
                    "author": entry.author
                });

                // Persist to versions.json cache
                let mut map = if let Ok(c) = std::fs::read_to_string(&versions_path) {
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&c)
                        .unwrap_or_default()
                } else {
                    serde_json::Map::new()
                };
                map.insert("_cachedReleaseNotes".to_string(), note_json.clone());
                let _ = std::fs::write(
                    &versions_path,
                    serde_json::to_string(&map).unwrap_or_default(),
                );

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

fn cleanup_update_artifacts(temp_zip: &Path, staging: &Path, handoff: &Path) {
    let _ = std::fs::remove_file(temp_zip);
    let _ = std::fs::remove_dir_all(staging);
    let _ = std::fs::remove_file(handoff);
}

#[tauri::command]
fn perform_launcher_update<R: Runtime>(
    app: AppHandle<R>,
    version: String,
    zip_url: String,
    checksums_url: Option<String>,
) {
    log::info!(
        "Perform launcher update requested: {} -> {}",
        version,
        zip_url
    );
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let temp_zip = get_appdata_dir().join("update.zip");
        if !engine::updater::is_newer_version(env!("CARGO_PKG_VERSION"), &version) {
            let _ = app_handle.emit(
                "onLauncherUpdateError",
                "Versi update tidak lebih baru dari launcher saat ini.".to_string(),
            );
            return;
        }
        if !engine::updater::is_safe_download_url(&zip_url) {
            let _ = app_handle.emit(
                "onLauncherUpdateError",
                "URL ZIP update tidak aman.".to_string(),
            );
            return;
        }
        let checksums_url = match checksums_url {
            Some(url) if engine::updater::is_safe_download_url(&url) => url,
            _ => {
                let _ = app_handle.emit(
                    "onLauncherUpdateError",
                    "Checksum update wajib tersedia dari URL HTTPS.".to_string(),
                );
                return;
            }
        };
        let staging = get_appdata_dir().join(".staging");
        let handoff_path = get_appdata_dir().join("update-handoff.cmd");
        cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
        let app_progress = app_handle.clone();

        let res = engine::downloader::download_file(&zip_url, &temp_zip, move |p| {
            let _ = app_progress.emit(
                "onLauncherUpdateProgress",
                serde_json::json!({
                    "percent": p.percent,
                    "status": p.status
                }),
            );
        })
        .await;

        match res {
            Ok(()) => {
                let verified_zip = async {
                    let zip_data = std::fs::read(&temp_zip)
                        .map_err(|error| format!("Gagal membaca ZIP update: {error}"))?;
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(15))
                        .build()
                        .map_err(|error| format!("Gagal membuat client checksum: {error}"))?;
                    let checksums = client
                        .get(&checksums_url)
                        .send()
                        .await
                        .map_err(|error| format!("Gagal mengambil checksum update: {error}"))?
                        .text()
                        .await
                        .map_err(|error| format!("Gagal membaca checksum update: {error}"))?;
                    let file_name = zip_url
                        .split('?')
                        .next()
                        .and_then(|url| Path::new(url).file_name())
                        .and_then(|name| name.to_str())
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| "Nama file ZIP update tidak valid.".to_string())?;
                    let expected = engine::updater::parse_checksum_manifest(&checksums)
                        .get(file_name)
                        .cloned()
                        .ok_or_else(|| format!("Checksum untuk {file_name} tidak ditemukan."))?;
                    let actual = engine::downloader::compute_sha256(&temp_zip)
                        .map_err(|error| format!("Gagal menghitung checksum update: {error}"))?;
                    if actual != expected {
                        return Err("Checksum ZIP update tidak cocok.".to_string());
                    }
                    Ok::<Vec<u8>, String>(zip_data)
                }
                .await;
                match verified_zip.and_then(|zip_data| {
                    if staging.exists() {
                        std::fs::remove_dir_all(&staging)
                            .map_err(|error| format!("Gagal membersihkan staging lama: {error}"))?;
                    }
                    engine::updater::extract_zip_update(&zip_data, &staging)
                }) {
                    Ok(exe_path) => {
                        let current_exe = match std::env::current_exe() {
                            Ok(path) => path,
                            Err(error) => {
                                cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                                let _ = app_handle.emit(
                                    "onLauncherUpdateError",
                                    format!(
                                        "Executable launcher saat ini tidak ditemukan: {error}"
                                    ),
                                );
                                return;
                            }
                        };
                        let handoff_path = get_appdata_dir().join("update-handoff.cmd");
                        if let Err(error) = engine::updater::create_update_handoff(
                            &staging,
                            &current_exe,
                            &handoff_path,
                        ) {
                            cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                            let _ = app_handle.emit(
                                "onLauncherUpdateError",
                                format!("Gagal menyiapkan restart update: {error}"),
                            );
                            return;
                        }
                        let _ = app_handle.emit(
                            "onLauncherUpdateProgress",
                            serde_json::json!({
                                "percent": 100,
                                "status": "Update terverifikasi dan siap diterapkan."
                            }),
                        );
                        log::info!("Update staged at {:?}", exe_path);
                        let _ = app_handle.emit("onLauncherUpdateStaged", ());
                        for remaining_seconds in launcher_update_restart_countdown() {
                            let _ = app_handle.emit(
                                "onLauncherUpdateRestarting",
                                serde_json::json!({"remainingSeconds": remaining_seconds}),
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                        #[cfg(windows)]
                        {
                            use std::os::windows::process::CommandExt;
                            let handoff_arg = handoff_path.to_string_lossy().to_string();
                            match std::process::Command::new("cmd")
                                .args(["/C", handoff_arg.as_str()])
                                .creation_flags(0x08000000)
                                .spawn()
                            {
                                Ok(_) => {
                                    let _ = app_handle.emit("onLauncherUpdateRestarting", ());
                                    app_handle.exit(0);
                                }
                                Err(error) => {
                                    cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                                    let _ = app_handle.emit(
                                        "onLauncherUpdateError",
                                        format!("Gagal menjalankan restart update: {error}"),
                                    );
                                }
                            }
                        }
                        #[cfg(not(windows))]
                        {
                            cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                            let _ = app_handle.emit(
                                "onLauncherUpdateError",
                                "Self-update handoff hanya tersedia pada Windows.".to_string(),
                            );
                        }
                        #[cfg(windows)]
                        let _ = std::fs::remove_file(&temp_zip);
                    }
                    Err(error) => {
                        cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                        let _ = app_handle.emit(
                            "onLauncherUpdateError",
                            format!("Gagal menyiapkan update launcher: {error}"),
                        );
                    }
                }
            }
            Err(error) => {
                cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
                let _ = app_handle.emit(
                    "onLauncherUpdateError",
                    format!("Gagal mengunduh update launcher: {error}"),
                );
            }
        }
    });
}

#[tauri::command]
async fn check_patch_status<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    install_method: String,
) -> Result<(), String> {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            let _ = app.emit(
                "onPatchStatus",
                serde_json::json!({
                    "status": "invalid",
                    "gamePath": game_path,
                    "installMethod": install_method,
                    "message": error
                }),
            );
            return Ok(());
        }
    };

    let normalized_path = match engine::path::normalize_game_path(&game_path) {
        Some(path) => path,
        None => {
            let _ = app.emit(
                "onPatchStatus",
                serde_json::json!({
                    "status": "invalid",
                    "gamePath": game_path,
                    "installMethod": method.as_str(),
                    "message": "Folder game tidak valid atau executable game tidak ditemukan."
                }),
            );
            return Ok(());
        }
    };

    let local = engine::patch_status::classify_installation(&normalized_path, method)
        .map_err(|error| format!("Gagal memeriksa instalasi patch: {error}"))?;
    let current_version = get_installed_patch_version();
    let latest_version = if matches!(local, engine::patch_status::LocalPatchState::Ready) {
        get_latest_patch_version().await
    } else {
        None
    };
    let status = engine::patch_status::resolve_patch_status(
        local,
        current_version.as_deref(),
        latest_version.as_deref(),
    );

    let _ = app.emit(
        "onPatchStatus",
        serde_json::json!({
            "status": status.as_str(),
            "gamePath": normalized_path,
            "installMethod": method.as_str(),
            "currentVersion": current_version,
            "latestVersion": latest_version
        }),
    );
    Ok(())
}

#[tauri::command]
fn notify_ui_interactive<R: Runtime>(app: AppHandle<R>, install_method: String) {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            log::warn!(
                "Install method active player tidak valid ({error}); memakai resource_mount"
            );
            engine::method::InstallMethod::ResourceMount
        }
    };
    if let Some(service) = app.try_state::<engine::active_player::ActivePlayerService>() {
        service.start(method);
    }
    log::info!("UI interactive milestone reached");
}

#[tauri::command]
fn reset_webview_cache<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    log::info!("Reset webview cache requested");
    let cache_dir = get_appdata_dir().join("Cache");
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|error| format!("Gagal menghapus cache WebView: {error}"))?;
    }
    let _ = app.emit(
        "onMediaStatus",
        serde_json::json!({"status": "checking", "message": "Cache direset; memulai sinkronisasi media..."}),
    );
    check_and_sync_media(app);
    Ok(())
}

#[tauri::command]
fn start_installation<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    _vh_mode: String,
    install_method: String,
) {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            let _ = app.emit("onInstallError", error);
            return;
        }
    };
    let normalized_game_path =
        match engine::installer::validate_installation_preconditions(&game_path, method) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(error) => {
                let _ = app.emit("onInstallError", error);
                return;
            }
        };
    let canonical_method = method.as_str().to_string();
    log::info!(
        "Start installation: path={}, method={}",
        normalized_game_path,
        canonical_method
    );

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let game_path = normalized_game_path;
        let p = Path::new(&game_path);

        let _ = app_handle.emit(
            "onProgressUpdate",
            serde_json::json!({
                "percent": 5,
                "status": "Memeriksa rilis mod terbaru..."
            }),
        );

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
        let patch_version = get_latest_patch_version()
            .await
            .unwrap_or_else(|| "unknown".to_string());

        let cache_pak = get_appdata_dir()
            .join("Cache")
            .join(engine::path::PAK_FILE_NAME);
        if let Some(parent) = cache_pak.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let target_asset_name = engine::path::PAK_FILE_NAME;
        let expected_hash = checksums
            .get(target_asset_name)
            .cloned()
            .unwrap_or_default();

        if expected_hash.is_empty() {
            log::error!("Checksum for {} not found in manifest", target_asset_name);
            let _ = app_handle.emit(
                "onInstallError",
                format!(
                    "Checksum SHA-256 untuk file {} tidak ditemukan pada server release.",
                    target_asset_name
                ),
            );
            return;
        }

        let mut need_download = true;
        if cache_pak.exists()
            && engine::downloader::verify_sha256(&cache_pak, &expected_hash).unwrap_or(false)
        {
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
                    let _ = app_handle.emit(
                        "onInstallError",
                        format!("Gagal memverifikasi metadata asset rilis: {}", e),
                    );
                    return;
                }
            };

            let dl_res = engine::downloader::download_file_with_expected_size(
                &pak_url,
                &cache_pak,
                Some(content_len),
                move |prog| {
                    let _ = app_progress.emit(
                        "onProgressUpdate",
                        serde_json::json!({
                            "percent": (prog.percent as f32 * 0.85) as u8,
                            "status": format!("Mengunduh patch... {}", prog.status),
                            "downloadedBytes": prog.downloaded_bytes,
                            "totalBytes": prog.total_bytes,
                            "speedMbps": prog.speed_mbps
                        }),
                    );
                },
            )
            .await;

            if let Err(e) = dl_res {
                log::error!("Patch download failed: {}", e);
                let _ = app_handle.emit(
                    "onInstallError",
                    format!("Gagal mengunduh patch mod: {}", e),
                );
                return;
            }

            if !engine::downloader::verify_sha256(&cache_pak, &expected_hash).unwrap_or(false) {
                let _ = std::fs::remove_file(&cache_pak);
                let _ = app_handle.emit(
                    "onInstallError",
                    "Integritas file patch gagal diverifikasi (SHA-256 mismatch).".to_string(),
                );
                return;
            }
        }

        let loader_cache = if method == engine::method::InstallMethod::Loader {
            let loader_cache = get_appdata_dir().join("Cache").join("winhttp.dll");
            let loader_hash = checksums.get("winhttp.dll").cloned().unwrap_or_default();
            if loader_hash.is_empty() {
                let _ = app_handle.emit(
                    "onInstallError",
                    "Checksum SHA-256 untuk loader winhttp.dll tidak ditemukan pada manifest rilis."
                        .to_string(),
                );
                return;
            }

            let mut need_loader_download = true;
            if loader_cache.exists()
                && engine::downloader::verify_sha256(&loader_cache, &loader_hash).unwrap_or(false)
            {
                need_loader_download = false;
            }
            if need_loader_download {
                let loader_url = format!("{}{}", WUWAID_LATEST_DOWNLOAD_BASE_URL, "winhttp.dll");
                let loader_len =
                    match engine::downloader::get_asset_content_length(&loader_url).await {
                        Ok(len) => len,
                        Err(error) => {
                            let _ = app_handle.emit(
                                "onInstallError",
                                format!("Gagal memeriksa metadata loader winhttp.dll: {error}"),
                            );
                            return;
                        }
                    };
                if let Err(error) = engine::downloader::download_file_with_expected_size(
                    &loader_url,
                    &loader_cache,
                    Some(loader_len),
                    |_| {},
                )
                .await
                {
                    let _ = app_handle.emit(
                        "onInstallError",
                        format!("Gagal mengunduh loader winhttp.dll: {error}"),
                    );
                    return;
                }
                if !engine::downloader::verify_sha256(&loader_cache, &loader_hash).unwrap_or(false)
                {
                    let _ = std::fs::remove_file(&loader_cache);
                    let _ = app_handle.emit(
                        "onInstallError",
                        "Integritas hash winhttp.dll gagal diverifikasi (SHA-256 mismatch)."
                            .to_string(),
                    );
                    return;
                }
            }
            Some(loader_cache)
        } else {
            None
        };

        let _ = app_handle.emit(
            "onProgressUpdate",
            serde_json::json!({
                "percent": 90,
                "status": "Memasang file mod..."
            }),
        );

        if let Err(error) = engine::installer::install_patch_transaction(
            p,
            method,
            &cache_pak,
            loader_cache.as_deref(),
        ) {
            let _ = app_handle.emit("onInstallError", error);
            return;
        }

        // Save metadata to versions.json
        let mut ver_map = serde_json::Map::new();
        ver_map.insert(
            "_vhVersion".to_string(),
            serde_json::Value::String(patch_version),
        );
        ver_map.insert(
            "_installMethod".to_string(),
            serde_json::Value::String(canonical_method),
        );
        let versions_path = get_appdata_dir().join("versions.json");
        if let Some(parent) = versions_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized = match serde_json::to_string(&ver_map) {
            Ok(serialized) => serialized,
            Err(error) => {
                let _ = app_handle.emit(
                    "onInstallError",
                    format!("Gagal menyusun metadata instalasi: {error}"),
                );
                return;
            }
        };
        if let Err(error) = std::fs::write(&versions_path, serialized) {
            let _ = app_handle.emit(
                "onInstallError",
                format!("Gagal menyimpan metadata instalasi: {error}"),
            );
            return;
        }

        let _ = app_handle.emit(
            "onProgressUpdate",
            serde_json::json!({
                "percent": 100,
                "status": "Instalasi selesai!"
            }),
        );
        let _ = app_handle.emit("onInstallComplete", ());
    });
}

#[tauri::command]
fn check_game_folder_write_access(
    game_path: String,
    install_method: String,
    _for_installation: bool,
) -> String {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(_) => return "invalid_method".to_string(),
    };
    match engine::installer::validate_installation_preconditions(&game_path, method) {
        Ok(_) => "ok".to_string(),
        Err(error) => error.split(':').next().unwrap_or("needs_admin").to_string(),
    }
}

#[tauri::command]
fn launch_game<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    dx11: bool,
    install_method: String,
) -> Result<(), String> {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            let _ = app.emit("onLaunchError", error.clone());
            return Err(error);
        }
    };
    let normalized_game_path =
        match engine::runtime::validate_launch_preconditions(&game_path, method) {
            Ok(path) => path,
            Err(error) => {
                let _ = app.emit("onLaunchError", error.clone());
                return Err(error);
            }
        };
    let canonical_method = method.as_str().to_string();
    log::info!(
        "Launch game: path={}, dx11={}, method={}",
        normalized_game_path.display(),
        dx11,
        canonical_method
    );
    let app_handle = app.clone();
    let p = normalized_game_path;

    tauri::async_runtime::spawn(async move {
        let _ = app_handle.emit("onGameLaunchStarted", ());
        let command = engine::runtime::build_launch_command(&p, dx11);

        match engine::runtime::launch_game(&p, dx11) {
            Ok(mut process) => {
                if let Some(service) =
                    app_handle.try_state::<engine::active_player::ActivePlayerService>()
                {
                    service.send_launch(method);
                }
                let mut evidence = engine::runtime::LaunchEvidence::for_process(
                    command,
                    process.mode,
                    process.id(),
                );
                // Hide immediately after a successful spawn. Process discovery is
                // still handled by the runtime monitor and no longer delays this.
                if let Some(window) = app_handle.get_webview_window("main") {
                    set_tray_mode(&app_handle, true);
                    let _ = window.hide();
                    suspend_webview(&app_handle);
                    notify_tray_minimized(&app_handle);
                }
                engine::runtime::trim_memory_working_set();

                match process.try_wait() {
                    Ok(Some(result)) => {
                        evidence.failure_kind =
                            Some(engine::runtime::SpawnFailureKind::ImmediateExit);
                        evidence.exit_code = result.exit_code;
                        evidence.stdout = result.stdout;
                        evidence.stderr = result.stderr;
                        evidence.game_log_tail = engine::runtime::collect_game_log_tail(&p);
                        evidence.mark_finished();
                        let _ = app_handle.emit("onLaunchError", launch_error_message(evidence));
                        finish_launch_lifecycle(&app_handle);
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        evidence.failure_kind =
                            Some(engine::runtime::SpawnFailureKind::SpawnFailed);
                        evidence.error = Some(error);
                        evidence.game_log_tail = engine::runtime::collect_game_log_tail(&p);
                        let _ = app_handle.emit("onLaunchError", launch_error_message(evidence));
                        finish_launch_lifecycle(&app_handle);
                        return;
                    }
                }

                evidence.mark_detected();
                set_launcher_process(&app_handle, Some(process.id()));
                emit_runtime_state(
                    &app_handle,
                    engine::runtime::RuntimeState {
                        active: true,
                        origin: engine::runtime::ProcessOrigin::Launcher,
                    },
                );

                // Monitor process in background and persist final evidence.
                let app_for_monitor = app_handle.clone();
                let p_for_monitor = p.clone();
                tokio::task::spawn_blocking(move || {
                    let process_result = process.wait();
                    match process_result {
                        Ok(result) => {
                            evidence.exit_code = result.exit_code;
                            evidence.stdout = result.stdout;
                            evidence.stderr = result.stderr;
                            evidence.game_log_tail =
                                engine::runtime::collect_game_log_tail(&p_for_monitor);
                            evidence.mark_finished();
                            let force_quit = take_force_quit_requested(&app_for_monitor);
                            if !force_quit && result.exit_code.unwrap_or(0) != 0 {
                                evidence.failure_kind =
                                    Some(engine::runtime::SpawnFailureKind::ProcessCrashed);
                                evidence.error = Some(
                                    "game process exited with a non-zero exit code".to_string(),
                                );
                                let _ = app_for_monitor
                                    .emit("onLaunchError", launch_error_message(evidence.clone()));
                            } else {
                                let _ = save_launch_evidence(evidence.clone());
                            }
                        }
                        Err(error) => {
                            evidence.failure_kind =
                                Some(engine::runtime::SpawnFailureKind::ProcessCrashed);
                            evidence.error = Some(error);
                            evidence.game_log_tail =
                                engine::runtime::collect_game_log_tail(&p_for_monitor);
                            let _ = app_for_monitor
                                .emit("onLaunchError", launch_error_message(evidence.clone()));
                        }
                    }
                    finish_launch_lifecycle(&app_for_monitor);
                });
            }
            Err(mut error) => {
                error.evidence.game_log_tail = engine::runtime::collect_game_log_tail(&p);
                let _ = app_handle.emit("onLaunchError", launch_error_message(error.evidence));
                finish_launch_lifecycle(&app_handle);
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn force_quit_game<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    if coordinator_launcher_pid(&app).is_some() {
        mark_force_quit_requested(&app);
    }
    let terminated = match engine::runtime::force_quit_game() {
        Ok(terminated) => terminated,
        Err(error) => {
            if engine::runtime::find_game_process_id().is_none() {
                set_launcher_process(&app, None);
            }
            let _ = app.emit("onLaunchError", format!("force_quit_failed: {error}"));
            return Err(error);
        }
    };
    set_launcher_process(&app, None);
    set_tray_mode(&app, false);
    emit_runtime_state(
        &app,
        engine::runtime::RuntimeState {
            active: false,
            origin: engine::runtime::ProcessOrigin::External,
        },
    );
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        resume_webview(&app);
    }
    let _ = app.emit("onGameLaunchFinished", ());
    log::info!("Force quit game requested");
    Ok(terminated)
}

#[tauri::command]
fn switch_method(
    game_path: String,
    new_method: String,
) -> Result<engine::installer::CleanupReport, String> {
    let method = engine::method::InstallMethod::parse(&new_method)?;
    let normalized = engine::installer::validate_installation_preconditions(&game_path, method)?;
    log::info!(
        "Switching method for {} to {}",
        normalized.display(),
        method
    );
    let versions_path = get_appdata_dir().join("versions.json");
    let report = engine::installer::cleanup_owned_artifacts_with_commit(&normalized, None, || {
        if versions_path.exists() {
            let original = std::fs::read(&versions_path)
                .map_err(|error| format!("metadata_read_failed: {error}"))?;
            let content = String::from_utf8(original.clone())
                .map_err(|error| format!("metadata_read_failed: {error}"))?;
            let mut json =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content)
                    .map_err(|error| format!("metadata_parse_failed: {error}"))?;
            json.insert(
                "_installMethod".to_string(),
                serde_json::Value::String(method.as_str().to_string()),
            );
            let serialized = serde_json::to_string(&json)
                .map_err(|error| format!("metadata_encode_failed: {error}"))?;
            if let Err(error) = std::fs::write(&versions_path, serialized) {
                let restore = std::fs::write(&versions_path, original);
                return Err(match restore {
                    Ok(()) => format!("metadata_write_failed: {error}"),
                    Err(restore_error) => format!(
                        "metadata_write_failed: {error}; metadata_rollback_failed: {restore_error}"
                    ),
                });
            }
        }
        Ok(())
    })?;
    Ok(report)
}

#[tauri::command]
fn uninstall(game_path: String) -> Result<String, String> {
    let method = engine::method::InstallMethod::ResourceMount;
    let normalized = engine::installer::validate_installation_preconditions(&game_path, method)
        .or_else(|_| {
            engine::path::normalize_game_path(&game_path)
                .ok_or_else(|| "invalid_game_path: executable game tidak ditemukan".to_string())
        })?;
    let versions_path = get_appdata_dir().join("versions.json");
    let _report =
        engine::installer::cleanup_owned_artifacts_with_commit(&normalized, None, || {
            if versions_path.exists() {
                std::fs::remove_file(&versions_path)
                    .map_err(|error| format!("metadata_remove_failed: {error}"))?;
            }
            Ok(())
        })?;
    log::info!("Uninstall patch completed for: {}", normalized.display());
    Ok("ok".to_string())
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
        .manage(RuntimeCoordinator::default())
        .manage(engine::active_player::ActivePlayerService::new(
            get_appdata_dir(),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .register_uri_scheme_protocol("media", media_protocol_handler)
        .setup(|app| {
            restore_legacy_signature_from_settings();
            let app_handle = app.handle().clone();
            configure_webview_memory_target(&app_handle);
            spawn_runtime_monitor(app_handle.clone());

            // Tray icon setup
            let quit_i = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Buka Launcher", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => request_close(app),
                    "show" => {
                        set_tray_mode(app, true);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            resume_webview(app);
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
                        set_tray_mode(app, true);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            resume_webview(app);
                        }
                    }
                })
                .build(app)?;

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
            open_support,
            get_vh_release_notes,
            get_launcher_release_notes,
            perform_launcher_update,
            check_patch_status,
            switch_method,
            notify_ui_interactive,
            reset_webview_cache,
            start_installation,
            check_game_folder_write_access,
            launch_game,
            force_quit_game,
            uninstall,
            restart_as_admin,
        ])
        .build(tauri::generate_context!())
        .expect("error while building wuwaid launcher application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                restore_legacy_signature_from_settings();
                if let Some(service) = app.try_state::<engine::active_player::ActivePlayerService>()
                {
                    service.stop();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;
    use std::time::Duration;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{assert_ipc_response, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{Emitter, Listener};

    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_test_environment() -> std::sync::MutexGuard<'static, ()> {
        TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn ipc_request(command: &str, args: serde_json::Value) -> InvokeRequest {
        InvokeRequest {
            cmd: command.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: tauri::Url::parse(if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            })
            .unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        }
    }

    #[test]
    fn window_minimize_action_distinguishes_normal_and_tray_modes() {
        assert_eq!(
            window_minimize_action(false),
            WindowMinimizeAction::Minimize
        );
        assert_eq!(window_minimize_action(true), WindowMinimizeAction::Hide);
    }

    #[test]
    fn tray_notification_body_is_explicit() {
        assert_eq!(
            tray_notification_body(),
            "Launcher berjalan di system tray. Klik ikon tray untuk membukanya kembali."
        );
    }

    #[test]
    fn launcher_update_restart_countdown_matches_main_branch() {
        assert_eq!(
            launcher_update_restart_countdown().collect::<Vec<_>>(),
            (1..=12).rev().collect::<Vec<_>>()
        );
    }

    #[test]
    fn launcher_release_note_payload_uses_launcher_metadata() {
        let release = engine::updater::ReleaseInfo {
            tag_name: "v2.6.2".to_string(),
            version: "2.6.2".to_string(),
            title: "WuwaID Launcher v2.6.2".to_string(),
            date: "2026-08-18T12:00:00Z".to_string(),
            author: "TitoTFP".to_string(),
            body: "## Launcher changes".to_string(),
            zip_url: None,
            checksums_url: None,
        };

        let payload = launcher_release_note_payload(&release);
        assert_eq!(payload["tag"], "v2.6.2");
        assert_eq!(payload["title"], "WuwaID Launcher v2.6.2");
        assert_eq!(payload["body"], "## Launcher changes");
        assert_eq!(payload["date"], "2026-08-18T12:00:00Z");
        assert_eq!(payload["author"], "TitoTFP");
    }

    #[test]
    fn test_real_tauri_command_path_and_event_delivery() {
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                get_app_version,
                check_game_folder_write_access
            ])
            .build(tauri::generate_context!())
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        assert_ipc_response(
            &window,
            ipc_request("get_app_version", serde_json::json!({})),
            Ok(env!("CARGO_PKG_VERSION").to_string()),
        );
        let tmp = tempfile::tempdir().unwrap();
        let exe_dir = tmp.path().join("Client").join("Binaries").join("Win64");
        let resource_dir = tmp
            .path()
            .join("Client")
            .join("Saved")
            .join("Resources")
            .join("2.6.0");
        let mount_dir = resource_dir.join("Mount");
        let official_dir = resource_dir.join("Lang_en").join("Base");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&mount_dir).unwrap();
        std::fs::create_dir_all(&official_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        std::fs::write(resource_dir.join("ResManifest"), b"manifest").unwrap();
        let official_pak = official_dir.join("pakchunk10-WindowsNoEditor.pak");
        let official_sig = official_dir.join("pakchunk10-WindowsNoEditor.sig");
        std::fs::write(&official_pak, b"OFFICIAL_RESOURCE_PAK").unwrap();
        std::fs::write(&official_sig, b"OFFICIAL_RESOURCE_SIG").unwrap();
        std::fs::write(
            mount_dir.join("MountLang_en.txt"),
            format!(
                "::Mount::\nLang_en/Base/pakchunk10-WindowsNoEditor,4,{},{},,\n::Del::\n",
                engine::installer::compute_sha1(&official_pak).unwrap(),
                engine::installer::compute_sha1(&official_sig).unwrap(),
            ),
        )
        .unwrap();
        assert_ipc_response(
            &window,
            ipc_request(
                "check_game_folder_write_access",
                serde_json::json!({
                    "gamePath": tmp.path().to_string_lossy(),
                    "installMethod": "resource_mount",
                    "forInstallation": true,
                }),
            ),
            Ok("ok".to_string()),
        );

        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onMediaReady", move |event| {
            let _ = tx.send(event.payload().to_string());
        });
        app.emit(
            "onMediaReady",
            serde_json::json!({
                "bgmUrl": media_url("bgm.mp3"),
                "videoUrl": media_url("bg-video.mp4"),
            }),
        )
        .unwrap();
        let payload = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload).unwrap()["videoUrl"],
            media_url("bg-video.mp4")
        );
        app.unlisten(listener);
    }

    #[tokio::test]
    async fn test_media_command_failure_emits_status_without_ready() {
        let _env_lock = lock_test_environment();
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

    #[tokio::test]
    async fn test_media_command_emits_ready_immediately_when_cached() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let cache_dir = appdata.path().join("Cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("bgm.mp3"), b"mock-bgm-audio").unwrap();
        std::fs::write(cache_dir.join("bg-video.mp4"), b"mock-bg-video").unwrap();
        engine::media::write_cached_manifest(
            &cache_dir,
            &engine::media::AssetManifest {
                update_date: None,
                assets: vec![
                    engine::media::AssetEntry {
                        name: "bgm.mp3".to_string(),
                        url: "http://127.0.0.1/bgm.mp3".to_string(),
                        sha256: engine::downloader::compute_sha256(&cache_dir.join("bgm.mp3"))
                            .unwrap(),
                    },
                    engine::media::AssetEntry {
                        name: "bg-video.mp4".to_string(),
                        url: "http://127.0.0.1/bg-video.mp4".to_string(),
                        sha256: engine::downloader::compute_sha256(&cache_dir.join("bg-video.mp4"))
                            .unwrap(),
                    },
                ],
            },
        )
        .unwrap();

        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());
        std::env::set_var("WUWAID_ASSETS_URL", "http://127.0.0.1:9/unreachable");
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (ready_tx, ready_rx) = sync_channel(1);
        let ready_listener = app.listen_any("onMediaReady", move |event| {
            let _ = ready_tx.send(event.payload().to_string());
        });

        check_and_sync_media(app.handle().clone());
        let payload = ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let expected_bgm_url = if cfg!(windows) {
            "http://media.localhost/bgm.mp3"
        } else {
            "media://localhost/bgm.mp3"
        };
        let expected_video_url = if cfg!(windows) {
            "http://media.localhost/bg-video.mp4"
        } else {
            "media://localhost/bg-video.mp4"
        };
        assert_eq!(json["bgmUrl"], expected_bgm_url);
        assert_eq!(json["videoUrl"], expected_video_url);

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
        assert_eq!(
            response.headers().get("Content-Range").unwrap(),
            "bytes 2-5/10"
        );

        let missing = tauri::http::Request::builder()
            .uri("media://localhost/missing.mp4")
            .body(Vec::new())
            .unwrap();
        assert_eq!(
            registered_media_protocol_response(appdata.path(), missing).status(),
            404
        );

        let invalid_range = tauri::http::Request::builder()
            .uri("media://localhost/bg-video.mp4")
            .header("Range", "bytes=99-100")
            .body(Vec::new())
            .unwrap();
        let invalid_response = registered_media_protocol_response(appdata.path(), invalid_range);
        assert_eq!(invalid_response.status(), 416);
        assert_eq!(
            invalid_response.headers().get("Content-Range").unwrap(),
            "bytes */10"
        );

        let traversal = tauri::http::Request::builder()
            .uri("media://localhost/../outside.mp4")
            .body(Vec::new())
            .unwrap();
        assert_eq!(
            registered_media_protocol_response(appdata.path(), traversal).status(),
            404
        );
    }

    #[test]
    fn test_parse_range_header() {
        let total = 1000;

        assert_eq!(parse_range_header("bytes=0-499", total), Some((0, 499)));
        assert_eq!(parse_range_header("bytes=500-", total), Some((500, 999)));
        assert_eq!(
            parse_range_header("bytes=900-2000", total),
            Some((900, 999))
        );
        assert_eq!(parse_range_header("bytes=-100", total), Some((900, 999)));
        assert_eq!(parse_range_header("items=0-10", total), None);
        assert_eq!(parse_range_header("bytes=500-200", total), None);
        assert_eq!(parse_range_header("bytes=1000-1200", total), None);
        assert_eq!(parse_range_header("bytes=1-2,4-5", total), None);
        assert_eq!(parse_range_header("bytes=0-0", 0), None);
    }

    #[test]
    fn settings_commands_persist_only_normalized_canonical_values() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        save_settings(
            r#"{"installMethod":"method2","bgmVolume":4,"perf":{"shadows":true},"launcherVisualMode":"off"}"#
                .to_string(),
        )
        .unwrap();
        let saved = std::fs::read_to_string(appdata.path().join("settings.json")).unwrap();
        assert!(saved.contains("loader"));
        assert!(!saved.contains("method2"));
        assert!(saved.contains("\"bgmVolume\":1.0"));
        assert!(!saved.contains("perf"));
        assert!(!saved.contains("launcherVisualMode"));

        std::fs::write(appdata.path().join("settings.json"), b"{").unwrap();
        let repaired = load_settings().unwrap();
        assert!(repaired.repaired);
        assert_eq!(
            repaired.settings.install_method,
            engine::method::InstallMethod::ResourceMount
        );
        assert!(
            std::fs::read_to_string(appdata.path().join("settings.json"))
                .unwrap()
                .contains("resource_mount")
        );

        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[tokio::test]
    async fn invalid_patch_method_emits_visible_invalid_status() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onPatchStatus", move |event| {
            let _ = tx.send(event.payload().to_string());
        });

        check_patch_status(app.handle().clone(), String::new(), "bogus".to_string())
            .await
            .unwrap();
        let payload = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["status"], "invalid");
        assert!(json["message"].as_str().unwrap().contains("tidak dikenal"));
        app.unlisten(listener);
    }

    #[test]
    fn switch_method_rejects_invalid_game_path_before_metadata_mutation() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());
        std::fs::write(
            appdata.path().join("versions.json"),
            br#"{"_vhVersion":"3.0.0","_installMethod":"loader"}"#,
        )
        .unwrap();

        let error = switch_method(
            appdata
                .path()
                .join("not-a-game")
                .to_string_lossy()
                .to_string(),
            "loader".to_string(),
        )
        .unwrap_err();
        assert!(error.contains("invalid_game_path"));
        assert_eq!(
            std::fs::read_to_string(appdata.path().join("versions.json")).unwrap(),
            r#"{"_vhVersion":"3.0.0","_installMethod":"loader"}"#
        );
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn switch_method_cleans_unsupported_legacy_artifacts() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        let foreign = pak_dir.join(engine::path::PAK_FILE_NAME);
        let legacy_marker = pak_dir.join(".wuwaid-managed-signature-bypass");
        std::fs::write(&foreign, b"foreign artifact").unwrap();
        std::fs::write(&legacy_marker, b"legacy marker").unwrap();
        let versions = appdata.path().join("versions.json");
        std::fs::write(&versions, br#"{"_installMethod":"loader"}"#).unwrap();

        let report = switch_method(
            game.path().to_string_lossy().to_string(),
            "loader".to_string(),
        )
        .unwrap();
        assert!(report.failures.is_empty());
        assert!(report.preserved.is_empty());
        assert_eq!(
            report.removed,
            vec![
                foreign.to_string_lossy().to_string(),
                legacy_marker.to_string_lossy().to_string(),
            ]
        );
        assert!(!foreign.exists());
        assert!(!legacy_marker.exists());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&versions).unwrap())
                .unwrap()["_installMethod"],
            "loader"
        );
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn startup_signature_migration_restores_backup_only() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        let backup = engine::signature::get_sig_backup_path(game.path());
        let signature = engine::signature::get_sig_path(game.path());
        std::fs::write(&backup, b"ORIGINAL_GAME_SIG").unwrap();
        std::fs::write(
            appdata.path().join("settings.json"),
            serde_json::to_vec(&serde_json::json!({
                "gamePath": game.path().to_string_lossy(),
            }))
            .unwrap(),
        )
        .unwrap();

        restore_legacy_signature_from_settings();

        assert_eq!(std::fs::read(&signature).unwrap(), b"ORIGINAL_GAME_SIG");
        assert!(!backup.exists());
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn startup_signature_migration_ignores_invalid_game_path() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let not_a_game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let pak_dir = not_a_game
            .path()
            .join("Client")
            .join("Content")
            .join("Paks");
        std::fs::create_dir_all(&pak_dir).unwrap();
        let backup = engine::signature::get_sig_backup_path(not_a_game.path());
        std::fs::write(&backup, b"DO_NOT_MOVE").unwrap();
        std::fs::write(
            appdata.path().join("settings.json"),
            serde_json::to_vec(&serde_json::json!({
                "gamePath": not_a_game.path().to_string_lossy(),
            }))
            .unwrap(),
        )
        .unwrap();

        restore_legacy_signature_from_settings();

        assert!(backup.exists());
        assert!(!engine::signature::get_sig_path(not_a_game.path()).exists());
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn switch_method_rolls_back_filesystem_when_metadata_commit_fails() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        let canonical = pak_dir.join(engine::path::PAK_FILE_NAME);
        std::fs::write(&canonical, b"legacy artifact").unwrap();
        let versions = appdata.path().join("versions.json");
        std::fs::write(&versions, b"not-json").unwrap();

        let error = switch_method(
            game.path().to_string_lossy().to_string(),
            "loader".to_string(),
        )
        .unwrap_err();

        assert!(error.contains("metadata_commit_failed"));
        assert!(canonical.exists());
        assert_eq!(std::fs::read(&versions).unwrap(), b"not-json");
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn switch_method_migrates_legacy_resource_mount_artifacts() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let resources = game
            .path()
            .join("Client")
            .join("Saved")
            .join("Resources")
            .join("3.5.0");
        let mount_dir = resources.join("Mount");
        let official_dir = resources.join("Lang_en").join("Base");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&mount_dir).unwrap();
        std::fs::create_dir_all(&official_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        std::fs::write(resources.join("ResManifest"), b"manifest").unwrap();
        let official_pak = official_dir.join("pakchunk10-WindowsNoEditor.pak");
        let official_sig = official_dir.join("pakchunk10-WindowsNoEditor.sig");
        std::fs::write(&official_pak, b"OFFICIAL_RESOURCE_PAK").unwrap();
        std::fs::write(&official_sig, b"OFFICIAL_RESOURCE_SIG").unwrap();
        std::fs::write(
            mount_dir.join("MountLang_en.txt"),
            format!(
                "::Mount::\nLang_en/Base/pakchunk10-WindowsNoEditor,4,{},{},,\n::Del::\n",
                engine::installer::compute_sha1(&official_pak).unwrap(),
                engine::installer::compute_sha1(&official_sig).unwrap(),
            ),
        )
        .unwrap();

        let plan = engine::installer::probe_resource_mount(game.path()).unwrap();
        std::fs::create_dir_all(plan.pak_path.parent().unwrap()).unwrap();
        std::fs::write(&plan.pak_path, b"legacy-placeholder-pak").unwrap();
        std::fs::write(&plan.sig_path, []).unwrap();
        std::fs::write(&plan.owner_marker_path, b"wuwaid-managed-mod").unwrap();
        let versions = appdata.path().join("versions.json");
        std::fs::write(&versions, br#"{"_installMethod":"resource_mount"}"#).unwrap();

        let report = switch_method(
            game.path().to_string_lossy().to_string(),
            "loader".to_string(),
        )
        .unwrap();

        assert!(report.failures.is_empty());
        assert!(report.preserved.is_empty());
        assert!(!plan.pak_path.exists());
        assert!(!plan.sig_path.exists());
        assert!(!plan.owner_marker_path.exists());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&versions).unwrap())
                .unwrap()["_installMethod"],
            "loader"
        );

        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn uninstall_removes_canonical_artifact_without_marker() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        let foreign = pak_dir.join(engine::path::PAK_FILE_NAME);
        std::fs::write(&foreign, b"foreign artifact").unwrap();
        let versions = appdata.path().join("versions.json");
        std::fs::write(&versions, br#"{"_vhVersion":"3.0.0"}"#).unwrap();

        let result = uninstall(game.path().to_string_lossy().to_string());
        assert_eq!(result.unwrap(), "ok");
        assert!(!foreign.exists());
        assert!(!versions.exists());
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn uninstall_rolls_back_filesystem_when_metadata_remove_fails() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        let canonical = pak_dir.join(engine::path::PAK_FILE_NAME);
        std::fs::write(&canonical, b"legacy artifact").unwrap();
        std::fs::create_dir(appdata.path().join("versions.json")).unwrap();

        let error = uninstall(game.path().to_string_lossy().to_string()).unwrap_err();

        assert!(error.contains("metadata_commit_failed"));
        assert!(canonical.exists());
        assert!(appdata.path().join("versions.json").is_dir());
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn core_commands_run_through_mock_ipc_with_deterministic_results() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());

        let exe_dir = game.path().join("Client").join("Binaries").join("Win64");
        let pak_dir = game.path().join("Client").join("Content").join("Paks");
        let resources = game
            .path()
            .join("Client")
            .join("Saved")
            .join("Resources")
            .join("2.6.0");
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::create_dir_all(&pak_dir).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
        std::fs::write(resources.join("ResManifest"), b"manifest").unwrap();
        let versions = appdata.path().join("versions.json");
        std::fs::write(&versions, br#"{"_installMethod":"loader"}"#).unwrap();

        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                start_installation,
                launch_game,
                check_patch_status,
                switch_method,
                uninstall,
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        assert_ipc_response(
            &window,
            ipc_request(
                "launch_game",
                serde_json::json!({
                    "gamePath": game.path().parent().unwrap().join("missing-game").to_string_lossy(),
                    "dx11": false,
                    "installMethod": "loader",
                }),
            ),
            Err("invalid_game_path: executable game tidak ditemukan".to_string()),
        );

        let (install_error_tx, install_error_rx) = sync_channel(1);
        let install_error_listener = app.listen_any("onInstallError", move |event| {
            let _ = install_error_tx.send(event.payload().to_string());
        });
        assert_ipc_response(
            &window,
            ipc_request(
                "start_installation",
                serde_json::json!({
                    "gamePath": game.path().to_string_lossy(),
                    "vhMode": "standard",
                    "installMethod": "unknown",
                }),
            ),
            Ok(()),
        );
        assert!(install_error_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .contains("tidak dikenal"));
        app.unlisten(install_error_listener);

        let (event_tx, event_rx) = sync_channel(1);
        let listener = app.listen_any("onPatchStatus", move |event| {
            let _ = event_tx.send(event.payload().to_string());
        });
        assert_ipc_response(
            &window,
            ipc_request(
                "check_patch_status",
                serde_json::json!({"gamePath": "", "installMethod": "unknown"}),
            ),
            Ok(()),
        );
        let event: serde_json::Value =
            serde_json::from_str(&event_rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(event["status"], "invalid");
        app.unlisten(listener);

        let foreign = pak_dir.join(engine::path::PAK_FILE_NAME);
        std::fs::write(&foreign, b"foreign").unwrap();
        assert_ipc_response(
            &window,
            ipc_request(
                "switch_method",
                serde_json::json!({
                    "gamePath": game.path().to_string_lossy(),
                    "newMethod": "loader",
                }),
            ),
            Ok(serde_json::json!({
                "removed": [foreign.to_string_lossy()],
                "preserved": [],
                "failures": [],
            })),
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&versions).unwrap())
                .unwrap()["_installMethod"],
            "loader"
        );

        assert_ipc_response(
            &window,
            ipc_request(
                "switch_method",
                serde_json::json!({
                    "gamePath": game.path().to_string_lossy(),
                    "newMethod": "loader",
                }),
            ),
            Ok(serde_json::json!({
                "removed": [],
                "preserved": [],
                "failures": [],
            })),
        );
        assert_ipc_response(
            &window,
            ipc_request(
                "uninstall",
                serde_json::json!({"gamePath": game.path().to_string_lossy()}),
            ),
            Ok("ok".to_string()),
        );
        assert_ipc_response(
            &window,
            ipc_request(
                "uninstall",
                serde_json::json!({"gamePath": game.path().to_string_lossy()}),
            ),
            Ok("ok".to_string()),
        );
        assert!(!versions.exists());
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }
}
