use crate::engine::{downloader, installer, repak};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const NORMAL_PAK_FILE_NAME: &str = "pakchunk0-ID-WindowsNoEditor_1000_P.pak";
pub const HIDE_UID_PATCH_VERSION: &str = "hide_uid_v1";
const UID_DATABASE_RELATIVE_PATH: &str = "Client/Content/Aki/ConfigDB/en/lang_multi_text.db";
const UID_TABLE_NAME: &str = "MultiText";
const UID_TARGET_IDS: [&str; 2] = ["Text_FriendMyUid_Text", "Text_UserId_Text"];
const MAX_PAK_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchVariant {
    Normal,
    HideUid,
}

impl PatchVariant {
    pub const fn from_hide_uid(hide_uid: bool) -> Self {
        if hide_uid {
            Self::HideUid
        } else {
            Self::Normal
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::HideUid => "hide_uid",
        }
    }
}

pub fn installed_variant_matches(installed: Option<&str>, desired: PatchVariant) -> bool {
    installed.unwrap_or("normal") == desired.as_str()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn unique_path(path: &Path, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    path.with_file_name(format!(
        ".{}.{}-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("asset"),
        suffix,
        std::process::id(),
        stamp
    ))
}

fn unique_work_directory(cache_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(cache_dir)
        .map_err(|error| format!("hide_uid_cache_parent_failed: {error}"))?;
    let work = unique_path(cache_dir, "work");
    fs::create_dir(&work).map_err(|error| format!("hide_uid_work_create_failed: {error}"))?;
    Ok(work)
}

fn valid_patch_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn derived_pak_path_with_version(
    cache_dir: &Path,
    source_hash: &str,
    patch_version: &str,
) -> PathBuf {
    cache_dir.join(format!(
        "{NORMAL_PAK_FILE_NAME}.{patch_version}.{source_hash}.pak"
    ))
}

pub fn derived_pak_path(cache_dir: &Path, source_hash: &str) -> PathBuf {
    derived_pak_path_with_version(cache_dir, source_hash, HIDE_UID_PATCH_VERSION)
}

fn derived_hash_path(pak_path: &Path) -> PathBuf {
    pak_path.with_extension("sha256")
}

fn cached_derived_pak_is_valid(pak_path: &Path) -> bool {
    let hash_path = derived_hash_path(pak_path);
    let Ok(expected) = fs::read_to_string(hash_path) else {
        return false;
    };
    let expected = expected.trim();
    valid_sha256(expected)
        && pak_path.is_file()
        && downloader::verify_sha256(pak_path, expected).unwrap_or(false)
        && installer::validate_pak_file(pak_path).unwrap_or(false)
}

fn write_derived_hash(pak_path: &Path, hash: &str) -> Result<(), String> {
    let marker = derived_hash_path(pak_path);
    let temporary = unique_path(&marker, "tmp");
    fs::write(&temporary, format!("{hash}\n"))
        .map_err(|error| format!("hide_uid_hash_write_failed: {error}"))?;
    if marker.exists() {
        fs::remove_file(&marker)
            .map_err(|error| format!("hide_uid_hash_replace_failed: {error}"))?;
    }
    fs::rename(&temporary, &marker)
        .map_err(|error| format!("hide_uid_hash_activate_failed: {error}"))
}

fn patch_uid_database(database: &Path) -> Result<(), String> {
    if !database.is_file() {
        return Err(format!("hide_uid_database_missing: {}", database.display()));
    }

    let mut connection = Connection::open(database)
        .map_err(|error| format!("hide_uid_database_open_failed: {error}"))?;
    connection
        .execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")
        .map_err(|error| format!("hide_uid_database_pragmas_failed: {error}"))?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("hide_uid_database_transaction_failed: {error}"))?;
    for id in UID_TARGET_IDS {
        let count: i64 = transaction
            .query_row(
                &format!("SELECT COUNT(*) FROM {UID_TABLE_NAME} WHERE Id = ?1"),
                params![id],
                |row| row.get(0),
            )
            .map_err(|error| format!("hide_uid_database_query_failed:{id}: {error}"))?;
        if count != 1 {
            return Err(format!(
                "hide_uid_target_count_invalid: {id} ditemukan {count} kali, diharapkan tepat 1"
            ));
        }

        let changed = transaction
            .execute(
                &format!("UPDATE {UID_TABLE_NAME} SET Content = '' WHERE Id = ?1"),
                params![id],
            )
            .map_err(|error| format!("hide_uid_database_update_failed:{id}: {error}"))?;
        if changed != 1 {
            return Err(format!(
                "hide_uid_target_update_invalid: {id} mengubah {changed} baris"
            ));
        }
    }
    transaction
        .commit()
        .map_err(|error| format!("hide_uid_database_commit_failed: {error}"))?;
    drop(connection);

    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", database.to_string_lossy(), suffix));
        if sidecar.exists() {
            fs::remove_file(&sidecar)
                .map_err(|error| format!("hide_uid_database_sidecar_failed: {error}"))?;
        }
    }

    let verification = Connection::open(database)
        .map_err(|error| format!("hide_uid_database_verify_open_failed: {error}"))?;
    for id in UID_TARGET_IDS {
        let content: String = verification
            .query_row(
                &format!("SELECT Content FROM {UID_TABLE_NAME} WHERE Id = ?1"),
                params![id],
                |row| row.get(0),
            )
            .map_err(|error| format!("hide_uid_database_verify_query_failed:{id}: {error}"))?;
        if !content.is_empty() {
            return Err(format!(
                "hide_uid_database_verify_failed: {id} tidak kosong"
            ));
        }
    }

    Ok(())
}

pub fn prepare_hide_uid_pak(
    source_pak: &Path,
    source_hash: &str,
    cache_dir: &Path,
) -> Result<PathBuf, String> {
    prepare_hide_uid_pak_with_version(source_pak, source_hash, cache_dir, HIDE_UID_PATCH_VERSION)
}

pub(crate) fn prepare_hide_uid_pak_with_version(
    source_pak: &Path,
    source_hash: &str,
    cache_dir: &Path,
    patch_version: &str,
) -> Result<PathBuf, String> {
    if !valid_patch_version(patch_version) {
        return Err("hide_uid_patch_version_invalid: versi patch tidak valid".to_string());
    }
    if !valid_sha256(source_hash) {
        return Err(
            "hide_uid_source_checksum_invalid: checksum PAK normal tidak valid".to_string(),
        );
    }
    if !source_pak.is_file() || !downloader::verify_sha256(source_pak, source_hash).unwrap_or(false)
    {
        return Err(
            "hide_uid_source_checksum_mismatch: PAK normal tidak cocok dengan manifest".to_string(),
        );
    }

    let destination = derived_pak_path_with_version(cache_dir, source_hash, patch_version);
    if cached_derived_pak_is_valid(&destination) {
        return Ok(destination);
    }

    let work = unique_work_directory(cache_dir)?;
    let result = (|| -> Result<PathBuf, String> {
        let unpacked = work.join("unpacked");
        let temporary = work.join("hide_uid.pak");
        repak::unpack_v12(source_pak, &unpacked)?;
        patch_uid_database(&unpacked.join(UID_DATABASE_RELATIVE_PATH))?;
        repak::pack_v12(&unpacked, &temporary)?;

        let size = fs::metadata(&temporary)
            .map_err(|error| format!("hide_uid_output_metadata_failed: {error}"))?
            .len();
        if size == 0 || size > MAX_PAK_BYTES {
            return Err("hide_uid_output_size_invalid: hasil PAK di luar batas".to_string());
        }
        if !installer::validate_pak_file(&temporary)? {
            return Err("hide_uid_output_pak_invalid: struktur PAK hasil tidak valid".to_string());
        }
        let derived_hash = downloader::compute_sha256(&temporary)
            .map_err(|error| format!("hide_uid_output_hash_failed: {error}"))?;
        if !valid_sha256(&derived_hash) {
            return Err("hide_uid_output_hash_invalid: hash hasil PAK tidak valid".to_string());
        }

        if destination.exists() {
            fs::remove_file(&destination)
                .map_err(|error| format!("hide_uid_output_replace_failed: {error}"))?;
        }
        fs::create_dir_all(cache_dir)
            .map_err(|error| format!("hide_uid_cache_create_failed: {error}"))?;
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("hide_uid_output_activate_failed: {error}"))?;
        write_derived_hash(&destination, &derived_hash)?;
        Ok(destination)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::repak as repak_tools;
    use rusqlite::Connection;

    fn create_database(path: &Path, duplicate_friend_id: bool) {
        create_database_with_unrelated(path, duplicate_friend_id, "keep me");
    }

    fn create_database_with_unrelated(
        path: &Path,
        duplicate_friend_id: bool,
        unrelated_content: &str,
    ) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE MultiText (Id TEXT, Content TEXT, RedirectDbIndex INTEGER);
                 INSERT INTO MultiText VALUES ('Text_FriendMyUid_Text', 'ID Pengguna: {0}', 0);
                 INSERT INTO MultiText VALUES ('Text_UserId_Text', 'ID Pengguna: {0}', 0);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO MultiText VALUES ('Unrelated_Text', ?1, 0)",
                params![unrelated_content],
            )
            .unwrap();
        if duplicate_friend_id {
            connection
                .execute(
                    "INSERT INTO MultiText VALUES (?1, 'duplicate', 0)",
                    params![UID_TARGET_IDS[0]],
                )
                .unwrap();
        }
    }

    fn create_source_pak(path: &Path) {
        create_source_pak_with_unrelated(path, "keep me");
    }

    fn create_source_pak_with_unrelated(path: &Path, unrelated_content: &str) {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join(UID_DATABASE_RELATIVE_PATH);
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        create_database_with_unrelated(&database, false, unrelated_content);
        repak_tools::pack_v12(temp.path(), path).unwrap();
    }

    #[test]
    fn defaults_and_variant_matching_are_local_only() {
        assert!(installed_variant_matches(None, PatchVariant::Normal));
        assert!(!installed_variant_matches(None, PatchVariant::HideUid));
        assert!(installed_variant_matches(
            Some("hide_uid"),
            PatchVariant::HideUid
        ));
        assert_eq!(PatchVariant::HideUid.as_str(), "hide_uid");
    }

    #[test]
    fn patches_only_the_two_uid_records() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("lang_multi_text.db");
        create_database(&database, false);

        patch_uid_database(&database).unwrap();
        let connection = Connection::open(database).unwrap();
        for id in UID_TARGET_IDS {
            let content: String = connection
                .query_row(
                    "SELECT Content FROM MultiText WHERE Id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(content.is_empty());
        }
        let unrelated: String = connection
            .query_row(
                "SELECT Content FROM MultiText WHERE Id = 'Unrelated_Text'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unrelated, "keep me");
    }

    #[test]
    fn rejects_missing_and_ambiguous_uid_records() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing.db");
        let connection = Connection::open(&missing).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE MultiText (Id TEXT, Content TEXT, RedirectDbIndex INTEGER);
                 INSERT INTO MultiText VALUES ('Text_FriendMyUid_Text', 'x', 0);",
            )
            .unwrap();
        drop(connection);
        assert!(patch_uid_database(&missing)
            .unwrap_err()
            .contains("hide_uid_target_count_invalid"));

        let duplicate = temp.path().join("duplicate.db");
        create_database(&duplicate, true);
        assert!(patch_uid_database(&duplicate)
            .unwrap_err()
            .contains("hide_uid_target_count_invalid"));
    }

    #[test]
    fn rejects_checksum_mismatch_and_invalid_source_pak() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(NORMAL_PAK_FILE_NAME);
        fs::write(&source, b"not a pak").unwrap();
        let cache = temp.path().join("cache");
        let actual_hash = downloader::compute_sha256(&source).unwrap();

        assert!(prepare_hide_uid_pak(&source, &"0".repeat(64), &cache)
            .unwrap_err()
            .contains("hide_uid_source_checksum_mismatch"));
        assert!(prepare_hide_uid_pak(&source, &actual_hash, &cache)
            .unwrap_err()
            .contains("repak_read_v12"));
    }

    #[test]
    fn cache_path_changes_with_source_hash_and_patch_version() {
        let cache = Path::new("cache");
        let first = derived_pak_path(cache, &"a".repeat(64));
        let second = derived_pak_path(cache, &"b".repeat(64));
        assert_ne!(first, second);
        assert!(first.to_string_lossy().contains(HIDE_UID_PATCH_VERSION));
    }

    #[test]
    fn builds_and_reuses_a_verified_hide_uid_pak() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(NORMAL_PAK_FILE_NAME);
        let cache = temp.path().join("cache");
        create_source_pak(&source);
        let source_hash = downloader::compute_sha256(&source).unwrap();

        let generated = prepare_hide_uid_pak(&source, &source_hash, &cache).unwrap();
        assert!(installer::validate_pak_file(&generated).unwrap());
        let unpacked = temp.path().join("verified");
        repak_tools::unpack_v12(&generated, &unpacked).unwrap();
        let connection = Connection::open(unpacked.join(UID_DATABASE_RELATIVE_PATH)).unwrap();
        for id in UID_TARGET_IDS {
            let content: String = connection
                .query_row(
                    "SELECT Content FROM MultiText WHERE Id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(content.is_empty());
        }

        let second = prepare_hide_uid_pak(&source, &source_hash, &cache).unwrap();
        assert_eq!(generated, second);
        assert!(cached_derived_pak_is_valid(&second));

        create_source_pak_with_unrelated(&source, "changed source bytes");
        let changed_hash = downloader::compute_sha256(&source).unwrap();
        assert_ne!(source_hash, changed_hash);
        let regenerated = prepare_hide_uid_pak(&source, &changed_hash, &cache).unwrap();
        assert_ne!(generated, regenerated);
        assert!(cached_derived_pak_is_valid(&regenerated));

        let new_algorithm =
            prepare_hide_uid_pak_with_version(&source, &changed_hash, &cache, "hide_uid_v2")
                .unwrap();
        assert_ne!(regenerated, new_algorithm);
        assert!(installer::validate_pak_file(&new_algorithm).unwrap());
    }
}
