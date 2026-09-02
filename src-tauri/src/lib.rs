pub mod engine;

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
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
#[cfg(any(windows, test))]
const LAUNCHER_UPDATE_RESTART_DELAY_SECONDS: u64 = 12;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(500);
const PROCESS_HANDOFF_POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_HANDOFF_GRACE: Duration = Duration::from_secs(3);
const MAX_UNRANGED_MEDIA_RESPONSE_BYTES: u64 = 1024 * 1024;
const TRAY_ICON_ID: &str = "launcher-tray";
#[cfg(windows)]
const LAUNCHER_UPDATE_READY_ENV: &str = "WUWAID_LAUNCHER_UPDATE_READY";

#[cfg(windows)]
fn windows_directory() -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = [0u16; 260];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length > 0 && length < buffer.len() {
        return PathBuf::from(OsString::from_wide(&buffer[..length]));
    }
    PathBuf::from(r"C:\Windows")
}

#[cfg(windows)]
fn windows_system_executable(name: &str) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buffer = [0u16; 260];
    let length = unsafe { GetSystemDirectoryW(Some(&mut buffer)) } as usize;
    let directory = if length > 0 && length < buffer.len() {
        PathBuf::from(OsString::from_wide(&buffer[..length]))
    } else {
        windows_directory().join("System32")
    };
    directory.join(name)
}

#[cfg(windows)]
fn windows_root_executable(name: &str) -> PathBuf {
    windows_directory().join(name)
}

#[cfg(any(windows, test))]
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
    #[cfg(any(test, debug_assertions))]
    if let Ok(url) = std::env::var("WUWAID_ASSETS_URL") {
        return url;
    }

    engine::media::ASSETS_URL.to_string()
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
    #[cfg(any(test, debug_assertions))]
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
        return dir;
    }

    #[cfg(any(test, debug_assertions))]
    {
        PathBuf::from("WuwaIDLauncher")
    }

    #[cfg(all(not(windows), not(any(test, debug_assertions))))]
    {
        let base = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(std::path::MAIN_SEPARATOR.to_string()));
        base.join("WuwaIDLauncher")
    }

    #[cfg(all(windows, not(any(test, debug_assertions))))]
    {
        panic!("LOCALAPPDATA harus tersedia pada build Windows produksi");
    }
}

fn dirs_sys_local_appdata() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".config"))
            .filter(|path| path.is_absolute())
    }
}

fn get_settings_path() -> PathBuf {
    get_appdata_dir().join("settings.json")
}

fn normalized_regular_game_path(game_path: &str) -> Option<PathBuf> {
    let normalized = engine::path::normalize_game_path(game_path)?;
    let canonical = std::fs::canonicalize(normalized).ok()?;
    canonical
        .join(engine::path::GAME_EXE_RELATIVE)
        .is_file()
        .then_some(canonical)
}

fn configured_game_path() -> Option<PathBuf> {
    let content = std::fs::read_to_string(get_settings_path()).ok()?;
    let settings = engine::settings::normalize_settings_json(&content).settings;
    normalized_regular_game_path(&settings.game_path)
}

fn configured_game_executable() -> Option<PathBuf> {
    configured_game_path().map(|path| path.join(engine::path::GAME_EXE_RELATIVE))
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

fn should_hide_to_tray(tray_mode: bool, launcher_pid: Option<u32>) -> bool {
    tray_mode || launcher_pid.is_some()
}

#[derive(Default)]
struct RuntimeCoordinator {
    launcher_pid: Mutex<Option<u32>>,
    launcher_game_pid: Mutex<Option<u32>>,
    launcher_identity: Mutex<Option<engine::runtime::ProcessIdentity>>,
    launcher_game_identity: Mutex<Option<engine::runtime::ProcessIdentity>>,
    #[cfg(windows)]
    termination_handle: Mutex<Option<usize>>,
    force_quit_requested: Mutex<bool>,
    tray_mode: Mutex<bool>,
}

fn coordinator_launcher_pid<R: Runtime>(app: &AppHandle<R>) -> Option<u32> {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.launcher_pid.lock().ok().and_then(|value| *value))
}

fn coordinator_launcher_identity<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<engine::runtime::ProcessIdentity> {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.launcher_identity.lock().ok().and_then(|value| *value))
}

fn coordinator_launcher_game_pid<R: Runtime>(app: &AppHandle<R>) -> Option<u32> {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.launcher_game_pid.lock().ok().and_then(|value| *value))
}

fn coordinator_launcher_game_identity<R: Runtime>(
    app: &AppHandle<R>,
) -> Option<engine::runtime::ProcessIdentity> {
    app.try_state::<RuntimeCoordinator>().and_then(|state| {
        state
            .launcher_game_identity
            .lock()
            .ok()
            .and_then(|value| *value)
    })
}

#[cfg(windows)]
fn set_launcher_termination_handle<R: Runtime>(app: &AppHandle<R>, handle: Option<usize>) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.termination_handle.lock() {
            if let Some(previous) = std::mem::replace(&mut *value, handle) {
                engine::runtime::close_termination_handle(previous);
            }
        }
    }
}

#[cfg(windows)]
fn take_launcher_termination_handle<R: Runtime>(app: &AppHandle<R>) -> Option<usize> {
    app.try_state::<RuntimeCoordinator>()
        .and_then(|state| state.termination_handle.lock().ok()?.take())
}

fn set_launcher_process<R: Runtime>(app: &AppHandle<R>, pid: Option<u32>) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.launcher_pid.lock() {
            *value = pid;
        }
        if let Ok(mut value) = state.launcher_game_pid.lock() {
            *value = pid;
        }
        let identity = pid.and_then(engine::runtime::process_identity);
        if let Ok(mut value) = state.launcher_identity.lock() {
            *value = identity;
        }
        if let Ok(mut value) = state.launcher_game_identity.lock() {
            *value = identity;
        }
        #[cfg(windows)]
        if pid.is_none() {
            if let Ok(mut value) = state.termination_handle.lock() {
                if let Some(handle) = value.take() {
                    engine::runtime::close_termination_handle(handle);
                }
            }
        }
        if pid.is_some() {
            if let Ok(mut value) = state.force_quit_requested.lock() {
                *value = false;
            }
        }
    }
}

fn set_launcher_game_process<R: Runtime>(app: &AppHandle<R>, pid: u32) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.launcher_game_pid.lock() {
            *value = Some(pid);
        }
        if let Ok(mut value) = state.launcher_game_identity.lock() {
            *value = engine::runtime::process_identity(pid);
        }
    }
}

fn set_tray_mode<R: Runtime>(app: &AppHandle<R>, tray_mode: bool) {
    if let Some(state) = app.try_state::<RuntimeCoordinator>() {
        if let Ok(mut value) = state.tray_mode.lock() {
            *value = tray_mode;
        }
    }
    if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
        if let Err(error) = tray.set_visible(tray_mode) {
            log::debug!("Visibilitas ikon tray tidak dapat diubah: {error}");
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

fn request_close<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    engine::operations::global().request_close()?;
    set_tray_mode(app, false);
    app.exit(0);
    Ok(())
}

#[cfg(windows)]
fn request_close_after_launcher_update<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    engine::operations::global().request_close_for_launcher_update()?;
    set_tray_mode(app, false);
    app.exit(0);
    Ok(())
}

fn request_tray_close<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if coordinator_launcher_pid(app).is_some() {
        engine::operations::global().request_close_for_tray()?;
        set_tray_mode(app, false);
        app.exit(0);
        return Ok(());
    }
    request_close(app)
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

fn launcher_force_quit_pid(tracked_pid: Option<u32>) -> Result<u32, String> {
    tracked_pid.ok_or_else(|| {
        "force_quit_target_not_launcher_launched: tidak ada game launcher-launched aktif"
            .to_string()
    })
}

fn emit_game_exit_notice<R: Runtime>(
    app: &AppHandle<R>,
    evidence: &engine::runtime::LaunchEvidence,
    origin: engine::runtime::ProcessOrigin,
    status: &'static str,
    reason: impl Into<String>,
) {
    if origin != engine::runtime::ProcessOrigin::Launcher {
        return;
    }
    let id = format!(
        "{}:{}",
        evidence.started_at_ms,
        evidence.pid.unwrap_or_default()
    );
    let _ = app.emit(
        "onGameExit",
        serde_json::json!({
            "id": id,
            "status": status,
            "reason": reason.into(),
        }),
    );
}

fn complete_launcher_exit<R: Runtime>(
    app: &AppHandle<R>,
    evidence: &engine::runtime::LaunchEvidence,
    status: &'static str,
    reason: impl Into<String>,
) {
    let _ = save_launch_evidence(evidence.clone());
    emit_game_exit_notice(
        app,
        evidence,
        engine::runtime::ProcessOrigin::Launcher,
        status,
        reason,
    );
    finish_launch_lifecycle(app);
}

fn exit_reason(prefix: &str, exit_code: Option<i32>) -> String {
    match exit_code {
        Some(code) => format!("{prefix} (exit code {code})"),
        None => prefix.to_string(),
    }
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

fn emit_launch_failure<R: Runtime>(
    app: &AppHandle<R>,
    game_path: &str,
    dx11: bool,
    csharp_environment: bool,
    error: impl Into<String>,
) {
    let mut evidence = engine::runtime::LaunchEvidence::for_failure(
        engine::runtime::build_launch_command_with_options(
            Path::new(game_path),
            dx11,
            csharp_environment,
        ),
        engine::runtime::SpawnFailureKind::SpawnFailed,
        None,
    );
    evidence.error = Some(error.into());
    let _ = app.emit("onLaunchError", launch_error_message(evidence));
}

fn finish_launch_lifecycle<R: Runtime>(app: &AppHandle<R>) {
    set_launcher_process(app, None);
    emit_runtime_state(
        app,
        engine::runtime::RuntimeState {
            active: false,
            origin: engine::runtime::ProcessOrigin::Launcher,
        },
    );
    restore_launcher_from_tray(app);
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

fn emit_tray_state<R: Runtime>(app: &AppHandle<R>, in_tray: bool) {
    let _ = app.emit(
        "onLauncherTrayState",
        serde_json::json!({"inTray": in_tray}),
    );
}

fn restore_launcher_from_tray<R: Runtime>(app: &AppHandle<R>) {
    // Keep the WebView event channel live while hidden so lifecycle events reach the UI.
    set_tray_mode(app, false);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    emit_tray_state(app, false);
}

fn spawn_runtime_monitor<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut previous = None;
        let mut external_snapshot_cache = engine::runtime::ProcessSnapshotCache::default();
        loop {
            interval.tick().await;
            if app.get_webview_window("main").is_none() {
                break;
            }
            let expected_executable = configured_game_executable();
            let tracked_pid = coordinator_launcher_pid(&app);
            let inspection = if tracked_pid.is_some() {
                // The launch monitor already owns this lifecycle and handles
                // root wait, handoff, and game exit. Avoid re-scanning the
                // process tree while the launcher-owned PID is active.
                engine::runtime::RuntimeProcessInspection::default()
            } else {
                // A full scan is needed only while idle to detect an external
                // game. Reuse verified paths across those reconciliation
                // snapshots, keyed by PID creation identity.
                engine::runtime::inspect_runtime_processes_with_cache(
                    None,
                    None,
                    None,
                    None,
                    expected_executable.as_deref(),
                    false,
                    &mut external_snapshot_cache,
                )
            };
            let detected_pid = inspection.detected_pid;
            let owned_pid = inspection.owned_pid;
            let state = if owned_pid.is_some() {
                engine::runtime::reconcile_runtime_state_with_owned(
                    tracked_pid,
                    detected_pid,
                    owned_pid,
                )
            } else if tracked_pid.is_some() {
                // Keep ownership through a short process-discovery gap. The
                // launch monitor is responsible for clearing this state only
                // after the owned tree has actually finished.
                engine::runtime::RuntimeState {
                    active: true,
                    origin: engine::runtime::ProcessOrigin::Launcher,
                }
            } else {
                engine::runtime::reconcile_runtime_state(None, detected_pid)
            };
            if previous != Some(state) {
                emit_runtime_state(&app, state);
                previous = Some(state);
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
    match window_minimize_action(should_hide_to_tray(
        is_tray_mode(app),
        coordinator_launcher_pid(app),
    )) {
        WindowMinimizeAction::Minimize => {
            let _ = window.minimize();
        }
        WindowMinimizeAction::Hide => {
            set_tray_mode(app, true);
            let _ = window.hide();
            emit_tray_state(app, true);
            notify_tray_minimized(app);
        }
    }
}

#[tauri::command]
fn close_window<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    request_close(window.app_handle())
}

#[tauri::command]
fn is_game_running() -> bool {
    let expected_executable = configured_game_executable();
    engine::runtime::is_game_running_for_path(expected_executable.as_deref())
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
        if let Some(valid) = normalized_regular_game_path(&folder_path) {
            return Ok(valid.to_string_lossy().to_string());
        }
        return Ok("?INVALID".to_string());
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

    if total_len > MAX_UNRANGED_MEDIA_RESPONSE_BYTES {
        // ponytail: the Tauri protocol body is Vec<u8>; cap an unsolicited response
        // at 1 MiB and let the media element request subsequent ranges.
        let end = MAX_UNRANGED_MEDIA_RESPONSE_BYTES - 1;
        let Ok(data) = read_media_range(&file_path, 0, end) else {
            return Response::builder().status(404).body(vec![]).unwrap();
        };
        return Response::builder()
            .status(206)
            .header("Content-Type", mime)
            .header("Content-Range", format!("bytes 0-{end}/{total_len}"))
            .header("Content-Length", data.len().to_string())
            .header("Accept-Ranges", "bytes")
            .header("Access-Control-Allow-Origin", "*")
            .body(data)
            .unwrap();
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
    configured_game_path()
        .and_then(|game_path| {
            engine::metadata::read_game_field(&path, &game_path, "_vhVersion")
                .ok()
                .flatten()
        })
        .unwrap_or_default()
}

fn get_installed_patch_version(game_path: &Path) -> Result<Option<String>, String> {
    let path = get_appdata_dir().join("versions.json");
    engine::metadata::read_game_field(&path, game_path, "_vhVersion")
}

fn known_patch_version(path: &Path, game_path: &Path) -> Option<String> {
    engine::metadata::read_game_field(path, game_path, "_vhVersion")
        .ok()
        .flatten()
        .filter(|version| !version.eq_ignore_ascii_case("unknown"))
}

fn validate_loader_metadata(game_path: &Path) -> Result<(), String> {
    let metadata_path = get_appdata_dir().join("versions.json");
    let Some(expected_hash) =
        engine::metadata::read_game_field(&metadata_path, game_path, "_loaderSha256")?
    else {
        return Err(
            "patch_not_ready: hash loader tidak tersedia pada metadata instalasi".to_string(),
        );
    };
    if engine::installer::validate_installed_loader_hash(game_path, &expected_hash)? {
        Ok(())
    } else {
        Err("patch_not_ready: hash loader tidak sesuai metadata instalasi".to_string())
    }
}

const MAX_LAUNCHER_RELEASE_NOTE_FILE_BYTES: u64 = 3 * 1024 * 1024;

fn get_launcher_release_notes_path() -> PathBuf {
    get_appdata_dir().join("launcher-release-notes.json")
}

fn get_pending_launcher_release_notes_path() -> PathBuf {
    get_appdata_dir().join("launcher-whats-new-pending.json")
}

fn get_launcher_release_note_transaction_path() -> PathBuf {
    get_appdata_dir().join("launcher-whats-new-transaction.json")
}

fn get_launcher_release_note_ready_path() -> PathBuf {
    get_appdata_dir().join("launcher-whats-new-ready.tag")
}

fn normalized_launcher_version(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value)
}

fn launcher_release_note_matches_version(
    note: &engine::atom_feed::ReleaseNoteEntry,
    version: &str,
) -> bool {
    let note_version = normalized_launcher_version(&note.tag);
    let current_version = normalized_launcher_version(version);
    !note_version.is_empty() && note_version.eq_ignore_ascii_case(current_version)
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

fn validated_launcher_release_note(
    payload: serde_json::Value,
) -> Option<engine::atom_feed::ReleaseNoteEntry> {
    serde_json::from_value::<engine::atom_feed::ReleaseNoteEntry>(payload)
        .ok()
        .and_then(|entry| engine::atom_feed::validate_release_note(&entry).ok())
        .filter(|entry| !entry.body.trim().is_empty())
}

fn fallback_launcher_release_note(tag: &str, version: &str) -> engine::atom_feed::ReleaseNoteEntry {
    engine::atom_feed::ReleaseNoteEntry {
        tag: tag.to_string(),
        date: String::new(),
        title: format!("WuwaID Launcher {version}"),
        body: "Catatan rilis belum tersedia. Launcher berhasil diperbarui.".to_string(),
        author: "WuwaID Team".to_string(),
    }
}

fn launcher_release_note_for_release(
    release: &engine::updater::ReleaseInfo,
) -> engine::atom_feed::ReleaseNoteEntry {
    validated_launcher_release_note(launcher_release_note_payload(release)).unwrap_or_else(|| {
        let mut fallback = fallback_launcher_release_note(&release.tag_name, &release.version);
        if !release.date.trim().is_empty() {
            fallback.date = release.date.clone();
        }
        if !release.title.trim().is_empty() {
            fallback.title = release.title.clone();
        }
        if !release.author.trim().is_empty() {
            fallback.author = release.author.clone();
        }
        fallback
    })
}

fn launcher_update_payload(
    release: &engine::updater::ReleaseInfo,
    note: &engine::atom_feed::ReleaseNoteEntry,
) -> serde_json::Value {
    serde_json::json!({
        "version": release.version,
        "tag": note.tag,
        "date": note.date,
        "body": note.body,
        "title": note.title,
        "author": note.author
    })
}

const MAX_LAUNCHER_RELEASE_NOTE_MARKER_BYTES: u64 = 256;

fn read_launcher_release_note(path: &Path) -> Option<engine::atom_feed::ReleaseNoteEntry> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_LAUNCHER_RELEASE_NOTE_FILE_BYTES {
        return None;
    }
    let content = std::fs::read(path).ok()?;
    let payload = serde_json::from_slice::<serde_json::Value>(&content).ok()?;
    validated_launcher_release_note(payload)
}

fn read_launcher_release_note_ready_marker(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_LAUNCHER_RELEASE_NOTE_MARKER_BYTES {
        return None;
    }
    let marker = std::fs::read_to_string(path).ok()?;
    let marker = marker.trim();
    (!marker.is_empty()).then(|| marker.to_string())
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Gagal menghapus file release notes: {error}")),
    }
}

fn launcher_release_note_marker_temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("launcher-whats-new-ready.tag");
    path.with_file_name(format!("{name}.tmp"))
}

fn clear_launcher_release_note_state(
    transaction_path: &Path,
    pending_path: &Path,
    ready_marker_path: &Path,
) -> Result<(), String> {
    // Invalidate the visibility marker first. If a later file deletion is
    // interrupted, pending data is still not eligible for display.
    let ready_marker_temp_path = launcher_release_note_marker_temp_path(ready_marker_path);
    let paths = [
        ready_marker_path,
        &ready_marker_temp_path,
        transaction_path,
        pending_path,
    ];
    let mut first_error = None;
    for path in paths {
        if let Err(error) = remove_file_if_exists(path) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn read_committed_launcher_release_note(
    transaction_path: &Path,
    pending_path: &Path,
    ready_marker_path: &Path,
    current_version: &str,
) -> Option<engine::atom_feed::ReleaseNoteEntry> {
    if transaction_path.exists()
        || launcher_release_note_marker_temp_path(ready_marker_path).exists()
    {
        return None;
    }
    let note = read_launcher_release_note(pending_path)?;
    let marker = read_launcher_release_note_ready_marker(ready_marker_path)?;
    if launcher_release_note_matches_version(&note, current_version)
        && launcher_release_note_matches_version(&note, &marker)
    {
        Some(note)
    } else {
        None
    }
}

#[doc(hidden)]
pub mod launcher_update_state {
    use super::{
        clear_launcher_release_note_state, launcher_release_note_marker_temp_path,
        launcher_release_note_matches_version, read_committed_launcher_release_note,
        read_launcher_release_note, read_launcher_release_note_ready_marker,
    };
    use crate::engine::atom_feed::ReleaseNoteEntry;
    use std::path::Path;

    pub fn read_committed_release_note(
        transaction_path: &Path,
        pending_path: &Path,
        ready_marker_path: &Path,
        current_version: &str,
    ) -> Option<ReleaseNoteEntry> {
        read_committed_launcher_release_note(
            transaction_path,
            pending_path,
            ready_marker_path,
            current_version,
        )
    }

    pub fn invalidate(
        transaction_path: &Path,
        pending_path: &Path,
        ready_marker_path: &Path,
    ) -> Result<(), String> {
        clear_launcher_release_note_state(transaction_path, pending_path, ready_marker_path)
    }

    pub fn acknowledge(
        transaction_path: &Path,
        pending_path: &Path,
        ready_marker_path: &Path,
        tag: &str,
    ) -> Result<(), String> {
        if transaction_path.exists()
            || launcher_release_note_marker_temp_path(ready_marker_path).exists()
        {
            return Ok(());
        }
        let Some(note) = read_launcher_release_note(pending_path) else {
            return Ok(());
        };
        let Some(marker) = read_launcher_release_note_ready_marker(ready_marker_path) else {
            return Ok(());
        };
        if launcher_release_note_matches_version(&note, tag)
            && launcher_release_note_matches_version(&note, &marker)
        {
            clear_launcher_release_note_state(transaction_path, pending_path, ready_marker_path)?;
        }
        Ok(())
    }
}

fn write_json_atomically<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_vec(value)
        .map_err(|error| format!("Gagal serialisasi metadata release notes: {error}"))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Gagal membuat folder release notes: {error}"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("launcher-release-notes.json");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary = parent.join(format!(".{name}.tmp-{}-{stamp}", std::process::id()));

    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Gagal membuat file sementara release notes: {error}"))?;
        file.write_all(&content)
            .map_err(|error| format!("Gagal menulis release notes: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Gagal menyimpan release notes: {error}"))?;
        drop(file);
        engine::downloader::replace_file_atomically(&temporary, path)
    })();
    let _ = std::fs::remove_file(&temporary);
    result
}

fn write_launcher_release_note(
    path: &Path,
    note: &engine::atom_feed::ReleaseNoteEntry,
) -> Result<(), String> {
    write_json_atomically(path, note)
}

const LAUNCHER_RELEASE_NOTE_COMMIT_WAIT: Duration = Duration::from_secs(10);

#[tauri::command]
fn get_launcher_release_notes<R: Runtime>(app: AppHandle<R>) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let transaction_path = get_launcher_release_note_transaction_path();
        let pending_path = get_pending_launcher_release_notes_path();
        let ready_marker_path = get_launcher_release_note_ready_path();
        let current_version = app_version_value();
        let deadline = Instant::now() + LAUNCHER_RELEASE_NOTE_COMMIT_WAIT;

        loop {
            if let Some(note) = read_committed_launcher_release_note(
                &transaction_path,
                &pending_path,
                &ready_marker_path,
                &current_version,
            ) {
                let _ = app_handle.emit("onLauncherReleaseNotes", note);
                return;
            }

            if let (Some(note), Some(marker)) = (
                read_launcher_release_note(&pending_path),
                read_launcher_release_note_ready_marker(&ready_marker_path),
            ) {
                // A committed note for a future version belongs to the next
                // launcher process; keep it until that version starts.
                if launcher_release_note_matches_version(&note, &marker)
                    && engine::updater::is_newer_version(&current_version, &note.tag)
                {
                    return;
                }
            }

            if !transaction_path.exists() && !pending_path.exists() && !ready_marker_path.exists() {
                let _ = clear_launcher_release_note_state(
                    &transaction_path,
                    &pending_path,
                    &ready_marker_path,
                );
                return;
            }
            if Instant::now() >= deadline {
                let _ = clear_launcher_release_note_state(
                    &transaction_path,
                    &pending_path,
                    &ready_marker_path,
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
}

#[tauri::command]
fn acknowledge_launcher_release_notes(tag: String) -> Result<(), String> {
    launcher_update_state::acknowledge(
        &get_launcher_release_note_transaction_path(),
        &get_pending_launcher_release_notes_path(),
        &get_launcher_release_note_ready_path(),
        &tag,
    )
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
                let release_note = launcher_release_note_for_release(&release);
                if let (Some(zip), Some(checksums)) =
                    (release.zip_url.clone(), release.checksums_url.clone())
                {
                    if let Err(error) = engine::updater::validate_update_request(
                        &release.version,
                        &release.tag_name,
                        &zip,
                        Some(&checksums),
                    ) {
                        let _ = app_handle.emit(
                            "onLauncherUpdateError",
                            format!("Asset update launcher tidak valid: {error}"),
                        );
                        return;
                    }
                    // The update dialog and post-restart modal must share this
                    // exact validated payload. Do not offer the update if its
                    // durable source cannot be published first.
                    if let Err(error) = write_launcher_release_note(
                        &get_launcher_release_notes_path(),
                        &release_note,
                    ) {
                        let _ = app_handle.emit(
                            "onLauncherUpdateError",
                            format!("Gagal menyimpan cache launcher release notes: {error}"),
                        );
                        return;
                    }
                    let _ = app_handle.emit(
                        "onLauncherUpdateAvailable",
                        launcher_update_payload(&release, &release_note),
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
        std::process::Command::new(windows_root_executable("explorer.exe"))
            .arg(SUPPORT_URL)
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
fn check_and_sync_media<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let operation =
        engine::operations::global().try_acquire(engine::operations::OperationKind::MediaSync)?;
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
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
    Ok(())
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

                if let Err(error) =
                    engine::metadata::update_cached_release_notes(&versions_path, note_json.clone())
                {
                    log::warn!("Gagal menyimpan cache release notes Patch ID: {error}");
                }

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

fn launcher_update_tag(version: &str) -> String {
    let version = version.trim();
    if version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

#[tauri::command]
fn perform_launcher_update<R: Runtime>(app: AppHandle<R>, version: String) -> Result<(), String> {
    if std::env::consts::OS != "windows" {
        let _ = app.emit(
            "onLauncherUpdateError",
            "Self-update launcher hanya tersedia pada Windows.".to_string(),
        );
        return Ok(());
    }

    let version = version.trim().to_string();
    let tag = launcher_update_tag(&version);
    let expected_zip = match engine::updater::expected_zip_asset_name(&tag) {
        Ok(name) => name,
        Err(error) => {
            let _ = app.emit(
                "onLauncherUpdateError",
                format!("Permintaan update launcher tidak valid: {error}"),
            );
            return Ok(());
        }
    };
    let zip_url = match engine::updater::expected_official_asset_url(&tag, &expected_zip) {
        Ok(url) => url,
        Err(error) => {
            let _ = app.emit(
                "onLauncherUpdateError",
                format!("Permintaan update launcher tidak valid: {error}"),
            );
            return Ok(());
        }
    };
    let checksums_url = match engine::updater::expected_official_asset_url(&tag, "SHA256sums.txt") {
        Ok(url) => url,
        Err(error) => {
            let _ = app.emit(
                "onLauncherUpdateError",
                format!("Permintaan update launcher tidak valid: {error}"),
            );
            return Ok(());
        }
    };
    log::info!(
        "Perform launcher update requested: {} -> {}",
        version,
        zip_url
    );
    if let Err(error) =
        engine::updater::validate_update_request(&version, &tag, &zip_url, Some(&checksums_url))
    {
        let _ = app.emit(
            "onLauncherUpdateError",
            format!("Permintaan update launcher tidak valid: {error}"),
        );
        return Ok(());
    }

    let pending_note_path = get_pending_launcher_release_notes_path();
    let transaction_path = get_launcher_release_note_transaction_path();
    let ready_marker_path = get_launcher_release_note_ready_path();
    #[cfg(windows)]
    let pending_note = match read_launcher_release_note(&get_launcher_release_notes_path())
        .filter(|note| launcher_release_note_matches_version(note, &tag))
    {
        Some(note) => note,
        None => {
            let _ = app.emit(
                "onLauncherUpdateError",
                "Catatan release update tidak tersedia. Silakan periksa update lagi.".to_string(),
            );
            return Ok(());
        }
    };

    let operation = engine::operations::global()
        .try_acquire(engine::operations::OperationKind::LauncherUpdate)?;
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        let temp_zip = get_appdata_dir().join("update.zip");
        let staging = get_appdata_dir().join(".staging");
        let handoff_path = get_appdata_dir().join("update-handoff.cmd");
        cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
        let app_progress = app_handle.clone();

        let result: Result<(), String> = async {
            launcher_update_state::invalidate(
                &transaction_path,
                &pending_note_path,
                &ready_marker_path,
            )
            .map_err(|error| format!("Gagal membersihkan state update sebelumnya: {error}"))?;
            if !engine::updater::is_newer_version(env!("CARGO_PKG_VERSION"), &version) {
                return Err("Versi update tidak lebih baru dari launcher saat ini.".to_string());
            }

            engine::downloader::download_file_with_expected_size_limited_policy(
                &zip_url,
                &temp_zip,
                None,
                engine::updater::MAX_UPDATE_ZIP_COMPRESSED_BYTES,
                engine::downloader::DownloadRedirectPolicy::OfficialGithubAsset {
                    expected_url: zip_url.clone(),
                },
                move |progress| {
                    let _ = app_progress.emit(
                        "onLauncherUpdateProgress",
                        serde_json::json!({
                            "percent": progress.percent,
                            "status": progress.status
                        }),
                    );
                },
            )
            .await
            .map_err(|error| format!("download: {error}"))?;

            let zip_data = std::fs::read(&temp_zip)
                .map_err(|error| format!("Gagal membaca ZIP update: {error}"))?;
            let checksum_body = engine::updater::fetch_official_asset_body(
                &checksums_url,
                &tag,
                "SHA256sums.txt",
                engine::updater::MAX_UPDATE_CHECKSUM_BYTES,
            )
            .await?;
            let checksum_text = String::from_utf8(checksum_body)
                .map_err(|error| format!("Checksum update bukan UTF-8 valid: {error}"))?;
            let zip_name = engine::updater::expected_zip_asset_name(&tag)?;
            let expected = engine::updater::parse_checksum_manifest(&checksum_text)
                .get(&zip_name)
                .cloned()
                .ok_or_else(|| format!("Checksum untuk {zip_name} tidak ditemukan."))?;
            let actual = engine::downloader::compute_sha256(&temp_zip)
                .map_err(|error| format!("Gagal menghitung checksum update: {error}"))?;
            if actual != expected {
                return Err("Checksum ZIP update tidak cocok.".to_string());
            }
            engine::updater::validate_update_archive(
                &zip_data,
                engine::updater::RELEASE_EXECUTABLE_NAME,
            )?;

            if staging.exists() {
                std::fs::remove_dir_all(&staging)
                    .map_err(|error| format!("Gagal membersihkan staging lama: {error}"))?;
            }
            let exe_path = engine::updater::extract_zip_update(&zip_data, &staging)?;
            #[cfg(windows)]
            let current_exe = std::env::current_exe().map_err(|error| {
                format!("Executable launcher saat ini tidak ditemukan: {error}")
            })?;

            #[cfg(windows)]
            {
                // Persist the complete payload before starting the handoff.
                // The handoff promotes this transaction to pending only after
                // the new executable is running and has passed its health check.
                write_launcher_release_note(&transaction_path, &pending_note)
                    .map_err(|error| format!("Gagal menyimpan transaksi What's New: {error}"))?;
                engine::updater::create_update_handoff_with_release_state(
                    &staging,
                    &current_exe,
                    &handoff_path,
                    &transaction_path,
                    &pending_note_path,
                    &ready_marker_path,
                    &tag,
                )?;
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

                use std::os::windows::process::CommandExt;
                let handoff_arg = handoff_path.to_string_lossy().to_string();
                std::process::Command::new(windows_system_executable("cmd.exe"))
                    .args(["/C", handoff_arg.as_str()])
                    .creation_flags(0x08000000)
                    .spawn()
                    .map_err(|error| format!("Gagal menjalankan restart update: {error}"))?;
                let _ = app_handle.emit(
                    "onLauncherUpdateRestarting",
                    serde_json::json!({"remainingSeconds": 0}),
                );
                request_close_after_launcher_update(&app_handle)?;
                drop(_operation);
                Ok::<(), String>(())
            }
            #[cfg(not(windows))]
            {
                let _ = exe_path;
                Err("Self-update handoff hanya tersedia pada Windows.".to_string())
            }
        }
        .await;

        if let Err(error) = result {
            cleanup_update_artifacts(&temp_zip, &staging, &handoff_path);
            let _ = launcher_update_state::invalidate(
                &transaction_path,
                &pending_note_path,
                &ready_marker_path,
            );
            let message = if let Some(error) = error.strip_prefix("download: ") {
                format!("Gagal mengunduh update launcher: {error}")
            } else {
                format!("Gagal menyiapkan update launcher: {error}")
            };
            let _ = app_handle.emit("onLauncherUpdateError", message);
        }
    });
    Ok(())
}

#[tauri::command]
async fn check_patch_status<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    install_method: String,
    uid_mode: String,
    uid_text: String,
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
                    "uidMode": uid_mode,
                    "uidText": uid_text,
                    "message": error
                }),
            );
            return Ok(());
        }
    };

    let desired_variant =
        match engine::patch_asset::PatchVariant::from_uid_selection(&uid_mode, &uid_text) {
            Ok(variant) => variant,
            Err(error) => {
                let _ = app.emit(
                    "onPatchStatus",
                    serde_json::json!({
                        "status": "invalid",
                        "gamePath": game_path,
                        "installMethod": method.as_str(),
                        "uidMode": uid_mode,
                        "uidText": uid_text,
                        "message": error
                    }),
                );
                return Ok(());
            }
        };

    let normalized_path = match normalized_regular_game_path(&game_path) {
        Some(path) => path,
        None => {
            let _ = app.emit(
                "onPatchStatus",
                serde_json::json!({
                    "status": "invalid",
                    "gamePath": game_path,
                    "installMethod": method.as_str(),
                    "uidMode": uid_mode,
                    "uidText": uid_text,
                    "message": "Folder game tidak valid atau executable game tidak ditemukan."
                }),
            );
            return Ok(());
        }
    };

    let mut local = engine::patch_status::classify_installation(&normalized_path, method)
        .map_err(|error| format!("Gagal memeriksa instalasi patch: {error}"))?;
    if method == engine::method::InstallMethod::Loader
        && matches!(local, engine::patch_status::LocalPatchState::Ready)
        && validate_loader_metadata(&normalized_path).is_err()
    {
        local = engine::patch_status::LocalPatchState::Invalid;
    }
    if matches!(local, engine::patch_status::LocalPatchState::Ready) {
        let metadata_path = get_appdata_dir().join("versions.json");
        let installed_variant =
            engine::metadata::read_game_field(&metadata_path, &normalized_path, "_patchVariant")?;
        if !engine::patch_asset::installed_variant_matches(
            installed_variant.as_deref(),
            desired_variant,
        ) {
            local = engine::patch_status::LocalPatchState::Invalid;
        }
    }
    let current_version = get_installed_patch_version(&normalized_path)
        .map_err(|error| format!("Gagal membaca metadata instalasi patch: {error}"))?;
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
            "uidMode": uid_mode,
            "uidText": uid_text,
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
    let operation =
        engine::operations::global().try_acquire(engine::operations::OperationKind::CacheReset)?;
    log::info!("Reset webview cache requested");
    let cache_dir = get_appdata_dir().join("Cache");
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|error| format!("Gagal menghapus cache WebView: {error}"))?;
    }
    drop(operation);
    let _ = app.emit(
        "onMediaStatus",
        serde_json::json!({"status": "checking", "message": "Cache direset; memulai sinkronisasi media..."}),
    );
    check_and_sync_media(app)
}

async fn ensure_cached_patch_asset<R: Runtime>(
    app: &AppHandle<R>,
    asset_name: &str,
    destination: &Path,
    expected_hash: &str,
) -> Result<(), String> {
    if destination.exists()
        && engine::downloader::verify_sha256(destination, expected_hash).unwrap_or(false)
    {
        return Ok(());
    }
    let url = format!("{}{}", WUWAID_LATEST_DOWNLOAD_BASE_URL, asset_name);
    let content_len = engine::downloader::get_asset_content_length(&url)
        .await
        .map_err(|error| format!("Gagal memverifikasi metadata asset {asset_name}: {error}"))?;
    let app_progress = app.clone();
    engine::downloader::download_file_with_expected_size(
        &url,
        destination,
        Some(content_len),
        move |progress| {
            let _ = app_progress.emit(
                "onProgressUpdate",
                serde_json::json!({
                    "percent": (progress.percent as f32 * 0.85) as u8,
                    "status": format!("Mengunduh patch... {}", progress.status),
                    "downloadedBytes": progress.downloaded_bytes,
                    "totalBytes": progress.total_bytes,
                    "speedMbps": progress.speed_mbps
                }),
            );
        },
    )
    .await
    .map_err(|error| format!("Gagal mengunduh {asset_name}: {error}"))?;
    if !engine::downloader::verify_sha256(destination, expected_hash).unwrap_or(false) {
        let _ = std::fs::remove_file(destination);
        return Err(format!(
            "Integritas file {asset_name} gagal diverifikasi (SHA-256 mismatch)."
        ));
    }
    Ok(())
}

fn prepare_cached_patch_asset(
    pak_path: &Path,
    expected_hash: &str,
    cache_dir: &Path,
    variant: engine::patch_asset::PatchVariant,
) -> Result<PathBuf, String> {
    if expected_hash.len() != 64
        || !expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !engine::downloader::verify_sha256(pak_path, expected_hash).unwrap_or(false)
    {
        return Err("Integritas PAK normal gagal diverifikasi.".to_string());
    }
    if !engine::installer::validate_pak_file(pak_path)? {
        return Err("Struktur PAK rilis tidak valid.".to_string());
    }
    match variant {
        engine::patch_asset::PatchVariant::Normal => Ok(pak_path.to_path_buf()),
        engine::patch_asset::PatchVariant::HideUid => {
            engine::patch_asset::prepare_hide_uid_pak(pak_path, expected_hash, cache_dir)
        }
        engine::patch_asset::PatchVariant::Custom(uid_text) => {
            engine::patch_asset::prepare_custom_uid_pak(
                pak_path,
                expected_hash,
                cache_dir,
                &uid_text,
            )
        }
    }
}

async fn prepare_patch_asset<R: Runtime>(
    app: &AppHandle<R>,
    checksums: &std::collections::HashMap<String, String>,
    variant: engine::patch_asset::PatchVariant,
) -> Result<PathBuf, String> {
    let cache_dir = get_appdata_dir().join("Cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("Gagal membuat cache patch: {error}"))?;

    let asset_name = engine::patch_asset::NORMAL_PAK_FILE_NAME;
    let expected_hash = checksums
        .get(asset_name)
        .cloned()
        .ok_or_else(|| format!("Checksum {asset_name} tidak ditemukan."))?;
    let pak_path = cache_dir.join(asset_name);
    ensure_cached_patch_asset(app, asset_name, &pak_path, &expected_hash).await?;

    if !matches!(&variant, engine::patch_asset::PatchVariant::Normal) {
        let _ = app.emit(
            "onProgressUpdate",
            serde_json::json!({
                "percent": 88,
                "status": "Membuat PAK UID custom..."
            }),
        );
    }

    let cache_dir_for_worker = cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare_cached_patch_asset(&pak_path, &expected_hash, &cache_dir_for_worker, variant)
    })
    .await
    .map_err(|error| format!("Pembuatan PAK patch dibatalkan: {error}"))?
}

#[tauri::command]
fn start_installation<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    _vh_mode: String,
    install_method: String,
    uid_mode: String,
    uid_text: String,
) -> Result<(), String> {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            let _ = app.emit("onInstallError", error);
            return Ok(());
        }
    };
    let patch_variant =
        match engine::patch_asset::PatchVariant::from_uid_selection(&uid_mode, &uid_text) {
            Ok(variant) => variant,
            Err(error) => {
                let _ = app.emit("onInstallError", error);
                return Ok(());
            }
        };
    let operation = engine::operations::global()
        .try_acquire(engine::operations::OperationKind::PatchInstall)?;
    let normalized_game_path =
        match engine::installer::validate_installation_preconditions(&game_path, method) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(error) => {
                let _ = app.emit("onInstallError", error);
                return Ok(());
            }
        };
    let expected_executable =
        Path::new(&normalized_game_path).join(engine::path::GAME_EXE_RELATIVE);
    if let Some(pid) = engine::runtime::find_game_process_id_for_path(Some(&expected_executable)) {
        let error = format!("busy: game sedang berjalan (pid {pid})");
        let _ = app.emit("onInstallError", error.clone());
        drop(operation);
        return Err(error);
    }
    let canonical_method = method.as_str().to_string();
    let patch_variant_id = patch_variant.identity();
    log::info!(
        "Start installation: path={}, method={}, variant={}",
        normalized_game_path,
        canonical_method,
        patch_variant_id
    );

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
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
                match engine::downloader::read_response_body_limited(
                    resp,
                    engine::updater::MAX_UPDATE_CHECKSUM_BYTES,
                )
                .await
                {
                    Ok(body) => {
                        engine::downloader::parse_sha256sums(&String::from_utf8_lossy(&body))
                    }
                    Err(error) => {
                        log::warn!("Checksum manifest patch ditolak: {error}");
                        std::collections::HashMap::new()
                    }
                }
            }
            _ => std::collections::HashMap::new(),
        };
        let versions_path = get_appdata_dir().join("versions.json");
        let patch_version = get_latest_patch_version()
            .await
            .or_else(|| known_patch_version(&versions_path, p))
            .unwrap_or_else(|| "unknown".to_string());

        let cache_pak =
            match prepare_patch_asset(&app_handle, &checksums, patch_variant.clone()).await {
                Ok(path) => path,
                Err(error) => {
                    let _ = app_handle.emit("onInstallError", error);
                    return;
                }
            };

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

        let expected_executable = p.join(engine::path::GAME_EXE_RELATIVE);
        if let Some(pid) =
            engine::runtime::find_game_process_id_for_path(Some(&expected_executable))
        {
            let error = format!("busy: game sedang berjalan (pid {pid})");
            let _ = app_handle.emit("onInstallError", error);
            return;
        }

        let loader_sha256 = match loader_cache.as_ref() {
            Some(loader_path) => match engine::downloader::compute_sha256(loader_path) {
                Ok(hash) => Some(hash),
                Err(error) => {
                    let _ = app_handle.emit(
                        "onInstallError",
                        format!("Gagal menghitung hash loader: {error}"),
                    );
                    return;
                }
            },
            None => None,
        };

        let _ = app_handle.emit(
            "onProgressUpdate",
            serde_json::json!({
                "percent": 90,
                "status": "Memasang file mod..."
            }),
        );

        if let Err(error) = engine::installer::install_patch_transaction_with_commit(
            p,
            method,
            &cache_pak,
            loader_cache.as_deref(),
            || {
                engine::metadata::update_installation_with_variant(
                    &versions_path,
                    p,
                    Some(&patch_version),
                    &canonical_method,
                    loader_sha256.as_deref(),
                    Some(&patch_variant_id),
                )
            },
        ) {
            let _ = app_handle.emit("onInstallError", error);
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
    Ok(())
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

fn wait_for_launcher_process_tree<R: Runtime>(
    app: &AppHandle<R>,
    process: &mut engine::runtime::LaunchedGame,
    root_pid: u32,
    launcher_identity: Option<engine::runtime::ProcessIdentity>,
    expected_executable: &Path,
) -> Result<engine::runtime::ProcessResult, String> {
    let mut root_result = None;
    let mut handoff_started_at = None;
    let mut handed_off_game = None;
    let mut process_snapshot_cache = engine::runtime::ProcessSnapshotCache::default();

    loop {
        if root_result.is_none() {
            if let Some(result) = process.try_wait()? {
                root_result = Some(result);
                handoff_started_at = Some(Instant::now());
            } else {
                // The root process is still alive, so there is no handoff to
                // reconcile yet. Avoid taking a full process snapshot and
                // querying executable paths on every root poll.
                std::thread::sleep(PROCESS_POLL_INTERVAL);
                continue;
            }
        }

        if let Some((game_pid, game_identity)) = handed_off_game {
            if engine::runtime::process_identity(game_pid) == Some(game_identity) {
                // The verified game root is alive. Its descendants are owned
                // for force-quit, but do not require a full snapshot every
                // 100 ms while the game is simply running.
                std::thread::sleep(PROCESS_POLL_INTERVAL);
                continue;
            }
            // The game root exited or changed identity. Take one fresh
            // reconciliation snapshot to detect a verified replacement
            // before completing the launcher lifecycle.
            handed_off_game = None;
        }

        let inspection = engine::runtime::inspect_runtime_processes_with_cache(
            Some(root_pid),
            launcher_identity,
            coordinator_launcher_game_pid(app),
            coordinator_launcher_game_identity(app),
            Some(expected_executable),
            true,
            &mut process_snapshot_cache,
        );
        if let Some(pid) = inspection.owned_pid {
            set_launcher_game_process(app, pid);
            if let Some(identity) = coordinator_launcher_game_identity(app) {
                handed_off_game = Some((pid, identity));
            }
            handoff_started_at = Some(Instant::now());
        }
        let owned_pid = inspection.owned_pid;
        let has_descendant = inspection.has_descendant;

        if root_result.is_some()
            && owned_pid.is_none()
            && has_descendant == Some(false)
            && handoff_started_at
                .is_some_and(|started_at| started_at.elapsed() >= PROCESS_HANDOFF_GRACE)
        {
            return process.finalize();
        }

        // Full reconciliation is needed only during the short post-root
        // handoff window and on a verified game-root transition. Before and
        // after that window, root/identity checks use the cheaper poll above.
        std::thread::sleep(PROCESS_HANDOFF_POLL_INTERVAL);
    }
}

#[tauri::command]
fn launch_game<R: Runtime>(
    app: AppHandle<R>,
    game_path: String,
    dx11: bool,
    csharp_environment: bool,
    install_method: String,
) -> Result<(), String> {
    let method = match engine::method::InstallMethod::parse(&install_method) {
        Ok(method) => method,
        Err(error) => {
            emit_launch_failure(&app, &game_path, dx11, csharp_environment, error);
            return Ok(());
        }
    };
    let normalized_game_path =
        match engine::runtime::validate_launch_preconditions(&game_path, method) {
            Ok(path) => path,
            Err(error) => {
                emit_launch_failure(&app, &game_path, dx11, csharp_environment, error);
                return Ok(());
            }
        };
    if method == engine::method::InstallMethod::Loader {
        if let Err(error) = validate_loader_metadata(&normalized_game_path) {
            emit_launch_failure(&app, &game_path, dx11, csharp_environment, error);
            return Ok(());
        }
    }
    let operation = match engine::operations::global()
        .try_acquire(engine::operations::OperationKind::GameLaunch)
    {
        Ok(operation) => operation,
        Err(error) => {
            emit_launch_failure(&app, &game_path, dx11, csharp_environment, error);
            return Ok(());
        }
    };
    let expected_executable = normalized_game_path.join(engine::path::GAME_EXE_RELATIVE);
    if let Some(pid) = engine::runtime::find_game_process_id_for_path(Some(&expected_executable)) {
        drop(operation);
        emit_launch_failure(
            &app,
            &game_path,
            dx11,
            csharp_environment,
            format!("busy: game sedang berjalan (pid {pid})"),
        );
        return Ok(());
    }
    let canonical_method = method.as_str().to_string();
    log::info!(
        "Launch game: path={}, dx11={}, csharp_environment={}, method={}",
        normalized_game_path.display(),
        dx11,
        csharp_environment,
        canonical_method
    );
    let app_handle = app.clone();
    let p = normalized_game_path;

    tauri::async_runtime::spawn(async move {
        let operation = operation;
        let _ = app_handle.emit("onGameLaunchStarted", ());
        let command =
            engine::runtime::build_launch_command_with_options(&p, dx11, csharp_environment);

        match engine::runtime::launch_game_with_options(&p, dx11, csharp_environment) {
            Ok(mut process) => {
                if let Some(service) =
                    app_handle.try_state::<engine::active_player::ActivePlayerService>()
                {
                    service.send_launch(method);
                }
                let root_pid = process.id();
                let mut evidence =
                    engine::runtime::LaunchEvidence::for_process(command, process.mode, root_pid);
                #[cfg(windows)]
                match process.duplicate_termination_handle() {
                    Ok(handle) => set_launcher_termination_handle(&app_handle, handle),
                    Err(error) => {
                        log::warn!("Handle paksa tutup elevated tidak dapat disimpan: {error}");
                    }
                }
                // Hide immediately after a successful spawn. Process discovery is
                // still handled by the runtime monitor and no longer delays this.
                if let Some(window) = app_handle.get_webview_window("main") {
                    set_tray_mode(&app_handle, true);
                    let _ = window.hide();
                    emit_tray_state(&app_handle, true);
                    notify_tray_minimized(&app_handle);
                }
                engine::runtime::trim_memory_working_set();

                evidence.mark_detected();
                set_launcher_process(&app_handle, Some(root_pid));
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
                let operation_for_monitor = operation;
                tokio::task::spawn_blocking(move || {
                    let _operation = operation_for_monitor;
                    let process_result = wait_for_launcher_process_tree(
                        &app_for_monitor,
                        &mut process,
                        root_pid,
                        coordinator_launcher_identity(&app_for_monitor),
                        &p_for_monitor.join(engine::path::GAME_EXE_RELATIVE),
                    );
                    match process_result {
                        Ok(result) => {
                            evidence.exit_code = result.exit_code;
                            evidence.stdout = result.stdout;
                            evidence.stderr = result.stderr;
                            evidence.game_log_tail =
                                engine::runtime::collect_game_log_tail(&p_for_monitor);
                            evidence.mark_finished();
                            let force_quit = take_force_quit_requested(&app_for_monitor);
                            if force_quit {
                                complete_launcher_exit(
                                    &app_for_monitor,
                                    &evidence,
                                    "force_quit",
                                    "Proses dihentikan oleh launcher.",
                                );
                            } else if evidence.exit_code.unwrap_or(0) != 0 {
                                evidence.failure_kind =
                                    Some(engine::runtime::SpawnFailureKind::ProcessCrashed);
                                evidence.error = Some(
                                    "game process exited with a non-zero exit code".to_string(),
                                );
                                complete_launcher_exit(
                                    &app_for_monitor,
                                    &evidence,
                                    "crashed",
                                    exit_reason(
                                        "Proses game berhenti tidak terduga",
                                        evidence.exit_code,
                                    ),
                                );
                            } else {
                                complete_launcher_exit(
                                    &app_for_monitor,
                                    &evidence,
                                    "normal",
                                    "Proses game selesai secara normal.",
                                );
                            }
                        }
                        Err(error) => {
                            evidence.failure_kind =
                                Some(engine::runtime::SpawnFailureKind::ProcessCrashed);
                            evidence.error = Some(error);
                            evidence.game_log_tail =
                                engine::runtime::collect_game_log_tail(&p_for_monitor);
                            complete_launcher_exit(
                                &app_for_monitor,
                                &evidence,
                                "crashed",
                                "Pemantauan proses game gagal.",
                            );
                        }
                    }
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
    let operation =
        engine::operations::global().try_acquire(engine::operations::OperationKind::ForceQuit)?;
    let tracked_pid = match launcher_force_quit_pid(coordinator_launcher_pid(&app)) {
        Ok(pid) => pid,
        Err(error) => {
            drop(operation);
            return Err(error);
        }
    };
    let expected_executable = match configured_game_executable() {
        Some(path) => path,
        None => {
            drop(operation);
            return Err(
                "force_quit_target_not_verified: folder game belum dikonfigurasi".to_string(),
            );
        }
    };
    mark_force_quit_requested(&app);
    #[cfg(windows)]
    let termination_handle = take_launcher_termination_handle(&app);
    #[cfg(not(windows))]
    let termination_handle = None;
    let quit_result = engine::runtime::force_quit_game_with_ownership(
        Some(tracked_pid),
        coordinator_launcher_identity(&app),
        coordinator_launcher_game_pid(&app),
        coordinator_launcher_game_identity(&app),
        Some(&expected_executable),
        termination_handle,
    );
    #[cfg(windows)]
    if let Some(handle) = termination_handle {
        engine::runtime::close_termination_handle(handle);
    }
    let terminated = match quit_result {
        Ok(terminated) => terminated,
        Err(error) => {
            let _ = take_force_quit_requested(&app);
            drop(operation);
            return Err(error);
        }
    };

    if !terminated {
        let _ = take_force_quit_requested(&app);
    }
    log::info!("Force quit game requested for tracked PID {tracked_pid}");
    drop(operation);
    Ok(terminated)
}

#[tauri::command]
fn switch_method(
    game_path: String,
    new_method: String,
) -> Result<engine::installer::CleanupReport, String> {
    let method = engine::method::InstallMethod::parse(&new_method)?;
    let operation = engine::operations::global()
        .try_acquire(engine::operations::OperationKind::MethodSwitch)?;
    let normalized = engine::installer::validate_installation_preconditions(&game_path, method)?;
    let expected_executable = normalized.join(engine::path::GAME_EXE_RELATIVE);
    if let Some(pid) = engine::runtime::find_game_process_id_for_path(Some(&expected_executable)) {
        drop(operation);
        return Err(format!("busy: game sedang berjalan (pid {pid})"));
    }
    log::info!(
        "Switching method for {} to {}",
        normalized.display(),
        method
    );
    let versions_path = get_appdata_dir().join("versions.json");
    let known_version = known_patch_version(&versions_path, &normalized);
    let report = engine::installer::cleanup_owned_artifacts_with_commit(&normalized, None, || {
        engine::metadata::update_installation(
            &versions_path,
            &normalized,
            known_version.as_deref(),
            method.as_str(),
            None,
        )
    })?;
    drop(operation);
    Ok(report)
}

#[tauri::command]
fn uninstall(game_path: String) -> Result<String, String> {
    let operation =
        engine::operations::global().try_acquire(engine::operations::OperationKind::Uninstall)?;
    let normalized = engine::installer::validate_signature_restore_path(&game_path)?;
    let expected_executable = normalized.join(engine::path::GAME_EXE_RELATIVE);
    if let Some(pid) = engine::runtime::find_game_process_id_for_path(Some(&expected_executable)) {
        drop(operation);
        return Err(format!("busy: game sedang berjalan (pid {pid})"));
    }
    let versions_path = get_appdata_dir().join("versions.json");
    let _report =
        engine::installer::cleanup_owned_artifacts_with_commit(&normalized, None, || {
            engine::metadata::remove_game(&versions_path, &normalized)
        })?;
    drop(operation);
    log::info!("Uninstall patch completed for: {}", normalized.display());
    Ok("ok".to_string())
}

#[tauri::command]
fn restart_as_admin() -> Result<(), String> {
    engine::elevation::restart_as_admin()?;
    log::info!("Restart as admin requested");
    Ok(())
}

// -----------------------------------------------------------------------------
// Application Entrypoint
// -----------------------------------------------------------------------------

#[cfg(windows)]
fn signal_launcher_update_ready() {
    let Some(path) = std::env::var_os(LAUNCHER_UPDATE_READY_ENV) else {
        return;
    };
    if let Err(error) = std::fs::write(path, format!("{}\n", std::process::id())) {
        log::error!("Tidak dapat menulis marker launcher update: {error}");
        // A handoff without a readiness marker cannot safely prove which
        // process to stop before rollback. Exit so the handoff can restore the
        // backup instead of copying over a live replacement executable.
        std::process::exit(1);
    }
    std::env::remove_var(LAUNCHER_UPDATE_READY_ENV);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run<R: tauri::Runtime>(context: tauri::Context<R>) {
    #[cfg(windows)]
    signal_launcher_update_ready();

    tauri::Builder::<R>::new()
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

            let _tray = TrayIconBuilder::with_id(TRAY_ICON_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        if let Err(error) = request_tray_close(app) {
                            log::debug!("Permintaan keluar dari tray ditolak: {error}");
                        }
                    }
                    "show" => {
                        restore_launcher_from_tray(app);
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
                        restore_launcher_from_tray(app);
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
            acknowledge_launcher_release_notes,
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
        .build(context)
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

#[cfg(feature = "frontend-fixture")]
pub mod frontend_fixture {
    use super::{
        check_patch_status, get_app_version, get_vh_version, is_game_running, load_settings,
        save_settings, switch_method,
    };
    use serde::Deserialize;
    use serde_json::{json, Value};
    use std::ffi::OsString;
    use std::io::{self, BufRead, BufWriter, Write};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{AppHandle, Emitter, Listener, Runtime};

    #[derive(Debug, Deserialize)]
    struct FixtureRequest {
        id: u64,
        command: String,
        #[serde(default)]
        args: Value,
    }

    struct EnvironmentGuard {
        previous_appdata: Option<OsString>,
    }

    impl Drop for EnvironmentGuard {
        fn drop(&mut self) {
            match self.previous_appdata.take() {
                Some(value) => std::env::set_var("WUWAID_E2E_APPDATA", value),
                None => std::env::remove_var("WUWAID_E2E_APPDATA"),
            }
        }
    }

    struct FixturePaths {
        root: PathBuf,
        legacy_game_path: PathBuf,
        canonical_game_path: PathBuf,
        cleanup_root: bool,
    }

    impl Drop for FixturePaths {
        fn drop(&mut self) {
            if self.cleanup_root {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
    }

    fn write_frame(writer: &Arc<Mutex<BufWriter<io::Stdout>>>, frame: Value) {
        if let Ok(mut writer) = writer.lock() {
            let _ = serde_json::to_writer(&mut *writer, &frame);
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }

    fn invoke_request(command: &str, args: Value) -> InvokeRequest {
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

    fn prepare_fixture() -> FixturePaths {
        let external_root = std::env::var_os("WUWAID_E2E_FIXTURE_ROOT");
        let cleanup_root = external_root.is_none();
        let root = external_root.map(PathBuf::from).unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "wuwaid-launcher-frontend-fixture-{}",
                std::process::id()
            ))
        });
        if cleanup_root {
            let _ = std::fs::remove_dir_all(&root);
        }
        let appdata = root.join("AppData");
        let game = root.join("Wuthering Waves");
        std::fs::create_dir_all(&appdata).unwrap();

        let executable = game.join(super::engine::path::GAME_EXE_RELATIVE);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"frontend IPC fixture game executable").unwrap();

        let canonical_game_path = std::fs::canonicalize(&game).unwrap();
        // Derive the legacy lexical alias from the canonical spelling so an
        // account-specific 8.3 alias such as RUNNER~1 cannot change identity.
        let canonical_text = canonical_game_path.to_string_lossy();
        let canonical_text = canonical_text
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical_text);
        let legacy_game_path = PathBuf::from(canonical_text)
            .join("..")
            .join("Wuthering Waves");
        std::fs::write(
            appdata.join("settings.json"),
            serde_json::json!({
                "gamePath": legacy_game_path.to_string_lossy(),
                "installMethod": "resource_mount",
                "dx11": false,
                "csharpEnvironment": false,
                "hideUid": false,
                "bgmVolume": 0.35,
                "bgmEnabled": true
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            appdata.join("versions.json"),
            br#"{"_vhVersion":"3.6.1-id.2","_installMethod":"resource_mount"}"#,
        )
        .unwrap();

        std::env::set_var("WUWAID_E2E_APPDATA", &appdata);

        FixturePaths {
            root,
            legacy_game_path,
            canonical_game_path,
            cleanup_root,
        }
    }

    #[tauri::command(rename = "check_and_sync_media")]
    fn fixture_check_and_sync_media<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
        app.emit(
            "onMediaStatus",
            json!({
                "status": "ready",
                "message": "frontend IPC fixture ready"
            }),
        )
        .map_err(|error| error.to_string())
    }

    #[tauri::command(rename = "notify_ui_interactive")]
    fn fixture_notify_ui_interactive(_install_method: String) {}

    #[tauri::command(rename = "get_vh_release_notes")]
    fn fixture_get_vh_release_notes() {}

    #[tauri::command(rename = "get_launcher_release_notes")]
    fn fixture_get_launcher_release_notes() {}

    #[tauri::command(rename = "fixture_emit_launcher_release_notes")]
    fn fixture_emit_launcher_release_notes<R: Runtime>(
        app: AppHandle<R>,
        tag: String,
    ) -> Result<(), String> {
        app.emit(
            "onLauncherReleaseNotes",
            json!({
                "tag": tag,
                "date": "2026-08-28T12:00:00Z",
                "title": "WuwaID Launcher 2.10.0",
                "body": "## What's new\\n- Verified update",
                "author": "WuwaID Team"
            }),
        )
        .map_err(|error| error.to_string())
    }

    #[tauri::command(rename = "fixture_emit_patch_status")]
    fn fixture_emit_patch_status<R: Runtime>(
        app: AppHandle<R>,
        status: String,
        game_path: String,
        install_method: String,
        uid_mode: String,
        uid_text: String,
    ) -> Result<(), String> {
        app.emit(
            "onPatchStatus",
            json!({
                "status": status,
                "gamePath": game_path,
                "installMethod": install_method,
                "uidMode": uid_mode,
                "uidText": uid_text
            }),
        )
        .map_err(|error| error.to_string())
    }

    #[tauri::command(rename = "acknowledge_launcher_release_notes")]
    fn fixture_acknowledge_launcher_release_notes(_tag: String) {}

    #[tauri::command(rename = "check_launcher_update")]
    fn fixture_check_launcher_update() {}

    fn dispatch_request(
        window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
        request: FixtureRequest,
    ) -> Value {
        let response = get_ipc_response(window, invoke_request(&request.command, request.args));
        match response {
            Ok(body) => json!({
                "type": "response",
                "id": request.id,
                "result": body.deserialize::<Value>().unwrap_or(Value::Null),
            }),
            Err(error) => json!({
                "type": "response",
                "id": request.id,
                "error": error,
            }),
        }
    }

    pub fn run() {
        let previous_appdata = std::env::var_os("WUWAID_E2E_APPDATA");
        let paths = prepare_fixture();
        let environment_guard = EnvironmentGuard { previous_appdata };
        let output = Arc::new(Mutex::new(BufWriter::new(io::stdout())));
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                get_app_version,
                get_vh_version,
                is_game_running,
                load_settings,
                save_settings,
                check_patch_status,
                switch_method,
                fixture_check_and_sync_media,
                fixture_notify_ui_interactive,
                fixture_get_vh_release_notes,
                fixture_get_launcher_release_notes,
                fixture_emit_launcher_release_notes,
                fixture_emit_patch_status,
                fixture_acknowledge_launcher_release_notes,
                fixture_check_launcher_update,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let mut listeners = Vec::new();
        for event_name in ["onPatchStatus", "onMediaStatus", "onLauncherReleaseNotes"] {
            let event_name = event_name.to_string();
            let listener_event_name = event_name.clone();
            let event_output = Arc::clone(&output);
            let listener = app.listen_any(&event_name, move |event| {
                let payload = serde_json::from_str::<Value>(event.payload())
                    .unwrap_or_else(|_| Value::String(event.payload().to_string()));
                write_frame(
                    &event_output,
                    json!({
                        "type": "event",
                        "event": listener_event_name,
                        "payload": payload
                    }),
                );
            });
            listeners.push(listener);
        }

        write_frame(
            &output,
            json!({
                "type": "ready",
                "legacyGamePath": paths.legacy_game_path,
                "canonicalGamePath": paths.canonical_game_path
            }),
        );

        let stdin = io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let Ok(request) = serde_json::from_str::<FixtureRequest>(&line) else {
                continue;
            };
            if request.command == "__shutdown" {
                write_frame(
                    &output,
                    json!({"type": "response", "id": request.id, "result": null}),
                );
                break;
            }
            let response = dispatch_request(&window, request);
            write_frame(&output, response);
        }

        for listener in listeners {
            app.unlisten(listener);
        }
        drop(window);
        drop(app);
        drop(environment_guard);
    }
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
        let guard = TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for _ in 0..200 {
            if engine::operations::global().active_operation().is_none() {
                return guard;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("test operation did not become idle")
    }

    fn conventional_canonical_path(path: &Path) -> String {
        let canonical = std::fs::canonicalize(path).unwrap();
        let text = canonical.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
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

    fn create_local_patch_source(
        root: &Path,
        include_database: bool,
        unrelated_content: &str,
    ) -> (PathBuf, String) {
        let tree = root.join("source-tree");
        let database = tree.join("Client/Content/Aki/ConfigDB/en/lang_multi_text.db");
        if include_database {
            std::fs::create_dir_all(database.parent().unwrap()).unwrap();
            let connection = rusqlite::Connection::open(&database).unwrap();
            connection
                .execute_batch(&format!(
                    "CREATE TABLE MultiText (Id TEXT, Content TEXT, RedirectDbIndex INTEGER);
                     INSERT INTO MultiText VALUES ('Text_FriendMyUid_Text', 'ID Pengguna: {{0}}', 0);
                     INSERT INTO MultiText VALUES ('Text_UserId_Text', 'ID Pengguna: {{0}}', 0);
                     INSERT INTO MultiText VALUES ('PrefabTextItem_1341587207_Text', 'UID:00000000000', 0);
                     INSERT INTO MultiText VALUES ('Unrelated_Text', '{unrelated_content}', 0);"
                ))
                .unwrap();
        } else {
            let unrelated = tree.join("Client/Content/unrelated.txt");
            std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
            std::fs::write(unrelated, unrelated_content).unwrap();
        }
        let source = root.join(engine::patch_asset::NORMAL_PAK_FILE_NAME);
        engine::repak::pack_v12(&tree, &source).unwrap();
        let hash = engine::downloader::compute_sha256(&source).unwrap();
        (source, hash)
    }

    #[test]
    fn normal_patch_preparation_returns_the_verified_source_pak() {
        let temp = tempfile::tempdir().unwrap();
        let (source, hash) = create_local_patch_source(temp.path(), true, "keep");
        let cache = temp.path().join("cache");

        let prepared = prepare_cached_patch_asset(
            &source,
            &hash,
            &cache,
            engine::patch_asset::PatchVariant::Normal,
        )
        .unwrap();

        assert_eq!(prepared, source);
        assert!(!engine::patch_asset::derived_pak_path(&cache, &hash).exists());
    }

    fn assert_hide_failure_does_not_fallback(
        source: &Path,
        expected_hash: &str,
        cache: &Path,
    ) -> String {
        let source_before = std::fs::read(source).unwrap();
        let error = prepare_cached_patch_asset(
            source,
            expected_hash,
            cache,
            engine::patch_asset::PatchVariant::HideUid,
        )
        .unwrap_err();
        assert_eq!(std::fs::read(source).unwrap(), source_before);
        assert!(!engine::patch_asset::derived_pak_path(cache, expected_hash).exists());
        error
    }

    #[test]
    fn failed_hide_uid_preparation_never_falls_back_to_normal_pak() {
        let temp = tempfile::tempdir().unwrap();
        let (source, hash) = create_local_patch_source(temp.path(), false, "normal-only");
        let missing_database_error = assert_hide_failure_does_not_fallback(
            &source,
            &hash,
            &temp.path().join("missing-database-cache"),
        );
        assert!(missing_database_error.contains("hide_uid_database_missing"));

        let checksum_error = assert_hide_failure_does_not_fallback(
            &source,
            &"0".repeat(64),
            &temp.path().join("checksum-cache"),
        );
        assert!(checksum_error.contains("Integritas PAK normal gagal"));

        let invalid_source = temp.path().join("invalid-source.pak");
        std::fs::write(&invalid_source, b"not a valid V12 pak").unwrap();
        let invalid_hash = engine::downloader::compute_sha256(&invalid_source).unwrap();
        let invalid_pak_error = assert_hide_failure_does_not_fallback(
            &invalid_source,
            &invalid_hash,
            &temp.path().join("invalid-source-cache"),
        );
        assert!(invalid_pak_error.contains("Struktur PAK rilis tidak valid"));
    }

    #[test]
    fn uid_customization_custom_preparation_never_falls_back_to_normal_pak() {
        let temp = tempfile::tempdir().unwrap();
        let (source, hash) = create_local_patch_source(temp.path(), false, "normal-only");
        let cache = temp.path().join("custom-cache");
        let source_before = std::fs::read(&source).unwrap();

        let error = prepare_cached_patch_asset(
            &source,
            &hash,
            &cache,
            engine::patch_asset::PatchVariant::Custom("Halo Nozomi".to_string()),
        )
        .unwrap_err();

        assert_eq!(std::fs::read(&source).unwrap(), source_before);
        assert!(error.contains("hide_uid_database_missing"));
        let has_pak_output = std::fs::read_dir(&cache)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|extension| extension == "pak")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        assert!(!has_pak_output, "custom failure produced a fallback PAK");
    }

    #[test]
    fn window_minimize_action_distinguishes_normal_and_tray_modes() {
        assert_eq!(
            window_minimize_action(false),
            WindowMinimizeAction::Minimize
        );
        assert_eq!(window_minimize_action(true), WindowMinimizeAction::Hide);
        assert!(should_hide_to_tray(false, Some(42)));
        assert!(!should_hide_to_tray(false, None));
        assert_eq!(launcher_force_quit_pid(Some(42)).unwrap(), 42);
        assert!(launcher_force_quit_pid(None)
            .unwrap_err()
            .contains("not_launcher_launched"));
    }

    #[test]
    fn tray_mode_tracks_icon_visibility_contract() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();

        set_tray_mode(handle, false);
        assert!(!is_tray_mode(handle));
        set_tray_mode(handle, true);
        assert!(is_tray_mode(handle));
        set_tray_mode(handle, false);
        assert!(!is_tray_mode(handle));
    }

    #[test]
    fn tray_notification_body_is_explicit() {
        assert_eq!(
            tray_notification_body(),
            "Launcher berjalan di system tray. Klik ikon tray untuk membukanya kembali."
        );
    }

    #[test]
    fn restoring_launcher_from_tray_delivers_recovery_state() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let (tray_tx, tray_rx) = sync_channel(1);
        let tray_listener = app.listen_any("onLauncherTrayState", move |event| {
            let _ = tray_tx.send(event.payload().to_string());
        });

        set_tray_mode(handle, true);
        assert!(is_tray_mode(handle));

        restore_launcher_from_tray(handle);

        assert!(!is_tray_mode(handle));
        assert!(window.is_visible().unwrap());
        let payload: serde_json::Value =
            serde_json::from_str(&tray_rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(payload["inTray"], false);
        assert_eq!(
            window_minimize_action(is_tray_mode(handle)),
            WindowMinimizeAction::Minimize
        );
        app.unlisten(tray_listener);
    }

    #[test]
    fn launcher_game_minimize_stays_in_tray_after_restore() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .plugin(tauri_plugin_notification::init())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let (tray_tx, tray_rx) = sync_channel(4);
        let tray_listener = app.listen_any("onLauncherTrayState", move |event| {
            let _ = tray_tx.send(event.payload().to_string());
        });

        set_launcher_process(handle, Some(42));
        set_tray_mode(handle, true);
        restore_launcher_from_tray(handle);
        assert!(window.is_visible().unwrap());
        assert_eq!(
            tray_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "{\"inTray\":false}"
        );

        minimize_window(window.clone());
        assert!(is_tray_mode(handle));
        assert_eq!(
            tray_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "{\"inTray\":true}"
        );

        restore_launcher_from_tray(handle);
        assert!(!is_tray_mode(handle));
        assert!(window.is_visible().unwrap());
        assert_eq!(
            tray_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "{\"inTray\":false}"
        );

        minimize_window(window.clone());
        assert!(is_tray_mode(handle));
        assert_eq!(
            tray_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            "{\"inTray\":true}"
        );
        app.unlisten(tray_listener);
    }

    #[test]
    fn force_quit_exit_restores_launcher_lifecycle() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let evidence = engine::runtime::LaunchEvidence::for_process(
            engine::runtime::LaunchCommand::new(
                Path::new(r"C:\\Games\\Client-Win64-Shipping.exe"),
                Path::new(r"C:\\Games"),
                false,
            ),
            engine::runtime::LaunchMode::Direct,
            42,
        );
        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onGameExit", move |event| {
            let _ = tx.send(event.payload().to_string());
        });

        set_launcher_process(handle, Some(42));
        set_tray_mode(handle, true);
        mark_force_quit_requested(handle);
        assert!(take_force_quit_requested(handle));
        complete_launcher_exit(
            handle,
            &evidence,
            "force_quit",
            "Proses dihentikan oleh launcher.",
        );

        let payload: serde_json::Value =
            serde_json::from_str(&rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(payload["status"], "force_quit");
        assert!(!is_tray_mode(handle));
        assert!(window.is_visible().unwrap());
        assert!(coordinator_launcher_pid(handle).is_none());
        assert!(appdata.path().join("Diagnostics").exists());
        app.unlisten(listener);
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn external_game_never_emits_launcher_exit_notice_or_restores_tray() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let evidence = engine::runtime::LaunchEvidence::for_process(
            engine::runtime::LaunchCommand::new(
                Path::new(r"C:\\Games\\Client-Win64-Shipping.exe"),
                Path::new(r"C:\\Games"),
                false,
            ),
            engine::runtime::LaunchMode::Direct,
            99,
        );
        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onGameExit", move |_event| {
            let _ = tx.send(());
        });
        let (tray_tx, tray_rx) = sync_channel(1);
        let tray_listener = app.listen_any("onLauncherTrayState", move |event| {
            let _ = tray_tx.send(event.payload().to_string());
        });

        let state = engine::runtime::reconcile_runtime_state(None, Some(99));
        assert_eq!(state.origin, engine::runtime::ProcessOrigin::External);
        assert!(state.active);
        assert!(coordinator_launcher_pid(handle).is_none());
        emit_game_exit_notice(
            handle,
            &evidence,
            engine::runtime::ProcessOrigin::External,
            "normal",
            "Proses eksternal selesai.",
        );
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        minimize_window(window.clone());
        assert!(!is_tray_mode(handle));
        assert!(tray_rx.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(window.is_visible().unwrap());
        app.unlisten(listener);
        app.unlisten(tray_listener);
    }

    #[test]
    fn finishing_launch_lifecycle_delivers_runtime_and_completion_events() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let _window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let (runtime_tx, runtime_rx) = sync_channel(1);
        let (finish_tx, finish_rx) = sync_channel(1);
        let runtime_listener = app.listen_any("onGameRuntimeState", move |event| {
            let _ = runtime_tx.send(event.payload().to_string());
        });
        let finish_listener = app.listen_any("onGameLaunchFinished", move |event| {
            let _ = finish_tx.send(event.payload().to_string());
        });

        set_launcher_process(handle, Some(42));
        set_tray_mode(handle, true);
        finish_launch_lifecycle(handle);

        let runtime_payload: serde_json::Value =
            serde_json::from_str(&runtime_rx.recv_timeout(Duration::from_secs(1)).unwrap())
                .unwrap();
        assert_eq!(runtime_payload["active"], false);
        assert!(finish_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        assert!(!is_tray_mode(handle));
        assert!(coordinator_launcher_pid(handle).is_none());
        app.unlisten(runtime_listener);
        app.unlisten(finish_listener);
    }

    #[test]
    fn game_exit_notice_payload_is_compact_and_stable() {
        let app = tauri::test::mock_builder()
            .manage(RuntimeCoordinator::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let evidence = engine::runtime::LaunchEvidence::for_process(
            engine::runtime::LaunchCommand::new(
                Path::new(r"C:\Games\Client-Win64-Shipping.exe"),
                Path::new(r"C:\Games"),
                false,
            ),
            engine::runtime::LaunchMode::Direct,
            42,
        );
        let (tx, rx) = sync_channel(1);
        let listener = app.listen_any("onGameExit", move |event| {
            let _ = tx.send(event.payload().to_string());
        });

        emit_game_exit_notice(
            handle,
            &evidence,
            engine::runtime::ProcessOrigin::Launcher,
            "crashed",
            "Proses berhenti.",
        );

        let payload: serde_json::Value =
            serde_json::from_str(&rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(payload["id"], format!("{}:42", evidence.started_at_ms));
        assert_eq!(payload["status"], "crashed");
        assert_eq!(payload["reason"], "Proses berhenti.");
        assert!(window.is_visible().unwrap());
        app.unlisten(listener);
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
    fn launcher_release_note_cache_round_trips_and_replaces_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("launcher-release-notes.json");
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: "2026-08-28T12:00:00Z".to_string(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            body: "## What's new".to_string(),
            author: "WuwaID Team".to_string(),
        };

        write_launcher_release_note(&path, &note).unwrap();

        assert_eq!(read_launcher_release_note(&path), Some(note));
        let temporary_files = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".launcher-release-notes.json.tmp-")
            })
            .count();
        assert_eq!(temporary_files, 0);
    }

    #[test]
    fn launcher_release_note_cache_write_surfaces_target_errors() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("target-directory");
        std::fs::create_dir(&path).unwrap();
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: String::new(),
            title: "Launcher".to_string(),
            body: "Notes".to_string(),
            author: "Team".to_string(),
        };

        assert!(write_launcher_release_note(&path, &note).is_err());
        assert!(path.is_dir());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".target-directory.tmp-")
                })
                .count(),
            0
        );
    }

    #[test]
    fn launcher_release_note_version_match_handles_tag_prefix_only() {
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: String::new(),
            title: "Launcher".to_string(),
            body: "Notes".to_string(),
            author: "Team".to_string(),
        };

        assert!(launcher_release_note_matches_version(&note, "2.10.0"));
        assert!(launcher_release_note_matches_version(&note, "V2.10.0"));
        assert!(!launcher_release_note_matches_version(&note, "2.10.1"));
    }

    #[test]
    fn acknowledging_pending_release_notes_requires_matching_tag() {
        let temp = tempfile::tempdir().unwrap();
        let transaction = temp.path().join("launcher-whats-new-transaction.json");
        let pending = temp.path().join("launcher-whats-new-pending.json");
        let ready = temp.path().join("launcher-whats-new-ready.tag");
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: String::new(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            body: "Notes".to_string(),
            author: "Team".to_string(),
        };
        write_launcher_release_note(&pending, &note).unwrap();
        std::fs::write(&ready, "v2.10.0\n").unwrap();

        launcher_update_state::acknowledge(&transaction, &pending, &ready, "v2.9.2").unwrap();
        assert!(pending.exists());
        assert!(ready.exists());
        std::fs::write(&transaction, "in progress").unwrap();
        launcher_update_state::acknowledge(&transaction, &pending, &ready, "2.10.0").unwrap();
        assert!(transaction.exists());
        assert!(pending.exists());
        assert!(ready.exists());
        std::fs::remove_file(&transaction).unwrap();
        launcher_update_state::acknowledge(&transaction, &pending, &ready, "2.10.0").unwrap();
        assert!(!transaction.exists());
        assert!(!pending.exists());
        assert!(!ready.exists());
    }

    #[test]
    fn committed_launcher_release_notes_require_matching_ready_marker() {
        let temp = tempfile::tempdir().unwrap();
        let pending = temp.path().join("launcher-whats-new-pending.json");
        let ready = temp.path().join("launcher-whats-new-ready.tag");
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: String::new(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            body: "Notes".to_string(),
            author: "Team".to_string(),
        };
        write_launcher_release_note(&pending, &note).unwrap();

        let transaction = temp.path().join("launcher-whats-new-transaction.json");
        assert!(
            read_committed_launcher_release_note(&transaction, &pending, &ready, "2.10.0")
                .is_none()
        );
        std::fs::write(&ready, "v2.9.2\n").unwrap();
        assert!(
            read_committed_launcher_release_note(&transaction, &pending, &ready, "2.10.0")
                .is_none()
        );
        std::fs::write(&ready, "v2.10.0\n").unwrap();
        assert_eq!(
            read_committed_launcher_release_note(&transaction, &pending, &ready, "2.10.0"),
            Some(note)
        );
        std::fs::write(&transaction, "in progress").unwrap();
        assert!(
            read_committed_launcher_release_note(&transaction, &pending, &ready, "2.10.0")
                .is_none()
        );
    }

    #[test]
    fn invalidated_launcher_release_note_state_cannot_be_displayed() {
        let temp = tempfile::tempdir().unwrap();
        let transaction = temp.path().join("launcher-whats-new-transaction.json");
        let pending = temp.path().join("launcher-whats-new-pending.json");
        let ready = temp.path().join("launcher-whats-new-ready.tag");
        std::fs::write(&transaction, "transaction").unwrap();
        std::fs::write(&pending, "pending").unwrap();
        std::fs::write(&ready, "v2.10.0\n").unwrap();

        launcher_update_state::invalidate(&transaction, &pending, &ready).unwrap();
        assert!(
            read_committed_launcher_release_note(&transaction, &pending, &ready, "2.10.0")
                .is_none()
        );
        assert!(!transaction.exists());
        assert!(!pending.exists());
        assert!(!ready.exists());
    }

    #[test]
    fn invalid_launcher_release_body_gets_safe_fallback_without_blocking_update() {
        let release = engine::updater::ReleaseInfo {
            tag_name: "v2.10.0".to_string(),
            version: "2.10.0".to_string(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            date: "2026-08-28T12:00:00Z".to_string(),
            author: "WuwaID Team".to_string(),
            body: "<script>alert(1)</script>".to_string(),
            zip_url: Some("https://github.com/TitoTFP/WuwaIDLauncher/releases/download/v2.10.0/WuwaIDLauncher-v2.10.0.zip".to_string()),
            checksums_url: Some("https://github.com/TitoTFP/WuwaIDLauncher/releases/download/v2.10.0/SHA256sums.txt".to_string()),
        };

        let note = launcher_release_note_for_release(&release);

        assert_eq!(note.tag, "v2.10.0");
        assert_eq!(note.title, release.title);
        assert_eq!(note.date, release.date);
        assert_eq!(note.author, release.author);
        assert!(note.body.contains("belum tersedia"));
        assert!(!note.body.contains("script"));
    }

    #[test]
    fn empty_launcher_release_body_gets_fallback_summary() {
        let release = engine::updater::ReleaseInfo {
            tag_name: "v2.10.0".to_string(),
            version: "2.10.0".to_string(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            date: String::new(),
            author: "WuwaID Team".to_string(),
            body: "  \n".to_string(),
            zip_url: None,
            checksums_url: None,
        };

        let note = launcher_release_note_for_release(&release);

        assert_eq!(
            note.body,
            "Catatan rilis belum tersedia. Launcher berhasil diperbarui."
        );
    }

    #[test]
    fn launcher_update_payload_carries_full_release_note() {
        let release = engine::updater::ReleaseInfo {
            tag_name: "v2.10.0".to_string(),
            version: "2.10.0".to_string(),
            title: "Ignored by note".to_string(),
            date: "ignored".to_string(),
            author: "ignored".to_string(),
            body: "ignored".to_string(),
            zip_url: None,
            checksums_url: None,
        };
        let note = engine::atom_feed::ReleaseNoteEntry {
            tag: "v2.10.0".to_string(),
            date: "2026-08-28".to_string(),
            title: "WuwaID Launcher 2.10.0".to_string(),
            body: "## Notes".to_string(),
            author: "WuwaID Team".to_string(),
        };

        let payload = launcher_update_payload(&release, &note);

        assert_eq!(payload["version"], "2.10.0");
        assert_eq!(payload["tag"], "v2.10.0");
        assert_eq!(payload["date"], "2026-08-28");
        assert_eq!(payload["title"], "WuwaID Launcher 2.10.0");
        assert_eq!(payload["author"], "WuwaID Team");
        assert_eq!(payload["body"], "## Notes");
    }

    #[test]
    fn test_real_tauri_command_path_and_event_delivery() {
        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                get_app_version,
                check_game_folder_write_access
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
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

        let mut started = false;
        for _ in 0..80 {
            match check_and_sync_media(app.handle().clone()) {
                Ok(()) => {
                    started = true;
                    break;
                }
                Err(error) if error.starts_with("busy:") => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => panic!("unexpected media command error: {error}"),
            }
        }
        assert!(started, "media operation did not become available");
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
        let listener_addr = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener_addr.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener_addr.accept().unwrap();
            use std::io::{Read, Write};
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = b"not-json";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        std::env::set_var("WUWAID_ASSETS_URL", format!("http://{address}/assets.json"));
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (ready_tx, ready_rx) = sync_channel(1);
        let ready_listener = app.listen_any("onMediaReady", move |event| {
            let _ = ready_tx.send(event.payload().to_string());
        });
        let (status_tx, status_rx) = sync_channel(2);
        let status_listener = app.listen_any("onMediaStatus", move |event| {
            let _ = status_tx.send(event.payload().to_string());
        });

        let mut started = false;
        for _ in 0..80 {
            match check_and_sync_media(app.handle().clone()) {
                Ok(()) => {
                    started = true;
                    break;
                }
                Err(error) if error.starts_with("busy:") => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => panic!("unexpected media command error: {error}"),
            }
        }
        assert!(started, "media operation did not become available");
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
        let _ = status_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let status = status_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(status.contains("offline") || status.contains("error"));

        app.unlisten(ready_listener);
        app.unlisten(status_listener);
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
    fn unranged_large_media_response_is_bounded() {
        let appdata = tempfile::tempdir().unwrap();
        let cache = appdata.path().join("Cache");
        std::fs::create_dir_all(&cache).unwrap();
        let total_len = MAX_UNRANGED_MEDIA_RESPONSE_BYTES as usize + 1;
        std::fs::write(cache.join("bg-video.mp4"), vec![b'x'; total_len]).unwrap();

        let request = tauri::http::Request::builder()
            .uri("media://localhost/bg-video.mp4")
            .body(Vec::new())
            .unwrap();
        let response = registered_media_protocol_response(appdata.path(), request);

        assert_eq!(response.status(), 206);
        assert_eq!(
            response.body().len(),
            MAX_UNRANGED_MEDIA_RESPONSE_BYTES as usize
        );
        assert_eq!(
            response
                .headers()
                .get("Content-Range")
                .unwrap()
                .to_str()
                .unwrap(),
            format!(
                "bytes 0-{}/{}",
                MAX_UNRANGED_MEDIA_RESPONSE_BYTES - 1,
                total_len
            )
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

    #[test]
    fn legacy_v290_appdata_survives_method_switch_and_emits_status() {
        let _env_lock = lock_test_environment();
        let appdata = tempfile::tempdir().unwrap();
        std::env::set_var("WUWAID_E2E_APPDATA", appdata.path());
        tauri::async_runtime::block_on(run_legacy_v290_appdata_scenario(appdata.path()));
        std::env::remove_var("WUWAID_E2E_APPDATA");
    }

    #[test]
    fn release_gate_fixture_round_trip_uses_run_owned_state() {
        let Some(root) = std::env::var_os("WUWAID_RELEASE_GATE_FIXTURE_ROOT") else {
            return;
        };
        let _env_lock = lock_test_environment();
        let root = PathBuf::from(root);
        let appdata = root.join("AppData");
        let game = root.join("Wuthering Waves");
        assert!(appdata.is_dir(), "release gate AppData fixture is missing");
        assert!(
            game.join(engine::path::GAME_EXE_RELATIVE).is_file(),
            "release gate game fixture is missing"
        );

        let previous_appdata = std::env::var_os("WUWAID_E2E_APPDATA");
        std::env::set_var("WUWAID_E2E_APPDATA", &appdata);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let loaded = load_settings().unwrap();
            assert_eq!(
                loaded.settings.install_method,
                engine::method::InstallMethod::ResourceMount
            );
            save_settings(serde_json::to_string(&loaded.settings).unwrap()).unwrap();
            std::fs::write(appdata.join(".release-gate-cargo-test-ran"), b"ok").unwrap();
        }));
        match previous_appdata {
            Some(value) => std::env::set_var("WUWAID_E2E_APPDATA", value),
            None => std::env::remove_var("WUWAID_E2E_APPDATA"),
        }
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    async fn run_legacy_v290_appdata_scenario(appdata: &Path) {
        let workspace = tempfile::tempdir().unwrap();
        let game = workspace.path().join("Wuthering Waves");
        let alias_parent = workspace.path().join("legacy-alias");
        let executable = game.join(engine::path::GAME_EXE_RELATIVE);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&alias_parent).unwrap();
        std::fs::write(&executable, b"mock game executable").unwrap();

        let legacy_game_path = alias_parent.join("..").join("Wuthering Waves");
        let canonical_game_path = std::fs::canonicalize(&game).unwrap();
        let settings_path = appdata.join("settings.json");
        let versions_path = appdata.join("versions.json");
        std::fs::write(
            &settings_path,
            serde_json::json!({
                "gamePath": legacy_game_path.to_string_lossy(),
                "installMethod": "resource_mount",
                "dx11": false,
                "csharpEnvironment": false,
                "hideUid": false,
                "bgmVolume": 0.35,
                "bgmEnabled": true
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            &versions_path,
            br#"{"_vhVersion":"3.6.1-id.2","_installMethod":"resource_mount"}"#,
        )
        .unwrap();
        let loaded = load_settings().unwrap();
        assert_eq!(
            loaded.settings.game_path,
            canonical_game_path.to_string_lossy()
        );
        assert_eq!(
            loaded.settings.install_method,
            engine::method::InstallMethod::ResourceMount
        );
        let repaired_settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(
            repaired_settings["gamePath"],
            canonical_game_path.to_string_lossy().as_ref()
        );

        let resources = game
            .join("Client")
            .join("Saved")
            .join("Resources")
            .join("3.6.1");
        let mount_dir = resources.join("Mount");
        let official_dir = resources.join("Lang_en").join("Base");
        std::fs::create_dir_all(&mount_dir).unwrap();
        std::fs::create_dir_all(&official_dir).unwrap();
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
        let plan = engine::installer::probe_resource_mount(&game).unwrap();
        let source_pak = workspace.path().join("legacy-patch.pak");
        let pak_bytes = engine::pak::pack(
            "../../../",
            0,
            &[(
                "Content/Localization/id.txt".to_string(),
                b"Bahasa Indonesia".to_vec(),
            )],
        )
        .unwrap();
        std::fs::write(&source_pak, pak_bytes).unwrap();
        engine::installer::deploy_resource_mount(&plan, &source_pak, &game).unwrap();
        assert!(engine::installer::validate_installed_resource_mount(&plan).unwrap());

        // Mirror the frontend's persisted selection before invoking the backend switch.
        save_settings(
            serde_json::json!({
                "gamePath": loaded.settings.game_path,
                "installMethod": "loader",
                "dx11": false,
                "csharpEnvironment": false,
                "hideUid": false,
                "bgmVolume": 0.35,
                "bgmEnabled": true
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            load_settings().unwrap().settings.install_method,
            engine::method::InstallMethod::Loader
        );

        let report = switch_method(
            legacy_game_path.to_string_lossy().to_string(),
            "loader".to_string(),
        )
        .unwrap();
        assert!(report.failures.is_empty());
        assert!(report.preserved.is_empty());
        assert!(!plan.pak_path.exists());
        assert!(!plan.sig_path.exists());
        assert!(!plan.mount_path.exists());
        assert!(!plan.owner_marker_path.exists());

        let metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&versions_path).unwrap()).unwrap();
        assert_eq!(metadata["_vhVersion"], "3.6.1-id.2");
        assert_eq!(metadata["_installMethod"], "loader");
        let game_key = engine::metadata::game_key(&game).unwrap();
        let game_metadata = metadata["games"]
            .as_object()
            .unwrap()
            .get(&game_key)
            .unwrap();
        assert_eq!(game_metadata["_vhVersion"], "3.6.1-id.2");
        assert_eq!(game_metadata["_installMethod"], "loader");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let (status_tx, status_rx) = sync_channel(1);
        let status_listener = app.listen_any("onPatchStatus", move |event| {
            let _ = status_tx.send(event.payload().to_string());
        });

        check_patch_status(
            app.handle().clone(),
            legacy_game_path.to_string_lossy().to_string(),
            "loader".to_string(),
            "default".to_string(),
            String::new(),
        )
        .await
        .unwrap();
        let after_switch: serde_json::Value =
            serde_json::from_str(&status_rx.recv_timeout(Duration::from_secs(1)).unwrap()).unwrap();
        assert_eq!(after_switch["status"], "not_installed");
        assert_eq!(
            after_switch["gamePath"],
            canonical_game_path.to_string_lossy().as_ref()
        );
        assert_eq!(after_switch["installMethod"], "loader");
        assert_eq!(after_switch["currentVersion"], "3.6.1-id.2");

        app.unlisten(status_listener);
    }

    #[cfg(windows)]
    #[test]
    fn windows_real_localappdata_upgrade_preserves_unrelated_state() {
        let _env_lock = lock_test_environment();
        let local_appdata = tempfile::tempdir().unwrap();
        let previous_e2e = std::env::var_os("WUWAID_E2E_APPDATA");
        let previous_local = std::env::var_os("LOCALAPPDATA");
        std::env::remove_var("WUWAID_E2E_APPDATA");
        std::env::set_var("LOCALAPPDATA", local_appdata.path());

        let launcher_appdata = get_appdata_dir();
        assert_eq!(
            launcher_appdata,
            local_appdata.path().join("WuwaIDLauncher")
        );
        std::fs::create_dir_all(&launcher_appdata).unwrap();
        let unrelated_path = launcher_appdata.join("unrelated-appdata.json");
        std::fs::write(&unrelated_path, br#"{"keep":true}"#).unwrap();

        tauri::async_runtime::block_on(run_legacy_v290_appdata_scenario(&launcher_appdata));

        assert_eq!(
            std::fs::read_to_string(&unrelated_path).unwrap(),
            r#"{"keep":true}"#
        );
        assert!(launcher_appdata.join("settings.json").is_file());
        assert!(launcher_appdata.join("versions.json").is_file());

        match previous_e2e {
            Some(value) => std::env::set_var("WUWAID_E2E_APPDATA", value),
            None => std::env::remove_var("WUWAID_E2E_APPDATA"),
        }
        match previous_local {
            Some(value) => std::env::set_var("LOCALAPPDATA", value),
            None => std::env::remove_var("LOCALAPPDATA"),
        }
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

        check_patch_status(
            app.handle().clone(),
            String::new(),
            "bogus".to_string(),
            "default".to_string(),
            String::new(),
        )
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
        let expected_removed = vec![
            conventional_canonical_path(&foreign),
            conventional_canonical_path(&legacy_marker),
        ];

        let report = switch_method(
            game.path().to_string_lossy().to_string(),
            "loader".to_string(),
        )
        .unwrap();
        assert!(report.failures.is_empty());
        assert!(report.preserved.is_empty());
        assert_eq!(report.removed, expected_removed);
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

        let (launch_error_tx, launch_error_rx) = sync_channel(1);
        let launch_error_listener = app.listen_any("onLaunchError", move |event| {
            let _ = launch_error_tx.send(event.payload().to_string());
        });
        assert_ipc_response(
            &window,
            ipc_request(
                "launch_game",
                serde_json::json!({
                    "gamePath": game.path().parent().unwrap().join("missing-game").to_string_lossy(),
                    "dx11": false,
                    "csharpEnvironment": false,
                    "installMethod": "loader",
                }),
            ),
            Ok(()),
        );
        assert!(launch_error_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .contains("invalid_game_path: executable game tidak ditemukan"));
        let diagnostics_dir = appdata.path().join("Diagnostics");
        let diagnostic_files = std::fs::read_dir(&diagnostics_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(diagnostic_files.len(), 1);
        let diagnostic = std::fs::read_to_string(diagnostic_files[0].path()).unwrap();
        assert!(diagnostic.contains("invalid_game_path: executable game tidak ditemukan"));
        app.unlisten(launch_error_listener);

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
                    "uidMode": "default",
                    "uidText": "",
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
                serde_json::json!({
                    "gamePath": "",
                    "installMethod": "unknown",
                    "uidMode": "default",
                    "uidText": ""
                }),
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
                "removed": [conventional_canonical_path(&foreign)],
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

    #[test]
    fn mutating_command_propagates_busy_operation_errors() {
        let _env_lock = lock_test_environment();
        let guard = engine::operations::global()
            .try_acquire(engine::operations::OperationKind::PatchInstall)
            .unwrap();
        let error = switch_method("not-a-game".to_string(), "loader".to_string()).unwrap_err();
        drop(guard);

        assert!(error.starts_with("busy:"));
    }
}
