use super::method::InstallMethod;
use crate::engine::path::normalize_game_path;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub game_path: String,
    pub install_method: InstallMethod,
    pub dx11: bool,
    pub auto_check_update: bool,
    pub bgm_volume: f64,
    pub bgm_enabled: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            game_path: String::new(),
            install_method: InstallMethod::ResourceMount,
            dx11: false,
            auto_check_update: true,
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
        "autoCheckUpdate",
        &mut settings.auto_check_update,
        &mut diagnostics,
        &mut repaired,
    );
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
