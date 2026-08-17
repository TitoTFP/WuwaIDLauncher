use crate::engine::{pak, signature};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

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
    // Resource Mount is deleted only when every artifact still matches the
    // launcher-owned structure and PAK hash marker.
    if let Ok(plan) = probe_resource_mount(game_path) {
        if validate_installed_resource_mount(&plan).unwrap_or(false) {
            let _ = fs::remove_file(&plan.owner_marker_path);
            let _ = fs::remove_file(&plan.pak_path);
            let _ = fs::remove_file(&plan.sig_path);
            let _ = fs::remove_file(&plan.mount_path);
            if let Some(patch_dir) = plan.pak_path.parent() {
                let _ = fs::remove_dir(patch_dir);
            }
        }
    }

    // Method 1's canonical PAK has a stable name and is launcher-owned.
    let method1_pak = signature::get_method1_pak_path(game_path);
    if method1_pak.is_file() {
        let _ = fs::remove_file(method1_pak);
    }

    // Method 2 is owned as a pair. A launcher marker permits cleanup of a
    // partial deploy; without it, preserve a lone foreign PAK or loader.
    let method2_pak = signature::get_method2_pak_path(game_path);
    let method2_loader = signature::get_method2_loader_path(game_path);
    let method2_marker = signature::get_method2_marker_path(game_path);
    let marked_owned = method2_marker.is_file()
        && fs::read_to_string(&method2_marker)
            .map(|value| value.trim() == "wuwaid-managed-method2")
            .unwrap_or(false);
    if marked_owned {
        let _ = fs::remove_file(&method2_pak);
        let _ = fs::remove_file(&method2_loader);
        let _ = fs::remove_file(&method2_marker);
        let method2_folder = signature::get_method2_folder(game_path);
        let _ = fs::remove_dir(method2_folder);
    }

    let _ = signature::restore_sig(game_path);
    signature::delete_legacy_files(game_path);
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
    fn test_e2e_resource_mount_method1() {
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
    fn test_e2e_loader_method2() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // 1. Deploy Loader Method (Method 2)
        let method2_pak = signature::get_method2_pak_path(&game_dir);
        let method2_loader = signature::get_method2_loader_path(&game_dir);

        if let Some(parent) = method2_pak.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        release_like_pak(&method2_pak);
        fs::write(&method2_loader, b"LOADER_DLL_MOCK").unwrap();
        fs::write(
            signature::get_method2_marker_path(&game_dir),
            "wuwaid-managed-method2",
        )
        .unwrap();

        assert!(method2_pak.exists());
        assert!(method2_loader.exists());

        // 2. Verify Cleanup
        remove_all_owned_artifacts(&game_dir);
        assert!(!method2_pak.exists());
        assert!(!method2_loader.exists());
    }

    #[test]
    fn test_e2e_signature_bypass_method3() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // 1. Deploy Method 3 (Sig Bypass)
        let method1_pak = signature::get_method1_pak_path(&game_dir);
        release_like_pak(&method1_pak);
        assert!(method1_pak.exists());

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
        assert!(!method1_pak.exists());
    }

    #[test]
    fn test_e2e_method_switching_and_cleanup() {
        let (_tmp, game_dir) = setup_mock_game_dir();

        // Step 1: Install Method 2 (Loader)
        let method2_pak = signature::get_method2_pak_path(&game_dir);
        let method2_loader = signature::get_method2_loader_path(&game_dir);
        if let Some(parent) = method2_pak.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        release_like_pak(&method2_pak);
        fs::write(&method2_loader, b"LOADER_DLL").unwrap();
        fs::write(
            signature::get_method2_marker_path(&game_dir),
            "wuwaid-managed-method2",
        )
        .unwrap();
        assert!(method2_pak.exists());

        // Step 2: Switch to Method 1 (Resource Mount) -> Clean previous method artifacts
        remove_all_owned_artifacts(&game_dir);
        assert!(!method2_pak.exists());
        assert!(!method2_loader.exists());

        let plan = probe_resource_mount(&game_dir).unwrap();
        let pak_payload = game_dir.join("mount.pak");
        release_like_pak(&pak_payload);
        assert!(deploy_resource_mount(&plan, &pak_payload, &game_dir).is_ok());
        assert!(plan.pak_path.exists());

        // Step 3: Switch to Method 3 (Sig Bypass) -> Clean previous method artifacts
        remove_all_owned_artifacts(&game_dir);
        assert!(!plan.pak_path.exists());

        let method1_pak = signature::get_method1_pak_path(&game_dir);
        release_like_pak(&method1_pak);
        assert!(method1_pak.exists());

        remove_all_owned_artifacts(&game_dir);
        assert!(!method1_pak.exists());
    }
}
