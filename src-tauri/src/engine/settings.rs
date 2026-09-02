use super::method::InstallMethod;
use crate::engine::path::normalize_game_path;
use crate::engine::validate_uid_text;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub game_path: String,
    pub install_method: InstallMethod,
    pub dx11: bool,
    pub csharp_environment: bool,
    pub uid_mode: String,
    pub uid_text: String,
    pub bgm_volume: f64,
    pub bgm_enabled: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            game_path: String::new(),
            install_method: InstallMethod::ResourceMount,
            dx11: false,
            csharp_environment: false,
            uid_mode: "default".to_string(),
            uid_text: String::new(),
            bgm_volume: 0.35,
            bgm_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsLoadResult {
    pub settings: LauncherSettings,
    pub repaired: bool,
    pub diagnostics: Vec<String>,
}

fn diagnostic(diagnostics: &mut Vec<String>, message: impl Into<String>) {
    diagnostics.push(message.into());
}

fn read_bool(
    object: &Map<String, Value>,
    key: &str,
    target: &mut bool,
    diagnostics: &mut Vec<String>,
    repaired: &mut bool,
) {
    let Some(value) = object.get(key) else {
        return;
    };
    if let Some(value) = value.as_bool() {
        *target = value;
    } else {
        *repaired = true;
        diagnostic(
            diagnostics,
            format!("Field settings {key} tidak valid; memakai default."),
        );
    }
}

pub fn normalize_settings_json(raw: &str) -> SettingsLoadResult {
    let value = match serde_json::from_str::<Value>(raw) {
        Ok(value) => value,
        Err(error) => {
            return SettingsLoadResult {
                settings: LauncherSettings::default(),
                repaired: true,
                diagnostics: vec![format!(
                    "settings.json rusak dan dipulihkan ke default: {error}"
                )],
            };
        }
    };

    let Some(object) = value.as_object() else {
        return SettingsLoadResult {
            settings: LauncherSettings::default(),
            repaired: true,
            diagnostics: vec!["settings.json bukan object dan dipulihkan ke default.".to_string()],
        };
    };

    let mut settings = LauncherSettings::default();
    let mut diagnostics = Vec::new();
    let mut repaired = false;

    if let Some(value) = object.get("gamePath") {
        match value.as_str() {
            Some(path) if path.trim().is_empty() => settings.game_path.clear(),
            Some(path) => match normalize_game_path(path) {
                Some(normalized) => {
                    // Release builds before the canonical-path migration persisted the
                    // picker result verbatim. Canonicalize it while loading so status
                    // events and metadata use the same identity after an upgrade.
                    let normalized = std::fs::canonicalize(&normalized).unwrap_or(normalized);
                    let normalized = normalized.to_string_lossy().to_string();
                    if normalized != path {
                        repaired = true;
                        diagnostic(
                            &mut diagnostics,
                            "Path game dinormalisasi ke folder instalasi yang valid.",
                        );
                    }
                    settings.game_path = normalized;
                }
                None => {
                    repaired = true;
                    diagnostic(
                        &mut diagnostics,
                        "Path game tidak valid dan dikosongkan; pilih folder game lagi.",
                    );
                }
            },
            None => {
                repaired = true;
                diagnostic(
                    &mut diagnostics,
                    "Field settings gamePath tidak valid; memakai default.",
                );
            }
        }
    }

    if let Some(value) = object.get("installMethod") {
        match value
            .as_str()
            .and_then(|value| InstallMethod::parse(value).ok())
        {
            Some(method) => settings.install_method = method,
            None => {
                repaired = true;
                diagnostic(
                    &mut diagnostics,
                    "Metode instalasi tidak valid; memakai resource_mount.",
                );
            }
        }
    }

    if object.contains_key("launcherVisualMode") || object.contains_key("perf") {
        repaired = true;
        diagnostic(
            &mut diagnostics,
            "Pengaturan performa lama dihapus; launcher memakai mode Penuh.",
        );
    }

    if object.contains_key("autoCheckUpdate") {
        repaired = true;
        diagnostic(
            &mut diagnostics,
            "Pemeriksaan update otomatis selalu aktif; pengaturan lama dihapus.",
        );
    }

    if object.contains_key("diagnosticsUploadEnabled") || object.contains_key("telemetryEnabled") {
        repaired = true;
        diagnostic(
            &mut diagnostics,
            "Pengaturan upload diagnostics dan telemetry dihapus karena server belum tersedia.",
        );
    }

    read_bool(
        object,
        "dx11",
        &mut settings.dx11,
        &mut diagnostics,
        &mut repaired,
    );
    read_bool(
        object,
        "csharpEnvironment",
        &mut settings.csharp_environment,
        &mut diagnostics,
        &mut repaired,
    );
    let mut uid_mode_valid = false;
    if let Some(value) = object.get("uidMode") {
        match value.as_str() {
            Some(mode) if matches!(mode, "default" | "custom") => {
                settings.uid_mode = mode.to_string();
                uid_mode_valid = true;
            }
            _ => {
                repaired = true;
                diagnostic(&mut diagnostics, "Mode UID tidak valid; memakai DEFAULT.");
            }
        }
    }
    if let Some(value) = object.get("uidText") {
        match value.as_str() {
            Some(text) if validate_uid_text(text).is_ok() => {
                settings.uid_text = text.to_string();
            }
            _ => {
                repaired = true;
                diagnostic(
                    &mut diagnostics,
                    "Teks UID custom tidak valid; memakai teks kosong.",
                );
            }
        }
    }
    if let Some(value) = object.get("hideUid") {
        match value.as_bool() {
            Some(hide_uid) => {
                if !uid_mode_valid {
                    settings.uid_mode = if hide_uid { "custom" } else { "default" }.to_string();
                }
            }
            None => {
                diagnostic(
                    &mut diagnostics,
                    "Field settings hideUid tidak valid; memakai default.",
                );
            }
        }
        repaired = true;
        diagnostic(
            &mut diagnostics,
            "Pengaturan hideUid lama dimigrasikan ke mode UID.",
        );
    }
    read_bool(
        object,
        "bgmEnabled",
        &mut settings.bgm_enabled,
        &mut diagnostics,
        &mut repaired,
    );
    if let Some(value) = object.get("bgmVolume") {
        match value.as_f64() {
            Some(value) if value.is_finite() => {
                let clamped = value.clamp(0.0, 1.0);
                if clamped != value {
                    repaired = true;
                    diagnostic(
                        &mut diagnostics,
                        "Volume BGM berada di luar 0..1 dan telah dibatasi.",
                    );
                }
                settings.bgm_volume = clamped;
            }
            _ => {
                repaired = true;
                diagnostic(
                    &mut diagnostics,
                    "Field settings bgmVolume tidak valid; memakai default.",
                );
            }
        }
    }

    SettingsLoadResult {
        settings,
        repaired,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_customization_settings_migrate_legacy_values_and_validate_text() {
        let defaults = normalize_settings_json(r#"{}"#);
        assert!(!defaults.settings.csharp_environment);
        assert_eq!(defaults.settings.uid_mode, "default");
        assert!(defaults.settings.uid_text.is_empty());

        let enabled = normalize_settings_json(r#"{"csharpEnvironment":true,"hideUid":true}"#);
        assert!(enabled.settings.csharp_environment);
        assert_eq!(enabled.settings.uid_mode, "custom");
        assert!(enabled.settings.uid_text.is_empty());
        assert!(enabled.repaired);

        let custom =
            normalize_settings_json(r#"{"uidMode":"custom","uidText":"Halo Nozomi ✦ 2026!"}"#);
        assert_eq!(custom.settings.uid_mode, "custom");
        assert_eq!(custom.settings.uid_text, "Halo Nozomi ✦ 2026!");
        assert!(!custom.repaired);

        let invalid = normalize_settings_json(
            r#"{"csharpEnvironment":"yes","uidMode":"custom","uidText":"bad\ntext"}"#,
        );
        assert!(invalid.repaired);
        assert!(!invalid.settings.csharp_environment);
        assert_eq!(invalid.settings.uid_mode, "custom");
        assert!(invalid.settings.uid_text.is_empty());
    }

    #[cfg(windows)]
    fn is_supported_windows_canonical_path(path: &str) -> bool {
        let bytes = path.as_bytes();
        let drive_path = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/');
        let extended_drive_path = bytes.len() >= 7
            && path.starts_with("\\\\?\\")
            && bytes[4].is_ascii_alphabetic()
            && bytes[5] == b':'
            && matches!(bytes[6], b'\\' | b'/');
        let legacy_unc_path = path.starts_with("\\\\") && !path.starts_with("\\\\?\\");
        let extended_unc_path = path.starts_with("\\\\?\\UNC\\");
        drive_path || extended_drive_path || legacy_unc_path || extended_unc_path
    }

    #[cfg(windows)]
    #[test]
    fn supported_windows_canonical_path_forms_include_drive_and_unc() {
        for path in [
            "C:\\Games\\Wuwa",
            "\\\\?\\C:\\Games\\Wuwa",
            "\\\\server\\share\\Wuwa",
            "\\\\?\\UNC\\server\\share\\Wuwa",
        ] {
            assert!(
                is_supported_windows_canonical_path(path),
                "unsupported Windows path identity: {path}"
            );
        }
    }

    #[test]
    fn legacy_game_path_is_canonicalized_when_settings_loads() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("game");
        let alias_parent = temp.path().join("alias");
        let executable = game.join(crate::engine::path::GAME_EXE_RELATIVE);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&alias_parent).unwrap();
        std::fs::write(&executable, b"mock game executable").unwrap();

        let legacy_path = alias_parent.join("..").join("game");
        let raw = serde_json::json!({
            "gamePath": legacy_path.to_string_lossy(),
            "installMethod": "loader"
        })
        .to_string();
        let result = normalize_settings_json(&raw);
        let canonical = std::fs::canonicalize(&game)
            .unwrap()
            .to_string_lossy()
            .to_string();

        assert_eq!(result.settings.game_path, canonical);
        #[cfg(windows)]
        assert!(
            is_supported_windows_canonical_path(&canonical),
            "unexpected canonical Windows path: {canonical}"
        );
        assert_eq!(result.settings.install_method, InstallMethod::Loader);
        assert!(result.repaired);
        assert!(result
            .diagnostics
            .iter()
            .any(|message| message.contains("Path game")));
    }
}
