use std::io::Cursor;

use tempfile::tempdir;
use wuwaid_launcher_lib::engine::{log_collector, settings, telemetry};

#[test]
fn diagnostics_json_redaction_removes_paths_and_sensitive_metadata() {
    let redacted = log_collector::redact_json_document(
        br#"{"gamePath":"C:\\Games\\Wuthering Waves","clientId":"secret","safe":true}"#,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&redacted).unwrap();
    assert!(value.get("gamePath").is_none());
    assert!(value.get("clientId").is_none());
    assert_eq!(value["safe"], true);
}

#[test]
fn telemetry_is_sent_only_after_explicit_opt_in() {
    assert!(telemetry::should_send_telemetry(true));
    assert!(!telemetry::should_send_telemetry(false));
}

#[test]
fn diagnostics_upload_policy_is_bounded_and_local_fallback_is_recoverable() {
    assert_eq!(log_collector::max_upload_attempts(), 2);

    let temp = tempdir().unwrap();
    let path = log_collector::save_logs_bundle(b"verified zip bytes", temp.path()).unwrap();
    assert!(path.starts_with(temp.path()));
    assert_eq!(std::fs::read(path).unwrap(), b"verified zip bytes");
}

#[test]
fn diagnostics_and_telemetry_default_to_disabled() {
    let defaults = settings::LauncherSettings::default();
    assert!(!defaults.diagnostics_upload_enabled);
    assert!(!defaults.telemetry_enabled);
}

#[tokio::test]
async fn diagnostics_collection_contains_redacted_settings() {
    let temp = tempdir().unwrap();
    let game = temp.path().join("game");
    let appdata = temp.path().join("appdata");
    std::fs::create_dir_all(game.join("Client").join("Saved").join("Logs")).unwrap();
    std::fs::create_dir_all(&appdata).unwrap();
    std::fs::write(
        appdata.join("settings.json"),
        br#"{"gamePath":"C:\\Private","telemetryEnabled":false}"#,
    )
    .unwrap();
    let bytes = log_collector::collect_logs_to_zip(&game, &appdata).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut settings = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("launcher/settings.json").unwrap(),
        &mut settings,
    )
    .unwrap();
    assert!(!settings.contains("C:\\Private"));
}
