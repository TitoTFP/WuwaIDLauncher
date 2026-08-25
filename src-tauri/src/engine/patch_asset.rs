use crate::engine::{downloader, installer};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

pub const ARCHIVE_FILE_NAME: &str = "WuwaID.zip";
pub const NORMAL_PAK_FILE_NAME: &str = "pakchunk0-ID-WindowsNoEditor_1000_P.pak";
pub const HIDE_UID_PAK_FILE_NAME: &str = "pakchunk0-ID-WindowsNoEditor-HideUID_1000_P.pak";
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PAK_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchVariant {
    Normal,
    HideUid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAsset {
    Archive,
    LegacyPak,
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

    pub const fn pak_file_name(self) -> &'static str {
        match self {
            Self::Normal => NORMAL_PAK_FILE_NAME,
            Self::HideUid => HIDE_UID_PAK_FILE_NAME,
        }
    }
}

pub fn installed_variant_matches(installed: Option<&str>, desired: PatchVariant) -> bool {
    installed.unwrap_or("normal") == desired.as_str()
}

pub fn select_release_asset(
    checksums: &HashMap<String, String>,
    variant: PatchVariant,
) -> Result<ReleaseAsset, String> {
    if checksums.contains_key(ARCHIVE_FILE_NAME) {
        if !checksums.contains_key(variant.pak_file_name()) {
            return Err(format!(
                "checksum_missing: {} tidak ada dalam SHA256sums.txt",
                variant.pak_file_name()
            ));
        }
        return Ok(ReleaseAsset::Archive);
    }
    if variant == PatchVariant::HideUid {
        return Err(
            "hide_uid_unsupported: rilis lama belum menyediakan varian Hide UID".to_string(),
        );
    }
    if checksums.contains_key(NORMAL_PAK_FILE_NAME) {
        Ok(ReleaseAsset::LegacyPak)
    } else {
        Err("checksum_missing: asset patch tidak ada dalam SHA256sums.txt".to_string())
    }
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    path.with_extension(format!("tmp-{}-{stamp}", std::process::id()))
}

pub fn extract_selected_patch(
    archive_path: &Path,
    variant: PatchVariant,
    expected_hash: &str,
    destination: &Path,
) -> Result<(), String> {
    let archive_size = fs::metadata(archive_path)
        .map_err(|error| format!("archive_metadata_failed: {error}"))?
        .len();
    if archive_size == 0 || archive_size > MAX_ARCHIVE_BYTES {
        return Err(format!(
            "archive_size_invalid: WuwaID.zip harus berukuran 1..{MAX_ARCHIVE_BYTES} bytes"
        ));
    }
    if expected_hash.len() != 64 || !expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            "archive_member_checksum_missing: checksum PAK pilihan tidak valid".to_string(),
        );
    }

    if destination.is_file()
        && downloader::verify_sha256(destination, expected_hash).unwrap_or(false)
        && installer::validate_pak_file(destination).unwrap_or(false)
    {
        return Ok(());
    }

    let file =
        fs::File::open(archive_path).map_err(|error| format!("archive_open_failed: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("archive_parse_failed: {error}"))?;
    if archive.len() != 2 {
        return Err("archive_members_invalid: WuwaID.zip wajib berisi tepat dua PAK".to_string());
    }

    let allowed = [NORMAL_PAK_FILE_NAME, HIDE_UID_PAK_FILE_NAME];
    let mut names = HashSet::with_capacity(2);
    for index in 0..archive.len() {
        let member = archive
            .by_index(index)
            .map_err(|error| format!("archive_member_failed: {error}"))?;
        let name = member.name();
        if member.is_dir()
            || member.enclosed_name().as_deref() != Some(Path::new(name))
            || !allowed.contains(&name)
        {
            return Err(format!("archive_member_invalid: {name}"));
        }
        if !names.insert(name.to_string()) {
            return Err(format!("archive_member_duplicate: {name}"));
        }
        if member.size() == 0 || member.size() > MAX_PAK_BYTES {
            return Err(format!("archive_member_size_invalid: {name}"));
        }
    }
    if !allowed.iter().all(|name| names.contains(*name)) {
        return Err("archive_members_invalid: varian PAK tidak lengkap".to_string());
    }

    let selected_name = variant.pak_file_name();
    let mut member = archive
        .by_name(selected_name)
        .map_err(|error| format!("archive_selected_member_missing: {error}"))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("archive_cache_parent_failed: {error}"))?;
    }
    let temp = unique_temp_path(destination);
    let result = (|| -> Result<(), String> {
        let mut output = fs::File::create(&temp)
            .map_err(|error| format!("archive_extract_create_failed: {error}"))?;
        let mut hasher = Sha256::new();
        let mut written = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = member
                .read(&mut buffer)
                .map_err(|error| format!("archive_extract_read_failed: {error}"))?;
            if count == 0 {
                break;
            }
            written = written.saturating_add(count as u64);
            if written > MAX_PAK_BYTES || written > member.size() {
                return Err("archive_extract_size_invalid: PAK melebihi batas".to_string());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("archive_extract_write_failed: {error}"))?;
            hasher.update(&buffer[..count]);
        }
        output
            .sync_all()
            .map_err(|error| format!("archive_extract_sync_failed: {error}"))?;
        if written != member.size() {
            return Err("archive_extract_size_mismatch: ukuran PAK tidak cocok".to_string());
        }
        let actual_hash = hex::encode(hasher.finalize());
        if !actual_hash.eq_ignore_ascii_case(expected_hash) {
            return Err("archive_member_checksum_mismatch: integritas PAK gagal".to_string());
        }
        if !installer::validate_pak_file(&temp)? {
            return Err("archive_member_pak_invalid: struktur PAK tidak valid".to_string());
        }
        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| format!("archive_cache_replace_failed: {error}"))?;
        }
        fs::rename(&temp, destination)
            .map_err(|error| format!("archive_cache_activate_failed: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::pak;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    fn valid_pak(label: &str) -> Vec<u8> {
        pak::pack(
            "../../../",
            0,
            &[(format!("{label}.db"), label.as_bytes().to_vec())],
        )
        .unwrap()
    }

    fn write_archive(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn release_selection_prefers_archive_and_never_downgrades_hide_uid() {
        assert!(installed_variant_matches(None, PatchVariant::Normal));
        assert!(!installed_variant_matches(None, PatchVariant::HideUid));
        assert!(installed_variant_matches(
            Some("hide_uid"),
            PatchVariant::HideUid
        ));

        let mut checksums = HashMap::new();
        checksums.insert(NORMAL_PAK_FILE_NAME.to_string(), "a".repeat(64));
        assert_eq!(
            select_release_asset(&checksums, PatchVariant::Normal).unwrap(),
            ReleaseAsset::LegacyPak
        );
        assert!(select_release_asset(&checksums, PatchVariant::HideUid)
            .unwrap_err()
            .contains("hide_uid_unsupported"));

        checksums.insert(ARCHIVE_FILE_NAME.to_string(), "b".repeat(64));
        checksums.insert(HIDE_UID_PAK_FILE_NAME.to_string(), "c".repeat(64));
        assert_eq!(
            select_release_asset(&checksums, PatchVariant::HideUid).unwrap(),
            ReleaseAsset::Archive
        );
    }

    #[test]
    fn extracts_only_selected_exact_member_and_checks_hash() {
        let temp = tempfile::tempdir().unwrap();
        let normal = valid_pak("normal");
        let hidden = valid_pak("hidden");
        let archive = temp.path().join(ARCHIVE_FILE_NAME);
        write_archive(
            &archive,
            &[
                (NORMAL_PAK_FILE_NAME, normal),
                (HIDE_UID_PAK_FILE_NAME, hidden.clone()),
            ],
        );
        let expected = hex::encode(Sha256::digest(&hidden));
        let output = temp.path().join("selected.pak");

        extract_selected_patch(&archive, PatchVariant::HideUid, &expected, &output).unwrap();
        assert_eq!(fs::read(output).unwrap(), hidden);
    }

    #[test]
    fn rejects_unexpected_members_and_checksum_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let pak = valid_pak("normal");
        let output = temp.path().join("selected.pak");
        let expected = hex::encode(Sha256::digest(&pak));

        let unexpected = temp.path().join("unexpected.zip");
        write_archive(
            &unexpected,
            &[
                (NORMAL_PAK_FILE_NAME, pak.clone()),
                ("../evil.pak", pak.clone()),
            ],
        );
        assert!(
            extract_selected_patch(&unexpected, PatchVariant::Normal, &expected, &output)
                .unwrap_err()
                .contains("archive_member_invalid")
        );

        let valid = temp.path().join("valid.zip");
        write_archive(
            &valid,
            &[
                (NORMAL_PAK_FILE_NAME, pak.clone()),
                (HIDE_UID_PAK_FILE_NAME, pak),
            ],
        );
        assert!(
            extract_selected_patch(&valid, PatchVariant::Normal, &"0".repeat(64), &output)
                .unwrap_err()
                .contains("checksum_mismatch")
        );
    }
}
