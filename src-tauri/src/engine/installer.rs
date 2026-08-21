use crate::engine::{method::InstallMethod, pak, path, signature};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
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
    pub source_signature_path: PathBuf,
    pub pak_path: PathBuf,
    pub sig_path: PathBuf,
    pub mount_path: PathBuf,
    pub owner_marker_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ResourceMountArtifacts {
    version_dir: PathBuf,
    pak_path: PathBuf,
    sig_path: PathBuf,
    mount_path: PathBuf,
    owner_marker_path: PathBuf,
}

impl ResourceMountArtifacts {
    fn from_version_dir(version_dir: PathBuf) -> Self {
        let mount_dir = version_dir.join("Mount");
        let patch_dir = version_dir.join("Patch").join(PATCH_FOLDER_NAME);
        Self {
            version_dir,
            pak_path: patch_dir.join(PATCH_PAK_FILE_NAME),
            sig_path: patch_dir.join(PATCH_SIG_FILE_NAME),
            mount_path: mount_dir.join(MOUNT_FILE_NAME),
            owner_marker_path: patch_dir.join(OWNER_MARKER_FILE_NAME),
        }
    }
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
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.len() != 4 || lines[0] != "::Mount::" || lines[2] != "::Del::" || !lines[3].is_empty()
    {
        return Ok(false);
    }

    let fields: Vec<&str> = lines[1].split(',').collect();
    Ok(fields.len() == 6
        && fields[0] == patch_mount_entry()
        && fields[1] == "99"
        && valid_sha1(fields[2])
        && valid_sha1(fields[3])
        && fields[4].is_empty()
        && fields[5].is_empty())
}

fn patch_mount_entry() -> String {
    let stem = PATCH_PAK_FILE_NAME
        .strip_suffix(".pak")
        .unwrap_or(PATCH_PAK_FILE_NAME);
    format!("Patch/{}/{}", PATCH_FOLDER_NAME, stem)
}

fn mount_content(pak_sha1: &str, signature_sha1: &str) -> String {
    format!(
        "::Mount::\n{},99,{},{},,\n::Del::\n",
        patch_mount_entry(),
        pak_sha1.to_ascii_uppercase(),
        signature_sha1.to_ascii_uppercase()
    )
}

fn valid_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Validate the footer and indexed data of a real Unreal PAK produced for the
/// launcher.  A non-empty file is not sufficient: the footer must contain the
/// expected magic/version and its SHA-1 must cover the index bytes.
pub fn validate_pak_file(pak_path: &Path) -> Result<bool, String> {
    const FOOTER_SIZE: usize = 16 + 1 + 4 + 4 + 8 + 8 + 20 + (32 * 5);
    const MAGIC_OFFSET: usize = 16 + 1;
    const VERSION_OFFSET: usize = MAGIC_OFFSET + 4;
    const INDEX_OFFSET_OFFSET: usize = VERSION_OFFSET + 4;
    const INDEX_SIZE_OFFSET: usize = INDEX_OFFSET_OFFSET + 8;
    const INDEX_HASH_OFFSET: usize = INDEX_SIZE_OFFSET + 8;

    let mut file = fs::File::open(pak_path).map_err(|e| format!("Gagal membaca PAK: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("Gagal membaca metadata PAK: {}", e))?
        .len();
    if file_len < FOOTER_SIZE as u64 {
        return Ok(false);
    }
    let footer_offset = file_len - FOOTER_SIZE as u64;
    file.seek(SeekFrom::Start(footer_offset))
        .map_err(|e| format!("Gagal mencari footer PAK: {}", e))?;
    let mut footer = [0u8; FOOTER_SIZE];
    file.read_exact(&mut footer)
        .map_err(|e| format!("Gagal membaca footer PAK: {}", e))?;
    let read_u32 = |offset: usize| -> u32 {
        u32::from_le_bytes(footer[offset..offset + 4].try_into().unwrap())
    };
    let read_u64 = |offset: usize| -> u64 {
        u64::from_le_bytes(footer[offset..offset + 8].try_into().unwrap())
    };

    if read_u32(MAGIC_OFFSET) != pak::PAK_MAGIC || read_u32(VERSION_OFFSET) != pak::WUWA_PAK_VERSION
    {
        return Ok(false);
    }

    let index_offset = read_u64(INDEX_OFFSET_OFFSET);
    let index_size = read_u64(INDEX_SIZE_OFFSET);
    if index_offset > footer_offset || index_size > footer_offset - index_offset {
        return Ok(false);
    }

    let expected = &footer[INDEX_HASH_OFFSET..INDEX_HASH_OFFSET + 20];
    let mut hasher = Sha1::new();
    file.seek(SeekFrom::Start(index_offset))
        .map_err(|e| format!("Gagal mencari index PAK: {}", e))?;
    let mut remaining = index_size;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])
            .map_err(|e| format!("Gagal membaca index PAK: {}", e))?;
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(hasher.finalize().as_slice() == expected)
}

pub fn validate_installed_resource_mount(plan: &ResourceMountPlan) -> Result<bool, String> {
    if !plan.pak_path.exists() || !plan.sig_path.exists() || !plan.mount_path.exists() {
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

    if !validate_mount_file(&plan.mount_path)? {
        return Ok(false);
    }
    let expected_mount = mount_content(
        &compute_sha1(&plan.pak_path).map_err(|e| format!("Gagal menghitung SHA-1 PAK: {}", e))?,
        &compute_sha1(&plan.sig_path)
            .map_err(|e| format!("Gagal menghitung SHA-1 signature: {}", e))?,
    );
    let actual_mount = fs::read_to_string(&plan.mount_path)
        .map_err(|e| format!("Gagal membaca mount file: {}", e))?
        .replace("\r\n", "\n");
    if actual_mount != expected_mount {
        return Ok(false);
    }

    Ok(compute_sha1(&plan.sig_path)
        .map_err(|e| format!("Gagal menghitung SHA-1 signature: {}", e))?
        .eq_ignore_ascii_case(
            &compute_sha1(&plan.source_signature_path)
                .map_err(|e| format!("Gagal menghitung SHA-1 signature resmi: {}", e))?,
        ))
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

pub fn validate_installed_loader(game_path: &Path) -> Result<bool, String> {
    let pak_path = loader_pak_path(game_path);
    let dll_path = loader_dll_path(game_path);
    if !pak_path.is_file() || !dll_path.is_file() {
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

    Ok(true)
}

fn legacy_method_pak_path(game_path: &Path) -> PathBuf {
    path::get_pak_dir(game_path).join(path::PAK_FILE_NAME)
}

fn legacy_method_marker_path(game_path: &Path) -> PathBuf {
    path::get_pak_dir(game_path).join(".wuwaid-managed-signature-bypass")
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
    let probe = directory.join(format!(
        ".wuwaid-write-probe-{}-{}",
        std::process::id(),
        stamp
    ));
    fs::write(&probe, b"probe")
        .map_err(|error| format!("needs_admin: target tidak dapat ditulis: {error}"))?;
    fs::remove_file(&probe)
        .map_err(|error| format!("needs_admin: probe permission tidak dapat dibersihkan: {error}"))
}

fn all_resource_mount_artifacts(game_path: &Path) -> Vec<ResourceMountArtifacts> {
    let root = get_resources_root(game_path);
    let mut versions = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let version_dir = entry.path();
            let version_name = entry.file_name().to_string_lossy().to_string();
            if version_dir.is_dir() {
                if let Some(version) = parse_resource_version(&version_name) {
                    versions.push((version, version_dir));
                }
            }
        }
    }
    versions.sort_by_key(|(version, _)| std::cmp::Reverse(*version));
    versions
        .into_iter()
        .map(|(_, version_dir)| ResourceMountArtifacts::from_version_dir(version_dir))
        .collect()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn reject_reparse_ancestors(path: &Path) -> Result<(), String> {
    let mut current = path.to_path_buf();
    loop {
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if is_reparse_point(&metadata) {
                return Err(format!(
                    "unsafe_reparse_point: canonical target berada di reparse point {:?}",
                    current
                ));
            }
        }
        if !current.pop() {
            break;
        }
    }
    Ok(())
}

fn validate_canonical_paths(game_path: &Path) -> Result<(), String> {
    let mut paths = vec![
        signature::get_sig_path(game_path),
        signature::get_sig_backup_path(game_path),
        legacy_method_pak_path(game_path),
        legacy_method_marker_path(game_path),
        signature::get_loader_pak_path(game_path),
        signature::get_loader_dll_path(game_path),
        signature::get_loader_marker_path(game_path),
    ];
    for artifacts in all_resource_mount_artifacts(game_path) {
        paths.extend([
            artifacts.pak_path,
            artifacts.sig_path,
            artifacts.mount_path,
            artifacts.owner_marker_path,
        ]);
    }
    for path in paths {
        reject_reparse_ancestors(&path)?;
    }
    Ok(())
}

fn method_target_directory(game_path: &Path, method: InstallMethod) -> Result<PathBuf, String> {
    match method {
        InstallMethod::ResourceMount => Ok(probe_resource_mount(game_path)?.mount_dir),
        InstallMethod::Loader => Ok(signature::get_loader_folder(game_path)),
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
    validate_canonical_paths(&normalized)?;
    check_directory_writable(&target)?;
    Ok(normalized)
}

fn known_artifact_paths(game_path: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        signature::get_sig_path(game_path),
        signature::get_sig_backup_path(game_path),
        legacy_method_pak_path(game_path),
        legacy_method_marker_path(game_path),
        signature::get_loader_pak_path(game_path),
        signature::get_loader_dll_path(game_path),
        signature::get_loader_marker_path(game_path),
    ];
    for artifacts in all_resource_mount_artifacts(game_path) {
        paths.extend([
            artifacts.pak_path,
            artifacts.sig_path,
            artifacts.mount_path,
            artifacts.owner_marker_path,
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
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("rollback_parent_failed {:?}: {error}", parent))?;
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
            reject_reparse_ancestors(target)?;
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
    let loader_contents =
        fs::read(loader_source).map_err(|error| format!("loader_source_read_failed: {error}"))?;
    if loader_contents.is_empty() {
        return Err("loader_source_empty: source loader kosong".to_string());
    }
    let pak_contents =
        fs::read(pak_source).map_err(|error| format!("transaction_source_failed: {error}"))?;
    let pak_target = signature::get_loader_pak_path(game_path);
    let loader_target = signature::get_loader_dll_path(game_path);
    write_transaction_files(&[(pak_target, pak_contents), (loader_target, loader_contents)])?;
    if !validate_installed_loader(game_path)? {
        return Err("transaction_validation_failed: loader tidak valid".to_string());
    }
    remove_legacy_marker(&signature::get_loader_marker_path(game_path))?;
    Ok(())
}

fn remove_legacy_marker(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(format!("legacy_marker_not_file: {:?}", path));
    }
    fs::remove_file(path)
        .map_err(|error| format!("legacy_marker_remove_failed {:?}: {error}", path))
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

fn cleanup_owned_artifacts_except(
    game_path: &Path,
    keep: Option<InstallMethod>,
) -> Result<CleanupReport, String> {
    validate_canonical_paths(game_path)?;
    let mut report = CleanupReport::default();

    let active_resource_version = if keep == Some(InstallMethod::ResourceMount) {
        probe_resource_mount(game_path)
            .ok()
            .map(|plan| plan.version_dir)
    } else {
        None
    };
    for artifacts in all_resource_mount_artifacts(game_path) {
        let keep_active_resource = active_resource_version
            .as_ref()
            .is_some_and(|version_dir| version_dir == &artifacts.version_dir);
        if !keep_active_resource {
            for target in [artifacts.pak_path, artifacts.sig_path, artifacts.mount_path] {
                remove_if_file(&target, &mut report);
            }
        }
        remove_if_file(&artifacts.owner_marker_path, &mut report);
    }

    remove_if_file(&legacy_method_pak_path(game_path), &mut report);
    remove_if_file(&legacy_method_marker_path(game_path), &mut report);

    if keep != Some(InstallMethod::Loader) {
        remove_if_file(&signature::get_loader_pak_path(game_path), &mut report);
        remove_if_file(&signature::get_loader_dll_path(game_path), &mut report);
    }
    remove_if_file(&signature::get_loader_marker_path(game_path), &mut report);

    if signature::get_sig_backup_path(game_path).is_file() {
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

fn cleanup_report_error(report: &CleanupReport) -> String {
    format!(
        "cleanup_partial_failure: failures=[{}]; preserved=[{}]",
        report.failures.join("; "),
        report.preserved.join("; ")
    )
}

pub fn cleanup_owned_artifacts_with_commit<F>(
    game_path: &Path,
    keep: Option<InstallMethod>,
    commit: F,
) -> Result<CleanupReport, String>
where
    F: FnOnce() -> Result<(), String>,
{
    validate_canonical_paths(game_path)?;
    let snapshot = snapshot_files(game_path);
    let report = cleanup_owned_artifacts_except(game_path, keep)?;
    if !report.failures.is_empty() || !report.preserved.is_empty() {
        let error = cleanup_report_error(&report);
        return Err(match restore_files(&snapshot) {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback_failed: {rollback_error}"),
        });
    }

    if let Err(error) = commit() {
        return Err(match restore_files(&snapshot) {
            Ok(()) => format!("metadata_commit_failed: {error}"),
            Err(rollback_error) => {
                format!("metadata_commit_failed: {error}; rollback_failed: {rollback_error}")
            }
        });
    }

    Ok(report)
}

pub fn cleanup_owned_artifacts(game_path: &Path) -> Result<CleanupReport, String> {
    cleanup_owned_artifacts_except(game_path, None)
}

/// Installs one method as a filesystem transaction and removes canonical paths
/// for the other methods after the new method is fully validated.
pub fn install_patch_transaction(
    game_path: &Path,
    method: InstallMethod,
    pak_source: &Path,
    loader_source: Option<&Path>,
) -> Result<(), String> {
    validate_installation_preconditions(&path_string(game_path), method)?;
    let snapshot = snapshot_files(game_path);

    let deploy_result = match method {
        InstallMethod::ResourceMount => {
            let plan = probe_resource_mount(game_path)?;
            deploy_resource_mount(&plan, pak_source, game_path)
        }
        InstallMethod::Loader => loader_source
            .ok_or_else(|| "loader_source_missing: source loader tidak ditemukan".to_string())
            .and_then(|source| deploy_loader_transaction(game_path, pak_source, source)),
    };
    if let Err(error) = deploy_result {
        let rollback = restore_files(&snapshot);
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback_failed: {rollback_error}"),
        });
    }

    let cleanup = cleanup_owned_artifacts_except(game_path, Some(method))?;
    if !cleanup.failures.is_empty() || !cleanup.preserved.is_empty() {
        let cleanup_error = format!(
            "cleanup_partial_failure: failures=[{}]; preserved=[{}]",
            cleanup.failures.join("; "),
            cleanup.preserved.join("; "),
        );
        let rollback = restore_files(&snapshot);
        return Err(match rollback {
            Ok(()) => cleanup_error,
            Err(rollback_error) => format!("{cleanup_error}; rollback_failed: {rollback_error}",),
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
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() && path.join("ResManifest").exists() {
                if let Some(version) = parse_resource_version(&name) {
                    versions.push((version, name, path));
                }
            }
        }
    }

    if versions.is_empty() {
        return Err("Resource game belum siap; ResManifest tidak ditemukan.".to_string());
    }

    versions.sort_by_key(|b| std::cmp::Reverse(b.0));
    let (_, version_name, version_dir) = versions.remove(0);

    let mount_dir = version_dir.join("Mount");
    if !mount_dir.is_dir() {
        return Err("Folder Mount tidak ditemukan pada resource game aktif.".to_string());
    }
    let source_signature_path = find_official_signature(&version_dir)
        .ok_or_else(|| "Signature resmi tidak ditemukan pada resource game aktif.".to_string())?;
    let patch_dir = version_dir.join("Patch").join(PATCH_FOLDER_NAME);

    let pak_path = patch_dir.join(PATCH_PAK_FILE_NAME);
    let sig_path = patch_dir.join(PATCH_SIG_FILE_NAME);
    let mount_path = mount_dir.join(MOUNT_FILE_NAME);
    let owner_marker_path = patch_dir.join(OWNER_MARKER_FILE_NAME);

    Ok(ResourceMountPlan {
        version_name,
        version_dir: version_dir.clone(),
        mount_dir,
        source_signature_path,
        pak_path,
        sig_path,
        mount_path,
        owner_marker_path,
    })
}

fn parse_resource_version(value: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

fn find_official_signature(version_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let english_root = version_dir.join("Lang_en");
    if let Ok(entries) = fs::read_dir(&english_root) {
        candidates.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir()),
        );
    }
    candidates.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    candidates.push(version_dir.join("Resource").join("Base"));

    let own_stem = PATCH_PAK_FILE_NAME
        .strip_suffix(".pak")
        .unwrap_or(PATCH_PAK_FILE_NAME);
    let own_stem_lower = own_stem.to_ascii_lowercase();
    for directory in candidates {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let mut signatures: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("sig"))
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| !name.to_ascii_lowercase().starts_with(&own_stem_lower))
                        .unwrap_or(false)
            })
            .collect();
        signatures.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
        for signature_path in signatures {
            if is_official_signature(version_dir, &signature_path) {
                return Some(signature_path);
            }
        }
    }
    None
}

fn is_official_signature(version_dir: &Path, signature_path: &Path) -> bool {
    let paired_pak = signature_path.with_extension("pak");
    if !paired_pak.is_file() {
        return false;
    }
    let Ok(relative) = signature_path.strip_prefix(version_dir) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    let mount_name = if relative.starts_with("Lang_en/") {
        "MountLang_en.txt"
    } else if relative.starts_with("Resource/") {
        "MountResource.txt"
    } else {
        return false;
    };
    let Some(mount_entry) = relative.strip_suffix(".sig") else {
        return false;
    };
    let mount_path = version_dir.join("Mount").join(mount_name);
    if !mount_path.is_file() {
        return false;
    }
    let Ok(pak_sha1) = compute_sha1(&paired_pak) else {
        return false;
    };
    let Ok(signature_sha1) = compute_sha1(signature_path) else {
        return false;
    };
    let Ok(content) = fs::read_to_string(mount_path) else {
        return false;
    };
    content.lines().any(|line| {
        let fields: Vec<_> = line.split(',').collect();
        fields.len() >= 4
            && fields[0].eq_ignore_ascii_case(mount_entry)
            && fields[2].eq_ignore_ascii_case(&pak_sha1)
            && fields[3].eq_ignore_ascii_case(&signature_sha1)
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
    validate_canonical_paths(game_path)?;

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
        let pak_sha1 = compute_sha1(&staging_pak)
            .map_err(|e| format!("Gagal menghitung SHA-1 staged PAK: {}", e))?;
        fs::rename(&staging_pak, &plan.pak_path)
            .map_err(|e| format!("Gagal atomic rename PAK: {}", e))?;

        if !plan.source_signature_path.is_file() {
            return Err("Signature resmi tidak ditemukan pada resource game aktif.".to_string());
        }
        fs::copy(&plan.source_signature_path, &plan.sig_path)
            .map_err(|e| format!("Gagal menyalin signature Resource Mount: {}", e))?;
        let signature_sha1 = compute_sha1(&plan.sig_path)
            .map_err(|e| format!("Gagal menghitung SHA-1 signature: {}", e))?;

        let mount_content = mount_content(&pak_sha1, &signature_sha1);
        fs::write(&plan.mount_path, mount_content.as_bytes())
            .map_err(|e| format!("Gagal menulis mount file: {}", e))?;

        if !validate_installed_resource_mount(plan)? {
            return Err("Validasi file index mount gagal.".to_string());
        }
        remove_legacy_marker(&plan.owner_marker_path)?;
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

    #[test]
    fn native_resource_mount_manifest_is_accepted() {
        let tmp = tempdir().unwrap();
        let mount_path = tmp.path().join(MOUNT_FILE_NAME);
        let content = format!(
            "::Mount::\nPatch/{}/{},99,{},{},,\n::Del::\n",
            PATCH_FOLDER_NAME,
            PATCH_PAK_FILE_NAME.trim_end_matches(".pak"),
            "A".repeat(40),
            "B".repeat(40),
        );
        fs::write(&mount_path, content).unwrap();

        assert!(validate_mount_file(&mount_path).unwrap());
    }

    #[test]
    fn native_resource_mount_manifest_formats_path_and_hashes_like_game() {
        let content = mount_content(&"a".repeat(40), &"b".repeat(40));

        assert_eq!(
            content,
            format!(
                "::Mount::\nPatch/{}/{},99,{},{},,\n::Del::\n",
                PATCH_FOLDER_NAME,
                PATCH_PAK_FILE_NAME.trim_end_matches(".pak"),
                "A".repeat(40),
                "B".repeat(40),
            )
        );
    }

    #[test]
    fn resource_mount_accepts_directory_res_manifest() {
        let (_tmp, game_dir) = setup_mock_game_dir();
        let manifest = game_dir
            .join("Client")
            .join("Saved")
            .join("Resources")
            .join("2.6.0")
            .join("ResManifest");
        fs::remove_file(&manifest).unwrap();
        fs::create_dir(&manifest).unwrap();

        let plan = probe_resource_mount(&game_dir).unwrap();

        assert_eq!(plan.version_name, "2.6.0");
    }

    fn setup_mock_game_dir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempdir().unwrap();
        let game_dir = tmp.path().to_path_buf();

        let paks_dir = game_dir.join("Client").join("Content").join("Paks");
        let res_root = game_dir.join("Client").join("Saved").join("Resources");
        let v2_dir = res_root.join("2.6.0");
        let mount_dir = v2_dir.join("Mount");
        let official_dir = v2_dir.join("Lang_en").join("Base");
        let binaries_dir = game_dir.join("Client").join("Binaries").join("Win64");

        fs::create_dir_all(&paks_dir).unwrap();
        fs::create_dir_all(&mount_dir).unwrap();
        fs::create_dir_all(&official_dir).unwrap();
        fs::create_dir_all(&binaries_dir).unwrap();
        fs::write(v2_dir.join("ResManifest"), b"manifest 2.6.0").unwrap();
        fs::write(binaries_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();

        let official_pak = official_dir.join("pakchunk10-WindowsNoEditor.pak");
        let official_sig = official_dir.join("pakchunk10-WindowsNoEditor.sig");
        fs::write(&official_pak, b"OFFICIAL_RESOURCE_PAK").unwrap();
        fs::write(&official_sig, b"OFFICIAL_RESOURCE_SIG").unwrap();
        fs::write(
            mount_dir.join("MountLang_en.txt"),
            format!(
                "::Mount::\nLang_en/Base/pakchunk10-WindowsNoEditor,4,{},{},,\n::Del::\n",
                compute_sha1(&official_pak).unwrap(),
                compute_sha1(&official_sig).unwrap(),
            ),
        )
        .unwrap();

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
        assert!(!plan.owner_marker_path.exists());
        assert!(plan.sig_path.exists());
        assert_eq!(
            fs::read_to_string(&plan.sig_path).unwrap(),
            "OFFICIAL_RESOURCE_SIG"
        );

        assert!(validate_mount_file(&plan.mount_path).unwrap());
        assert!(validate_installed_resource_mount(&plan).unwrap());

        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());
        assert!(!plan.mount_path.exists());
        assert!(!plan.owner_marker_path.exists());
    }

    #[test]
    fn cleanup_removes_legacy_resource_mount_with_explicit_launcher_marker() {
        let (_tmp, game_dir) = setup_mock_game_dir();
        let plan = probe_resource_mount(&game_dir).unwrap();

        fs::create_dir_all(plan.pak_path.parent().unwrap()).unwrap();
        fs::write(&plan.pak_path, b"legacy-placeholder-pak").unwrap();
        fs::write(&plan.sig_path, []).unwrap();
        fs::write(&plan.owner_marker_path, b"wuwaid-managed-mod").unwrap();

        let report = cleanup_owned_artifacts(&game_dir).unwrap();

        assert!(
            report.failures.is_empty(),
            "unexpected failures: {report:?}"
        );
        assert!(
            report.preserved.is_empty(),
            "unexpected preserved paths: {report:?}"
        );
        assert!(!plan.pak_path.exists());
        assert!(!plan.sig_path.exists());
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
        assert!(!plan.owner_marker_path.exists());
        assert!(plan.sig_path.exists());

        let mount_content = fs::read_to_string(&plan.mount_path).unwrap();
        assert!(mount_content.starts_with("::Mount::\nPatch/wuwaindonesia/WuWaID_99_P,99,"));
        assert!(mount_content.ends_with("::Del::\n"));

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

        assert!(loader_pak.exists());
        assert!(loader_dll.exists());

        // 2. Verify Cleanup
        remove_all_owned_artifacts(&game_dir);
        assert!(!loader_pak.exists());
        assert!(!loader_dll.exists());
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

        // Step 3: Re-run cleanup after switching back to Resource Mount.
        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());
    }
}
