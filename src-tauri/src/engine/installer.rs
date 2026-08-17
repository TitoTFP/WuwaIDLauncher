use crate::engine::{downloader, method::InstallMethod, pak, path, signature};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const PATCH_FOLDER_NAME: &str = "wuwaindonesia";
pub const PATCH_PAK_FILE_NAME: &str = "WuWaID_99_P.pak";
pub const PATCH_SIG_FILE_NAME: &str = "WuWaID_99_P.sig";
pub const MOUNT_FILE_NAME: &str = "wuwaindonesia.txt";
pub const OWNER_MARKER_FILE_NAME: &str = ".wuwaid-resource-mount";

#[derive(Debug, Clone)]
pub struct ResourceMountPlan {
    pub version_name: String,
    pub version_dir: PathBuf,
    pub mount_dir: PathBuf,
    pub pak_path: PathBuf,
    pub sig_path: PathBuf,
    pub mount_path: PathBuf,
    pub owner_marker_path: PathBuf,
}

pub fn get_resources_root(game_path: &Path) -> PathBuf {
    game_path.join("Client").join("Saved").join("Resources")
}

pub fn compute_sha1(file_path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(file_path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()).to_lowercase())
}

pub fn validate_mount_file(mount_path: &Path) -> Result<bool, String> {
    if !mount_path.exists() {
        return Ok(false);
    }
    let content =
        fs::read_to_string(mount_path).map_err(|e| format!("Gagal membaca mount file: {}", e))?;

    let lines: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() != 3 {
        return Ok(false);
    }

    Ok(lines[0] == "99"
        && lines[1] == "../Patch/wuwaindonesia/WuWaID_99_P.pak"
        && lines[2] == "../Patch/wuwaindonesia/WuWaID_99_P.sig")
}

/// Validate the footer and indexed data of a real Unreal PAK produced for the
/// launcher.  A non-empty file is not sufficient: the footer must contain the
/// expected magic/version and its SHA-1 must cover the index bytes.
pub fn validate_pak_file(pak_path: &Path) -> Result<bool, String> {
    let bytes = fs::read(pak_path).map_err(|e| format!("Gagal membaca PAK: {}", e))?;
    const FOOTER_SIZE: usize = 16 + 1 + 4 + 4 + 8 + 8 + 20 + (32 * 5);
    const MAGIC_OFFSET: usize = 16 + 1;
    const VERSION_OFFSET: usize = MAGIC_OFFSET + 4;
    const INDEX_OFFSET_OFFSET: usize = VERSION_OFFSET + 4;
    const INDEX_SIZE_OFFSET: usize = INDEX_OFFSET_OFFSET + 8;
    const INDEX_HASH_OFFSET: usize = INDEX_SIZE_OFFSET + 8;

    if bytes.len() < FOOTER_SIZE {
        return Ok(false);
    }
    let footer = bytes.len() - FOOTER_SIZE;
    let read_u32 = |offset: usize| -> u32 {
        u32::from_le_bytes(
            bytes[footer + offset..footer + offset + 4]
                .try_into()
                .unwrap(),
        )
    };
    let read_u64 = |offset: usize| -> u64 {
        u64::from_le_bytes(
            bytes[footer + offset..footer + offset + 8]
                .try_into()
                .unwrap(),
        )
    };

    if read_u32(MAGIC_OFFSET) != pak::PAK_MAGIC || read_u32(VERSION_OFFSET) != pak::WUWA_PAK_VERSION
    {
        return Ok(false);
    }

    let index_offset = read_u64(INDEX_OFFSET_OFFSET) as usize;
    let index_size = read_u64(INDEX_SIZE_OFFSET) as usize;
    if index_offset > footer || index_size > footer.saturating_sub(index_offset) {
        return Ok(false);
    }

    let expected_end = footer + INDEX_HASH_OFFSET + 20;
    if expected_end > bytes.len() {
        return Ok(false);
    }
    let expected = &bytes[footer + INDEX_HASH_OFFSET..expected_end];
    let mut hasher = Sha1::new();
    hasher.update(&bytes[index_offset..index_offset + index_size]);
    Ok(hasher.finalize().as_slice() == expected)
}

fn marker_hash(marker: &str) -> Option<&str> {
    marker
        .trim()
        .strip_prefix("wuwaid-managed-mod:sha1=")
        .filter(|hash| hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn validate_installed_resource_mount(plan: &ResourceMountPlan) -> Result<bool, String> {
    if !plan.pak_path.exists()
        || !plan.sig_path.exists()
        || !plan.owner_marker_path.exists()
        || !plan.mount_path.exists()
    {
        return Ok(false);
    }
    if !validate_pak_file(&plan.pak_path)? {
        return Ok(false);
    }
    if fs::metadata(&plan.sig_path)
        .map_err(|e| format!("Gagal membaca metadata signature: {}", e))?
        .len()
        == 0
    {
        return Ok(false);
    }

    let marker = fs::read_to_string(&plan.owner_marker_path)
        .map_err(|e| format!("Gagal membaca owner marker: {}", e))?;
    let Some(expected_sha1) = marker_hash(&marker) else {
        return Ok(false);
    };
    if compute_sha1(&plan.pak_path).map_err(|e| format!("Gagal menghitung SHA-1 PAK: {}", e))?
        != expected_sha1.to_ascii_lowercase()
    {
        return Ok(false);
    }

    validate_mount_file(&plan.mount_path)
}

pub fn loader_pak_path(game_path: &Path) -> PathBuf {
    signature::get_loader_pak_path(game_path)
}

pub fn loader_dll_path(game_path: &Path) -> PathBuf {
    signature::get_loader_dll_path(game_path)
}

pub fn loader_marker_path(game_path: &Path) -> PathBuf {
    signature::get_loader_marker_path(game_path)
}

pub fn signature_bypass_pak_path(game_path: &Path) -> PathBuf {
    signature::get_signature_bypass_pak_path(game_path)
}

pub fn signature_bypass_marker_path(game_path: &Path) -> PathBuf {
    signature::get_signature_bypass_marker_path(game_path)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn marker_value<'a>(marker: &'a str, prefix: &str, key: &str) -> Option<&'a str> {
    marker
        .trim()
        .strip_prefix(prefix)?
        .split(';')
        .find_map(|part| part.strip_prefix(key))
        .filter(|value| valid_sha256(value))
}

pub fn validate_installed_loader(game_path: &Path) -> Result<bool, String> {
    let pak_path = loader_pak_path(game_path);
    let dll_path = loader_dll_path(game_path);
    let marker_path = loader_marker_path(game_path);
    if !pak_path.is_file() || !dll_path.is_file() || !marker_path.is_file() {
        return Ok(false);
    }
    if !validate_pak_file(&pak_path)?
        || fs::metadata(&dll_path)
            .map_err(|e| format!("Gagal membaca metadata loader: {e}"))?
            .len()
            == 0
    {
        return Ok(false);
    }

    let marker = fs::read_to_string(&marker_path)
        .map_err(|e| format!("Gagal membaca marker loader: {e}"))?;
    let Some(pak_hash) = marker_value(&marker, "wuwaid-managed-loader:", "pak-sha256=") else {
        return Ok(false);
    };
    let Some(dll_hash) = marker_value(&marker, "wuwaid-managed-loader:", "loader-sha256=") else {
        return Ok(false);
    };
    let actual_pak_hash = downloader::compute_sha256(&pak_path)
        .map_err(|e| format!("Gagal menghitung hash PAK loader: {e}"))?;
    let actual_dll_hash = downloader::compute_sha256(&dll_path)
        .map_err(|e| format!("Gagal menghitung hash loader: {e}"))?;
    Ok(actual_pak_hash == pak_hash && actual_dll_hash == dll_hash)
}

pub fn validate_installed_signature_bypass(game_path: &Path) -> Result<bool, String> {
    let pak_path = signature_bypass_pak_path(game_path);
    let marker_path = signature_bypass_marker_path(game_path);
    if !pak_path.is_file() || !marker_path.is_file() || !validate_pak_file(&pak_path)? {
        return Ok(false);
    }
    let marker = fs::read_to_string(&marker_path)
        .map_err(|e| format!("Gagal membaca marker signature bypass: {e}"))?;
    let Some(expected_hash) = marker_value(
        &marker,
        "wuwaid-managed-signature-bypass:",
        "sha256=",
    ) else {
        return Ok(false);
    };
    let actual_hash = downloader::compute_sha256(&pak_path)
        .map_err(|e| format!("Gagal menghitung hash PAK signature bypass: {e}"))?;
    Ok(actual_hash == expected_hash)
}

#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
pub struct CleanupReport {
    pub removed: Vec<String>,
    pub preserved: Vec<String>,
    pub failures: Vec<String>,
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn existing_directory(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn check_directory_writable(path: &Path) -> Result<(), String> {
    let directory = existing_directory(path)
        .ok_or_else(|| format!("invalid_target: target parent tidak ditemukan: {:?}", path))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let probe = directory.join(format!(".wuwaid-write-probe-{}-{}", std::process::id(), stamp));
    fs::write(&probe, b"probe")
        .map_err(|error| format!("needs_admin: target tidak dapat ditulis: {error}"))?;
    fs::remove_file(&probe)
        .map_err(|error| format!("needs_admin: probe permission tidak dapat dibersihkan: {error}"))
}

fn method_target_directory(game_path: &Path, method: InstallMethod) -> Result<PathBuf, String> {
    match method {
        InstallMethod::ResourceMount => Ok(probe_resource_mount(game_path)?.mount_dir),
        InstallMethod::Loader => Ok(signature::get_loader_folder(game_path)),
        InstallMethod::SignatureBypass => Ok(path::get_pak_dir(game_path)),
    }
}

/// Validate and normalize the game path before any download, cleanup, or write.
/// The returned path is the only path that installation code should use.
pub fn validate_installation_preconditions(
    game_path: &str,
    method: InstallMethod,
) -> Result<PathBuf, String> {
    let normalized = path::normalize_game_path(game_path)
        .ok_or_else(|| "invalid_game_path: executable game tidak ditemukan".to_string())?;
    let target = method_target_directory(&normalized, method).map_err(|error| match method {
        InstallMethod::ResourceMount => format!("resource_not_ready: {error}"),
        _ => error,
    })?;
    check_directory_writable(&target)?;
    Ok(normalized)
}

fn known_artifact_paths(game_path: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        signature::get_sig_path(game_path),
        signature::get_sig_backup_path(game_path),
        signature::get_signature_bypass_pak_path(game_path),
        signature::get_signature_bypass_marker_path(game_path),
        signature::get_loader_pak_path(game_path),
        signature::get_loader_dll_path(game_path),
        signature::get_loader_marker_path(game_path),
    ];
    if let Ok(plan) = probe_resource_mount(game_path) {
        paths.extend([
            plan.pak_path,
            plan.sig_path,
            plan.mount_path,
            plan.owner_marker_path,
        ]);
    }
    paths
}

#[derive(Debug, Clone)]
struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

fn snapshot_files(game_path: &Path) -> Vec<FileSnapshot> {
    known_artifact_paths(game_path)
        .into_iter()
        .map(|path| FileSnapshot {
            contents: path.is_file().then(|| fs::read(&path).ok()).flatten(),
            path,
        })
        .collect()
}

fn restore_files(snapshot: &[FileSnapshot]) -> Result<(), String> {
    for item in snapshot {
        match &item.contents {
            Some(contents) => {
                if let Some(parent) = item.path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("rollback_parent_failed {:?}: {error}", parent)
                    })?;
                }
                if item.path.exists() && !item.path.is_file() {
                    return Err(format!("rollback_target_not_file: {:?}", item.path));
                }
                fs::write(&item.path, contents)
                    .map_err(|error| format!("rollback_write_failed {:?}: {error}", item.path))?;
            }
            None => {
                if item.path.is_file() {
                    fs::remove_file(&item.path).map_err(|error| {
                        format!("rollback_remove_failed {:?}: {error}", item.path)
                    })?;
                }
            }
        }
    }
    Ok(())
}

fn target_is_available(path: &Path) -> bool {
    !path.exists()
}

fn reject_foreign_targets(
    game_path: &Path,
    method: InstallMethod,
) -> Result<(), String> {
    let targets = match method {
        InstallMethod::ResourceMount => {
            let plan = probe_resource_mount(game_path)?;
            vec![plan.pak_path, plan.sig_path, plan.mount_path, plan.owner_marker_path]
        }
        InstallMethod::Loader => vec![
            signature::get_loader_pak_path(game_path),
            signature::get_loader_dll_path(game_path),
            signature::get_loader_marker_path(game_path),
        ],
        InstallMethod::SignatureBypass => vec![
            signature::get_signature_bypass_pak_path(game_path),
            signature::get_signature_bypass_marker_path(game_path),
        ],
    };

    let owned = match method {
        InstallMethod::ResourceMount => probe_resource_mount(game_path)
            .ok()
            .map(|plan| validate_installed_resource_mount(&plan).unwrap_or(false))
            .unwrap_or(false),
        InstallMethod::Loader => validate_installed_loader(game_path).unwrap_or(false),
        InstallMethod::SignatureBypass => {
            validate_installed_signature_bypass(game_path).unwrap_or(false)
        }
    };
    if !owned && targets.iter().any(|target| !target_is_available(target)) {
        return Err(format!(
            "target_conflict: artefak pada target {} tidak memiliki ownership marker valid",
            targets
                .iter()
                .find(|target| !target_is_available(target))
                .map(|target| path_string(target))
                .unwrap_or_default()
        ));
    }
    Ok(())
}

fn write_transaction_files(files: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let transaction_dir = files
        .first()
        .and_then(|(path, _)| path.parent())
        .ok_or_else(|| "transaction_target_missing: target tidak memiliki parent".to_string())?
        .join(format!(".wuwaid-transaction-{}", std::process::id()));
    if transaction_dir.exists() {
        fs::remove_dir_all(&transaction_dir)
            .map_err(|error| format!("transaction_cleanup_failed: {error}"))?;
    }
    fs::create_dir_all(&transaction_dir)
        .map_err(|error| format!("transaction_create_failed: {error}"))?;

    let result = (|| -> Result<(), String> {
        for (index, (target, contents)) in files.iter().enumerate() {
            let staged = transaction_dir.join(format!("{}.stage", index));
            fs::write(&staged, contents)
                .map_err(|error| format!("transaction_stage_failed {:?}: {error}", target))?;
        }
        for (index, (target, _)) in files.iter().enumerate() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("transaction_parent_failed {:?}: {error}", parent))?;
            }
            if target.exists() {
                if !target.is_file() {
                    return Err(format!("transaction_target_not_file: {:?}", target));
                }
                fs::remove_file(target)
                    .map_err(|error| format!("transaction_replace_failed {:?}: {error}", target))?;
            }
            fs::rename(transaction_dir.join(format!("{}.stage", index)), target)
                .map_err(|error| format!("transaction_commit_failed {:?}: {error}", target))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&transaction_dir);
    result
}

fn deploy_signature_bypass_transaction(game_path: &Path, pak_source: &Path) -> Result<(), String> {
    if !pak_source.is_file() || !validate_pak_file(pak_source)? {
        return Err("invalid_patch_pak: source PAK tidak valid".to_string());
    }
    let pak_contents = fs::read(pak_source)
        .map_err(|error| format!("transaction_source_failed: {error}"))?;
    let hash = downloader::compute_sha256(pak_source)
        .map_err(|error| format!("transaction_hash_failed: {error}"))?;
    let target = signature::get_signature_bypass_pak_path(game_path);
    let marker = signature::get_signature_bypass_marker_path(game_path);
    write_transaction_files(&[
        (target, pak_contents),
        (
            marker,
            format!("wuwaid-managed-signature-bypass:sha256={hash}").into_bytes(),
        ),
    ])?;
    if !validate_installed_signature_bypass(game_path)? {
        return Err("transaction_validation_failed: signature bypass tidak valid".to_string());
    }
    Ok(())
}

fn deploy_loader_transaction(
    game_path: &Path,
    pak_source: &Path,
    loader_source: &Path,
) -> Result<(), String> {
    if !pak_source.is_file() || !validate_pak_file(pak_source)? {
        return Err("invalid_patch_pak: source PAK tidak valid".to_string());
    }
    if !loader_source.is_file() {
        return Err("loader_source_missing: source loader tidak ditemukan".to_string());
    }
    let loader_contents = fs::read(loader_source)
        .map_err(|error| format!("loader_source_read_failed: {error}"))?;
    if loader_contents.is_empty() {
        return Err("loader_source_empty: source loader kosong".to_string());
    }
    let pak_contents = fs::read(pak_source)
        .map_err(|error| format!("transaction_source_failed: {error}"))?;
    let pak_hash = downloader::compute_sha256(pak_source)
        .map_err(|error| format!("transaction_hash_failed: {error}"))?;
    let loader_hash = downloader::compute_sha256(loader_source)
        .map_err(|error| format!("loader_hash_failed: {error}"))?;
    let pak_target = signature::get_loader_pak_path(game_path);
    let loader_target = signature::get_loader_dll_path(game_path);
    let marker_target = signature::get_loader_marker_path(game_path);
    write_transaction_files(&[
        (pak_target, pak_contents),
        (loader_target, loader_contents),
        (
            marker_target,
            format!(
                "wuwaid-managed-loader:pak-sha256={pak_hash};loader-sha256={loader_hash}"
            )
            .into_bytes(),
        ),
    ])?;
    if !validate_installed_loader(game_path)? {
        return Err("transaction_validation_failed: loader tidak valid".to_string());
    }
    Ok(())
}

fn remove_if_file(path: &Path, report: &mut CleanupReport) {
    if !path.exists() {
        return;
    }
    if !path.is_file() {
        report.preserved.push(path_string(path));
        return;
    }
    match fs::remove_file(path) {
        Ok(()) => report.removed.push(path_string(path)),
        Err(error) => report
            .failures
            .push(format!("{}: {}", path_string(path), error)),
    }
}

fn preserve_paths(paths: &[PathBuf], report: &mut CleanupReport) {
    for path in paths {
        if path.exists() {
            report.preserved.push(path_string(path));
        }
    }
}

fn cleanup_owned_artifacts_except(
    game_path: &Path,
    keep: Option<InstallMethod>,
) -> Result<CleanupReport, String> {
    let mut report = CleanupReport::default();

    if keep != Some(InstallMethod::ResourceMount) {
        if let Ok(plan) = probe_resource_mount(game_path) {
            if validate_installed_resource_mount(&plan).unwrap_or(false) {
                for target in [
                    plan.owner_marker_path,
                    plan.pak_path,
                    plan.sig_path,
                    plan.mount_path,
                ] {
                    remove_if_file(&target, &mut report);
                }
            } else {
                preserve_paths(
                    &[plan.owner_marker_path, plan.pak_path, plan.sig_path, plan.mount_path],
                    &mut report,
                );
            }
        }
    }

    if keep != Some(InstallMethod::SignatureBypass) {
        if validate_installed_signature_bypass(game_path).unwrap_or(false) {
            remove_if_file(&signature::get_signature_bypass_pak_path(game_path), &mut report);
            remove_if_file(
                &signature::get_signature_bypass_marker_path(game_path),
                &mut report,
            );
        } else {
            preserve_paths(
                &[
                    signature::get_signature_bypass_pak_path(game_path),
                    signature::get_signature_bypass_marker_path(game_path),
                ],
                &mut report,
            );
        }
    }

    if keep != Some(InstallMethod::Loader) {
        if validate_installed_loader(game_path).unwrap_or(false) {
            remove_if_file(&signature::get_loader_pak_path(game_path), &mut report);
            remove_if_file(&signature::get_loader_dll_path(game_path), &mut report);
            remove_if_file(&signature::get_loader_marker_path(game_path), &mut report);
        } else {
            preserve_paths(
                &[
                    signature::get_loader_pak_path(game_path),
                    signature::get_loader_dll_path(game_path),
                    signature::get_loader_marker_path(game_path),
                ],
                &mut report,
            );
        }
    }

    if keep != Some(InstallMethod::SignatureBypass)
        && signature::get_sig_backup_path(game_path).is_file()
    {
        match signature::restore_sig(game_path) {
            Ok(true) => report
                .removed
                .push(path_string(&signature::get_sig_backup_path(game_path))),
            Ok(false) => {}
            Err(error) => report.failures.push(format!("signature_restore: {error}")),
        }
    }

    Ok(report)
}

pub fn cleanup_owned_artifacts(game_path: &Path) -> Result<CleanupReport, String> {
    cleanup_owned_artifacts_except(game_path, None)
}

/// Installs one method as a filesystem transaction and removes only the other
/// launcher-owned methods after the new method is fully validated.
pub fn install_patch_transaction(
    game_path: &Path,
    method: InstallMethod,
    pak_source: &Path,
    loader_source: Option<&Path>,
) -> Result<(), String> {
    validate_installation_preconditions(&path_string(game_path), method)?;
    reject_foreign_targets(game_path, method)?;
    let snapshot = snapshot_files(game_path);

    let deploy_result = match method {
        InstallMethod::ResourceMount => {
            let plan = probe_resource_mount(game_path)?;
            deploy_resource_mount(&plan, pak_source, game_path)
        }
        InstallMethod::Loader => loader_source
            .ok_or_else(|| "loader_source_missing: source loader tidak ditemukan".to_string())
            .and_then(|source| deploy_loader_transaction(game_path, pak_source, source)),
        InstallMethod::SignatureBypass => deploy_signature_bypass_transaction(game_path, pak_source),
    };
    if let Err(error) = deploy_result {
        let rollback = restore_files(&snapshot);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback_failed: {rollback_error}"),
        });
    }

    let cleanup = cleanup_owned_artifacts_except(game_path, Some(method))?;
    if !cleanup.failures.is_empty() {
        let rollback = restore_files(&snapshot);
        return Err(match rollback {
            Ok(()) => format!("cleanup_partial_failure: {}", cleanup.failures.join("; ")),
            Err(rollback_error) => format!(
                "cleanup_partial_failure: {}; rollback_failed: {rollback_error}",
                cleanup.failures.join("; ")
            ),
        });
    }

    Ok(())
}

pub fn probe_resource_mount(game_path: &Path) -> Result<ResourceMountPlan, String> {
    let root = get_resources_root(game_path);
    if !root.exists() {
        return Err("Folder resource game tidak ditemukan.".to_string());
    }

    let mut versions = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest = path.join("ResManifest");
                if manifest.exists() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    versions.push((name, path));
                }
            }
        }
    }

    if versions.is_empty() {
        return Err("Resource game belum siap; ResManifest tidak ditemukan.".to_string());
    }

    // Pick highest/latest version
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    let (version_name, version_dir) = &versions[0];

    let mount_dir = version_dir.join("Mount");
    let patch_dir = version_dir.join("Patch").join(PATCH_FOLDER_NAME);

    let pak_path = patch_dir.join(PATCH_PAK_FILE_NAME);
    let sig_path = patch_dir.join(PATCH_SIG_FILE_NAME);
    let mount_path = mount_dir.join(MOUNT_FILE_NAME);
    let owner_marker_path = patch_dir.join(OWNER_MARKER_FILE_NAME);

    Ok(ResourceMountPlan {
        version_name: version_name.clone(),
        version_dir: version_dir.clone(),
        mount_dir,
        pak_path,
        sig_path,
        mount_path,
        owner_marker_path,
    })
}

pub fn deploy_resource_mount(
    plan: &ResourceMountPlan,
    pak_source: &Path,
    game_path: &Path,
) -> Result<(), String> {
    // A downloaded PAK must be a structurally valid Unreal PAK before it can
    // replace anything in the game directory.
    if !validate_pak_file(pak_source)? {
        return Err("File PAK rilis tidak memiliki footer/index Unreal yang valid.".to_string());
    }

    let targets = [
        plan.pak_path.clone(),
        plan.sig_path.clone(),
        plan.owner_marker_path.clone(),
        plan.mount_path.clone(),
    ];
    let rollback_dir = plan.version_dir.join(format!(
        ".wuwaid-resource-mount-rollback-{}",
        std::process::id()
    ));
    let staging_pak = plan.pak_path.with_extension("tmp_stage");

    let result = (|| -> Result<(), String> {
        if rollback_dir.exists() {
            fs::remove_dir_all(&rollback_dir)
                .map_err(|e| format!("Gagal membersihkan rollback lama: {}", e))?;
        }
        fs::create_dir_all(&rollback_dir)
            .map_err(|e| format!("Gagal membuat rollback Resource Mount: {}", e))?;

        // Snapshot every pre-existing destination, including foreign files.
        // This makes a failed replacement lossless instead of deleting a user's
        // existing mount or signature.
        for (index, target) in targets.iter().enumerate() {
            if target.exists() {
                if !target.is_file() {
                    return Err(format!("Target Resource Mount bukan file: {:?}", target));
                }
                fs::copy(target, rollback_dir.join(format!("{}.bak", index)))
                    .map_err(|e| format!("Gagal mencadangkan {:?}: {}", target, e))?;
            }
        }

        if let Some(parent) = plan.pak_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create patch dir: {}", e))?;
        }
        fs::create_dir_all(&plan.mount_dir)
            .map_err(|e| format!("Failed to create mount dir: {}", e))?;

        let _ = fs::remove_file(&staging_pak);
        fs::copy(pak_source, &staging_pak).map_err(|e| format!("Gagal staging PAK: {}", e))?;
        let sha1 = compute_sha1(&staging_pak)
            .map_err(|e| format!("Gagal menghitung SHA-1 staged PAK: {}", e))?;
        fs::rename(&staging_pak, &plan.pak_path)
            .map_err(|e| format!("Gagal atomic rename PAK: {}", e))?;

        let orig_sig = signature::get_sig_path(game_path);
        let backup_sig = signature::get_sig_backup_path(game_path);
        let source_sig = if orig_sig.is_file() {
            Some(orig_sig)
        } else if backup_sig.is_file() {
            Some(backup_sig)
        } else {
            None
        };
        let source_sig =
            source_sig.ok_or_else(|| "Signature game asli tidak ditemukan.".to_string())?;
        fs::copy(source_sig, &plan.sig_path)
            .map_err(|e| format!("Gagal menyalin signature Resource Mount: {}", e))?;

        fs::write(
            &plan.owner_marker_path,
            format!("wuwaid-managed-mod:sha1={}", sha1).as_bytes(),
        )
        .map_err(|e| format!("Gagal menulis owner marker: {}", e))?;

        let mount_content = format!(
            "99\n../Patch/{}/{}\n../Patch/{}/{}\n",
            PATCH_FOLDER_NAME, PATCH_PAK_FILE_NAME, PATCH_FOLDER_NAME, PATCH_SIG_FILE_NAME
        );
        fs::write(&plan.mount_path, mount_content.as_bytes())
            .map_err(|e| format!("Gagal menulis mount file: {}", e))?;

        if !validate_installed_resource_mount(plan)? {
            return Err("Validasi file index mount gagal.".to_string());
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            let _ = fs::remove_dir_all(&rollback_dir);
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&staging_pak);
            for (index, target) in targets.iter().enumerate() {
                let backup = rollback_dir.join(format!("{}.bak", index));
                if backup.exists() {
                    let _ = fs::remove_file(target);
                    let _ = fs::copy(&backup, target);
                } else if target.is_file() {
                    let _ = fs::remove_file(target);
                }
            }
            let _ = fs::remove_dir_all(&rollback_dir);
            Err(error)
        }
    }
}

pub fn remove_all_owned_artifacts(game_path: &Path) {
    let _ = cleanup_owned_artifacts(game_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::path::validate_game_path;
    use tempfile::tempdir;

    fn release_like_pak(path: &Path) {
        let bytes = pak::pack(
            "../../../",
            0,
            &[(
                "Content/Localization/id.txt".to_string(),
                b"Bahasa Indonesia".to_vec(),
            )],
        )
        .unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn setup_mock_game_dir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path().to_path_buf();

        let paks_dir = game_dir.join("Client").join("Content").join("Paks");
        let res_root = game_dir.join("Client").join("Saved").join("Resources");
        let v2_dir = res_root.join("2.6.0");
        let binaries_dir = game_dir.join("Client").join("Binaries").join("Win64");

        fs::create_dir_all(&paks_dir).unwrap();
        fs::create_dir_all(&v2_dir).unwrap();
        fs::create_dir_all(&binaries_dir).unwrap();
        fs::write(v2_dir.join("ResManifest"), b"manifest 2.6.0").unwrap();
        fs::write(binaries_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();

        let sig_path = signature::get_sig_path(&game_dir);
        fs::write(&sig_path, b"ORIGINAL_GAME_SIG").unwrap();

        (tmp, game_dir)
    }

    #[test]
    fn test_resource_mount_lifecycle() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        let plan = probe_resource_mount(&game_dir).unwrap();
        assert_eq!(plan.version_name, "2.6.0");

        let dummy_pak = game_dir.join("source.pak");
        release_like_pak(&dummy_pak);

        let deploy_res = deploy_resource_mount(&plan, &dummy_pak, &game_dir);
        assert!(deploy_res.is_ok());
        assert!(plan.pak_path.exists());
        assert!(plan.mount_path.exists());
        assert!(plan.owner_marker_path.exists());
        assert!(plan.sig_path.exists());
        assert_eq!(
            fs::read_to_string(&plan.sig_path).unwrap(),
            "ORIGINAL_GAME_SIG"
        );

        assert!(validate_mount_file(&plan.mount_path).unwrap());
        assert!(validate_installed_resource_mount(&plan).unwrap());

        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());
        assert!(!plan.mount_path.exists());
        assert!(!plan.owner_marker_path.exists());
    }

    #[test]
    fn test_e2e_resource_mount() {
        let (_tmp, game_dir) = setup_mock_game_dir();
        assert!(validate_game_path(&game_dir).is_some());

        let plan = probe_resource_mount(&game_dir).unwrap();
        let pak_payload = game_dir.join("resource_mount.pak");
        release_like_pak(&pak_payload);

        assert!(deploy_resource_mount(&plan, &pak_payload, &game_dir).is_ok());
        assert!(plan.pak_path.exists());
        assert!(plan.mount_path.exists());
        assert!(plan.owner_marker_path.exists());
        assert!(plan.sig_path.exists());

        let mount_content = fs::read_to_string(&plan.mount_path).unwrap();
        assert!(mount_content.contains("99\n../Patch/wuwaindonesia/WuWaID_99_P.pak"));

        assert!(validate_installed_resource_mount(&plan).unwrap());

        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());
        assert!(!plan.mount_path.exists());
        assert!(!plan.owner_marker_path.exists());
    }

    #[test]
    fn test_e2e_loader() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // 1. Deploy Loader Method (Method 2)
        let loader_pak = signature::get_loader_pak_path(&game_dir);
        let loader_dll = signature::get_loader_dll_path(&game_dir);

        if let Some(parent) = loader_pak.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        release_like_pak(&loader_pak);
        fs::write(&loader_dll, b"LOADER_DLL_MOCK").unwrap();
        fs::write(
            signature::get_loader_marker_path(&game_dir),
            format!(
                "wuwaid-managed-loader:pak-sha256={};loader-sha256={}",
                downloader::compute_sha256(&loader_pak).unwrap(),
                downloader::compute_sha256(&loader_dll).unwrap()
            ),
        )
        .unwrap();

        assert!(loader_pak.exists());
        assert!(loader_dll.exists());

        // 2. Verify Cleanup
        remove_all_owned_artifacts(&game_dir);
        assert!(!loader_pak.exists());
        assert!(!loader_dll.exists());
    }

    #[test]
    fn test_e2e_signature_bypass() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // 1. Deploy Method 3 (Sig Bypass)
        let bypass_pak = signature::get_signature_bypass_pak_path(&game_dir);
        release_like_pak(&bypass_pak);
        fs::write(
            signature::get_signature_bypass_marker_path(&game_dir),
            format!(
                "wuwaid-managed-signature-bypass:sha256={}",
                downloader::compute_sha256(&bypass_pak).unwrap()
            ),
        )
        .unwrap();
        assert!(bypass_pak.exists());

        // 2. Signature Bypass & Launch Simulation
        assert!(signature::bypass_sig(&game_dir).unwrap());
        let backup_path = signature::get_sig_backup_path(&game_dir);
        let sig_path = signature::get_sig_path(&game_dir);
        assert!(backup_path.exists());
        assert!(!sig_path.exists());
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "ORIGINAL_GAME_SIG"
        );

        // 3. Signature Restoration Simulation (e.g. after 150s timer or on exit)
        assert!(signature::restore_sig(&game_dir).unwrap());
        assert!(sig_path.exists());
        assert_eq!(fs::read_to_string(&sig_path).unwrap(), "ORIGINAL_GAME_SIG");

        // 4. Cleanup
        remove_all_owned_artifacts(&game_dir);
        assert!(!bypass_pak.exists());
    }

    #[test]
    fn test_e2e_method_switching_and_cleanup() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // Step 1: Install Method 2 (Loader)
        let loader_pak = signature::get_loader_pak_path(&game_dir);
        let loader_dll = signature::get_loader_dll_path(&game_dir);
        if let Some(parent) = loader_pak.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        release_like_pak(&loader_pak);
        fs::write(&loader_dll, b"LOADER_DLL").unwrap();
        fs::write(
            signature::get_loader_marker_path(&game_dir),
            format!(
                "wuwaid-managed-loader:pak-sha256={};loader-sha256={}",
                downloader::compute_sha256(&loader_pak).unwrap(),
                downloader::compute_sha256(&loader_dll).unwrap()
            ),
        )
        .unwrap();
        assert!(loader_pak.exists());

        // Step 2: Switch to Method 1 (Resource Mount) -> Clean previous method artifacts
        remove_all_owned_artifacts(&game_dir);
        assert!(!loader_pak.exists());
        assert!(!loader_dll.exists());

        let plan = probe_resource_mount(&game_dir).unwrap();
        let pak_payload = game_dir.join("mount.pak");
        release_like_pak(&pak_payload);
        assert!(deploy_resource_mount(&plan, &pak_payload, &game_dir).is_ok());
        assert!(plan.pak_path.exists());

        // Step 3: Switch to Method 3 (Sig Bypass) -> Clean previous method artifacts
        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());

        let bypass_pak = signature::get_signature_bypass_pak_path(&game_dir);
        release_like_pak(&bypass_pak);
        fs::write(
            signature::get_signature_bypass_marker_path(&game_dir),
            format!(
                "wuwaid-managed-signature-bypass:sha256={}",
                downloader::compute_sha256(&bypass_pak).unwrap()
            ),
        )
        .unwrap();
        assert!(bypass_pak.exists());

        remove_all_owned_artifacts(&game_dir);
        assert!(!bypass_pak.exists());
    }
}
