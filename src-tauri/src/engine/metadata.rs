use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const GAMES_KEY: &str = "games";
const SCHEMA_KEY: &str = "_schemaVersion";
const CURRENT_SCHEMA_VERSION: u64 = 3;

static METADATA_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_metadata() -> std::sync::MutexGuard<'static, ()> {
    METADATA_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn metadata_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}

pub fn game_key(game_path: &Path) -> Result<String, String> {
    let canonical = fs::canonicalize(game_path)
        .map_err(|error| metadata_error("metadata_game_path_failed", error))?;
    let mut key = canonical.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        key = key.to_ascii_lowercase();
    }
    Ok(key)
}

fn read_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let content =
        fs::read_to_string(path).map_err(|error| metadata_error("metadata_read_failed", error))?;
    serde_json::from_str::<Value>(&content)
        .map_err(|error| metadata_error("metadata_parse_failed", error))?
        .as_object()
        .cloned()
        .ok_or_else(|| "metadata_parse_failed: versions.json bukan object".to_string())
}

fn game_entries(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .get(GAMES_KEY)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn migrate_legacy_entry(object: &Map<String, Value>, key: &str) -> Option<Map<String, Value>> {
    let mut entry = Map::new();
    for field in [
        "_vhVersion",
        "_installMethod",
        "_loaderSha256",
        "_patchVariant",
    ] {
        if let Some(value) = object.get(field) {
            entry.insert(field.to_string(), value.clone());
        }
    }
    (!entry.is_empty())
        .then_some(entry)
        .filter(|_| !key.is_empty())
}

fn insert_game_entry(object: &mut Map<String, Value>, key: &str, entry: Map<String, Value>) {
    let games = object
        .entry(GAMES_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !games.is_object() {
        *games = Value::Object(Map::new());
    }
    games
        .as_object_mut()
        .expect("games object was normalized")
        .insert(key.to_string(), Value::Object(entry));
    object.insert(
        SCHEMA_KEY.to_string(),
        Value::Number(CURRENT_SCHEMA_VERSION.into()),
    );
}

fn current_game_entry(object: &Map<String, Value>, key: &str) -> Option<Map<String, Value>> {
    if object.contains_key(GAMES_KEY) {
        return game_entries(object)
            .get(key)
            .and_then(Value::as_object)
            .cloned();
    }
    migrate_legacy_entry(object, key)
}

pub fn read_game_field(
    path: &Path,
    game_path: &Path,
    field: &str,
) -> Result<Option<String>, String> {
    let _guard = lock_metadata();
    let key = game_key(game_path)?;
    let object = read_object(path)?;
    Ok(current_game_entry(&object, &key)
        .and_then(|entry| entry.get(field).and_then(Value::as_str).map(str::to_string))
        .filter(|value| !value.trim().is_empty()))
}

fn unique_temp_path(path: &Path, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("versions.json");
    path.with_file_name(format!(
        ".{file_name}.{suffix}-{}-{stamp}",
        std::process::id()
    ))
}

pub fn write_object_atomic(path: &Path, object: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| metadata_error("metadata_parent_failed", error))?;
    }
    let temp = unique_temp_path(path, "tmp");
    let serialized = serde_json::to_vec(object)
        .map_err(|error| metadata_error("metadata_encode_failed", error))?;
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp)
            .map_err(|error| metadata_error("metadata_temp_create_failed", error))?;
        use std::io::Write;
        file.write_all(&serialized)
            .map_err(|error| metadata_error("metadata_temp_write_failed", error))?;
        file.sync_all()
            .map_err(|error| metadata_error("metadata_temp_sync_failed", error))?;

        #[cfg(windows)]
        {
            if path.exists() {
                let backup = unique_temp_path(path, "backup");
                fs::copy(path, &backup)
                    .map_err(|error| metadata_error("metadata_backup_failed", error))?;
                if let Err(error) = replace_file_windows(&temp, path) {
                    let restore = replace_file_windows(&backup, path);
                    if let Err(restore_error) = restore {
                        return Err(format!(
                            "metadata_replace_failed: {error}; metadata_restore_failed: {restore_error}"
                        ));
                    }
                    return Err(error);
                }
                let _ = fs::remove_file(backup);
            } else {
                replace_file_windows(&temp, path)
                    .map_err(|error| metadata_error("metadata_activate_failed", error))?;
            }
        }
        #[cfg(not(windows))]
        {
            fs::rename(&temp, path)
                .map_err(|error| metadata_error("metadata_activate_failed", error))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(windows)]
fn replace_file_windows(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| format!("metadata_replace_windows_failed: {error}"))
    }
}

fn update_object<F>(path: &Path, update: F) -> Result<(), String>
where
    F: FnOnce(&mut Map<String, Value>) -> Result<(), String>,
{
    let _guard = lock_metadata();
    let mut object = read_object(path)?;
    update(&mut object)?;
    write_object_atomic(path, &object)
}

pub fn update_installation(
    path: &Path,
    game_path: &Path,
    version: Option<&str>,
    install_method: &str,
    loader_hash: Option<&str>,
) -> Result<(), String> {
    update_installation_with_variant(path, game_path, version, install_method, loader_hash, None)
}

pub fn update_installation_with_variant(
    path: &Path,
    game_path: &Path,
    version: Option<&str>,
    install_method: &str,
    loader_hash: Option<&str>,
    patch_variant: Option<&str>,
) -> Result<(), String> {
    let key = game_key(game_path)?;
    update_object(path, |object| {
        let mut entry = current_game_entry(object, &key).unwrap_or_default();
        if let Some(version) = version.filter(|value| {
            let value = value.trim();
            !value.is_empty() && !value.eq_ignore_ascii_case("unknown")
        }) {
            entry.insert(
                "_vhVersion".to_string(),
                Value::String(version.trim().to_string()),
            );
            object.insert(
                "_vhVersion".to_string(),
                Value::String(version.trim().to_string()),
            );
        }
        entry.insert(
            "_installMethod".to_string(),
            Value::String(install_method.to_string()),
        );
        object.insert(
            "_installMethod".to_string(),
            Value::String(install_method.to_string()),
        );
        let variant = patch_variant
            .map(str::trim)
            .filter(|value| matches!(*value, "normal" | "hide_uid"))
            .or_else(|| {
                entry
                    .get("_patchVariant")
                    .and_then(Value::as_str)
                    .filter(|value| matches!(*value, "normal" | "hide_uid"))
            })
            .unwrap_or("normal");
        entry.insert(
            "_patchVariant".to_string(),
            Value::String(variant.to_string()),
        );
        match loader_hash.filter(|hash| !hash.trim().is_empty()) {
            Some(hash) => {
                entry.insert(
                    "_loaderSha256".to_string(),
                    Value::String(hash.trim().to_ascii_lowercase()),
                );
            }
            None => {
                entry.remove("_loaderSha256");
            }
        }
        insert_game_entry(object, &key, entry);
        Ok(())
    })
}

pub fn update_cached_release_notes(path: &Path, notes: Value) -> Result<(), String> {
    update_object(path, |object| {
        object.insert("_cachedReleaseNotes".to_string(), notes);
        Ok(())
    })
}

pub fn remove_game(path: &Path, game_path: &Path) -> Result<(), String> {
    let _guard = lock_metadata();
    let key = game_key(game_path)?;
    if !path.exists() {
        return Ok(());
    }
    let mut object = read_object(path)?;
    if let Some(games) = object.get_mut(GAMES_KEY).and_then(Value::as_object_mut) {
        games.remove(&key);
        if games.is_empty() {
            object.remove(GAMES_KEY);
        }
    }
    // Legacy mirrors are only a compatibility view.  They must not be used to
    // identify another game's installation after the keyed entry is removed.
    object.remove("_vhVersion");
    object.remove("_installMethod");
    object.remove("_patchVariant");
    if object
        .keys()
        .all(|key| key == "_cachedReleaseNotes" || key == SCHEMA_KEY)
    {
        object.remove(SCHEMA_KEY);
    }
    if object.is_empty() {
        fs::remove_file(path).map_err(|error| metadata_error("metadata_remove_failed", error))
    } else {
        write_object_atomic(path, &object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_metadata_migrates_legacy_values_and_preserves_release_notes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions.json");
        fs::write(
            &path,
            r#"{"_vhVersion":"v3.0.0","_installMethod":"loader","_cachedReleaseNotes":{"tag":"v3.0.0"}}"#,
        )
        .unwrap();
        let game = temp.path().join("game");
        fs::create_dir_all(&game).unwrap();

        update_installation(&path, &game, None, "resource_mount", None).unwrap();
        let object: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let key = game_key(&game).unwrap();
        assert_eq!(object["games"][key.clone()]["_vhVersion"], "v3.0.0");
        assert_eq!(object["_cachedReleaseNotes"]["tag"], "v3.0.0");
        assert_eq!(
            object["games"][key.clone()]["_installMethod"],
            "resource_mount"
        );
        assert_eq!(object["games"][key]["_patchVariant"], "normal");
    }

    #[test]
    fn installation_variant_is_persisted_and_preserved_by_method_updates() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions.json");
        let game = temp.path().join("game");
        fs::create_dir_all(&game).unwrap();

        update_installation_with_variant(
            &path,
            &game,
            Some("v1"),
            "resource_mount",
            None,
            Some("hide_uid"),
        )
        .unwrap();
        update_installation(&path, &game, None, "loader", None).unwrap();

        assert_eq!(
            read_game_field(&path, &game, "_patchVariant").unwrap(),
            Some("hide_uid".to_string())
        );
    }

    #[test]
    fn unknown_version_does_not_replace_known_version() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions.json");
        let game = temp.path().join("game");
        fs::create_dir_all(&game).unwrap();
        update_installation(
            &path,
            &game,
            Some("v3.0.0"),
            "loader",
            Some(&"a".repeat(64)),
        )
        .unwrap();
        update_installation(&path, &game, Some("unknown"), "loader", None).unwrap();
        assert_eq!(
            read_game_field(&path, &game, "_vhVersion").unwrap(),
            Some("v3.0.0".to_string())
        );
    }

    #[test]
    fn legacy_metadata_is_not_reused_for_a_second_game_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions.json");
        fs::write(
            &path,
            r#"{"_vhVersion":"v3.0.0","_installMethod":"loader"}"#,
        )
        .unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        update_installation(&path, &first, None, "loader", None).unwrap();

        assert_eq!(
            read_game_field(&path, &first, "_vhVersion").unwrap(),
            Some("v3.0.0".to_string())
        );
        assert_eq!(read_game_field(&path, &second, "_vhVersion").unwrap(), None);
    }

    #[test]
    fn removing_one_game_keeps_other_game_and_cached_notes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("versions.json");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        update_installation(&path, &first, Some("v1"), "loader", None).unwrap();
        update_installation(&path, &second, Some("v2"), "loader", None).unwrap();
        update_cached_release_notes(&path, serde_json::json!({"tag":"v2"})).unwrap();
        remove_game(&path, &first).unwrap();
        assert_eq!(
            read_game_field(&path, &second, "_vhVersion").unwrap(),
            Some("v2".to_string())
        );
        assert!(
            serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()
                ["_cachedReleaseNotes"]
                .is_object()
        );
    }
}
