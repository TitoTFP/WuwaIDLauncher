use wuwaid_launcher_lib::engine::settings;

#[test]
fn legacy_remote_diagnostics_settings_are_not_serialized() {
    let result = settings::normalize_settings_json(
        r#"{"diagnosticsUploadEnabled":true,"telemetryEnabled":true}"#,
    );
    let serialized = serde_json::to_value(result.settings).unwrap();

    assert!(result.repaired);
    assert!(!serialized
        .as_object()
        .unwrap()
        .contains_key("diagnosticsUploadEnabled"));
    assert!(!serialized
        .as_object()
        .unwrap()
        .contains_key("telemetryEnabled"));
    assert!(result
        .diagnostics
        .iter()
        .any(|message| message.contains("server belum tersedia")));
}

#[test]
fn launcher_settings_default_to_local_only_operations() {
    let serialized = serde_json::to_value(settings::LauncherSettings::default()).unwrap();
    let object = serialized.as_object().unwrap();

    assert!(!object.contains_key("diagnosticsUploadEnabled"));
    assert!(!object.contains_key("telemetryEnabled"));
}
