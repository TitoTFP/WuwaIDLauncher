use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wuwaid_launcher_lib::engine::{
    downloader, installer, method::InstallMethod, pak, patch_asset, path, repak, signature,
};

fn release_like_pak(path: &Path) {
    release_like_pak_with_content(path, b"Bahasa Indonesia");
}

fn release_like_pak_with_content(path: &Path, content: &[u8]) {
    let bytes = pak::pack(
        "../../../",
        0,
        &[("Content/Localization/id.txt".to_string(), content.to_vec())],
    )
    .unwrap();
    fs::write(path, bytes).unwrap();
}

fn create_hide_uid_source_pak(path: &Path) {
    let temp = tempfile::tempdir().unwrap();
    let database = temp
        .path()
        .join("Client/Content/Aki/ConfigDB/en/lang_multi_text.db");
    fs::create_dir_all(database.parent().unwrap()).unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE MultiText (Id TEXT, Content TEXT, RedirectDbIndex INTEGER);
             INSERT INTO MultiText VALUES ('Text_FriendMyUid_Text', 'ID Pengguna: {0}', 0);
             INSERT INTO MultiText VALUES ('Text_UserId_Text', 'ID Pengguna: {0}', 0);
             INSERT INTO MultiText VALUES ('PrefabTextItem_1341587207_Text', 'UID:00000000000', 0);",
        )
        .unwrap();
    repak::pack_v12(temp.path(), path).unwrap();
}

fn setup_game() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().to_path_buf();
    let exe_dir = game.join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&exe_dir).unwrap();
    fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();

    let paks = game.join("Client").join("Content").join("Paks");
    fs::create_dir_all(&paks).unwrap();
    fs::write(paks.join(path::SIG_FILE_NAME), b"ORIGINAL SIG").unwrap();

    let resource_version = game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.6.0");
    let mount_dir = resource_version.join("Mount");
    let official_dir = resource_version.join("Lang_en").join("Base");
    fs::create_dir_all(&mount_dir).unwrap();
    fs::create_dir_all(&official_dir).unwrap();
    fs::write(resource_version.join("ResManifest"), b"manifest").unwrap();
    let official_pak = official_dir.join("pakchunk10-WindowsNoEditor.pak");
    let official_sig = official_dir.join("pakchunk10-WindowsNoEditor.sig");
    fs::write(&official_pak, b"OFFICIAL_RESOURCE_PAK").unwrap();
    fs::write(&official_sig, b"OFFICIAL_RESOURCE_SIG").unwrap();
    fs::write(
        mount_dir.join("MountLang_en.txt"),
        format!(
            "::Mount::\nLang_en/Base/pakchunk10-WindowsNoEditor,4,{},{},,\n::Del::\n",
            installer::compute_sha1(&official_pak).unwrap(),
            installer::compute_sha1(&official_sig).unwrap(),
        ),
    )
    .unwrap();

    (temp, game)
}

fn snapshot_files(paths: &[PathBuf]) -> Vec<Option<Vec<u8>>> {
    paths
        .iter()
        .map(|path| path.is_file().then(|| fs::read(path).unwrap()))
        .collect()
}

fn assert_snapshot_unchanged(paths: &[PathBuf], before: &[Option<Vec<u8>>]) {
    assert_eq!(
        snapshot_files(paths),
        before,
        "deployment changed an artifact after a failed transaction"
    );
}

#[test]
fn preflight_normalizes_game_path_and_selects_method_target() {
    let (_temp, game) = setup_game();
    let nested = game
        .join("Client")
        .join("Binaries")
        .join("Win64")
        .to_string_lossy()
        .to_string();

    let normalized =
        installer::validate_installation_preconditions(&nested, InstallMethod::Loader).unwrap();
    assert_eq!(normalized, fs::canonicalize(&game).unwrap());

    let invalid_dir = tempfile::tempdir().unwrap();
    let invalid = installer::validate_installation_preconditions(
        &invalid_dir.path().to_string_lossy(),
        InstallMethod::ResourceMount,
    )
    .unwrap_err();
    assert!(invalid.contains("invalid_game_path"));
}

#[test]
fn preflight_rejects_resource_mount_without_resource_manifest() {
    let (_temp, game) = setup_game();
    fs::remove_dir_all(game.join("Client").join("Saved").join("Resources")).unwrap();

    let error = installer::validate_installation_preconditions(
        &game.to_string_lossy(),
        InstallMethod::ResourceMount,
    )
    .unwrap_err();
    assert!(error.contains("resource_not_ready"));
}

#[test]
fn loader_transaction_requires_loader_and_leaves_no_partial_artifacts() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    let error =
        installer::install_patch_transaction(&game, InstallMethod::Loader, &pak_source, None)
            .unwrap_err();
    assert!(error.contains("loader_source_missing"));
    assert!(!signature::get_loader_pak_path(&game).exists());
    assert!(!signature::get_loader_dll_path(&game).exists());
    assert!(!signature::get_loader_marker_path(&game).exists());
    assert!(!game
        .join("Client")
        .join("Binaries")
        .join("Win64")
        .join(".wuwaid-transaction")
        .exists());
}

#[test]
fn transaction_switches_methods_and_replaces_canonical_targets() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    let loader_source = game.join("source.dll");
    let unrelated = game
        .join("Client")
        .join("Content")
        .join("Paks")
        .join("user-file.txt");
    release_like_pak(&pak_source);
    fs::write(&loader_source, b"loader bytes").unwrap();
    fs::write(&unrelated, b"do not touch").unwrap();

    installer::install_patch_transaction(
        &game,
        InstallMethod::Loader,
        &pak_source,
        Some(&loader_source),
    )
    .unwrap();
    assert!(installer::validate_installed_loader(&game).unwrap());

    installer::install_patch_transaction(&game, InstallMethod::ResourceMount, &pak_source, None)
        .unwrap();
    let resource_plan = installer::probe_resource_mount(&game).unwrap();
    assert!(installer::validate_installed_resource_mount(&resource_plan).unwrap());
    assert!(!signature::get_loader_pak_path(&game).exists());
    assert!(!signature::get_loader_dll_path(&game).exists());

    let foreign = signature::get_loader_dll_path(&game);
    fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    fs::write(&foreign, b"foreign loader").unwrap();
    let report = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(
        report.preserved.is_empty(),
        "unexpected preserved paths: {report:?}"
    );
    assert!(!foreign.exists());
    assert_eq!(fs::read(&unrelated).unwrap(), b"do not touch");

    let legacy = game
        .join("Client")
        .join("Content")
        .join("Paks")
        .join(path::PAK_FILE_NAME);
    fs::write(&legacy, b"foreign pak").unwrap();
    let report = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(report.failures.is_empty());
    assert!(!legacy.exists());
}

#[test]
fn cleanup_removes_resource_artifacts_across_all_versions() {
    let (_temp, game) = setup_game();
    let old_version = game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.5.0");
    let old_patch = old_version.join("Patch").join(installer::PATCH_FOLDER_NAME);
    let old_mount = old_version.join("Mount");
    fs::create_dir_all(&old_patch).unwrap();
    fs::create_dir_all(&old_mount).unwrap();
    let old_pak = old_patch.join(installer::PATCH_PAK_FILE_NAME);
    let old_sig = old_patch.join(installer::PATCH_SIG_FILE_NAME);
    let old_marker = old_patch.join(installer::OWNER_MARKER_FILE_NAME);
    let old_mount_file = old_mount.join(installer::MOUNT_FILE_NAME);
    fs::write(&old_pak, b"old patch").unwrap();
    fs::write(&old_sig, b"old signature").unwrap();
    fs::write(&old_marker, b"legacy marker").unwrap();
    fs::write(&old_mount_file, b"old mount").unwrap();

    let report = installer::cleanup_owned_artifacts(&game).unwrap();

    assert!(
        report.failures.is_empty(),
        "unexpected failures: {report:?}"
    );
    assert!(
        report.preserved.is_empty(),
        "unexpected preserved paths: {report:?}"
    );
    assert!(!old_pak.exists());
    assert!(!old_sig.exists());
    assert!(!old_marker.exists());
    assert!(!old_mount_file.exists());
    assert!(game
        .join("Client")
        .join("Saved")
        .join("Resources")
        .join("2.6.0")
        .join("ResManifest")
        .exists());
}

#[test]
fn non_file_canonical_target_blocks_install_and_rolls_back() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    let loader_dll = signature::get_loader_dll_path(&game);
    fs::create_dir(&loader_dll).unwrap();

    let error = installer::install_patch_transaction(
        &game,
        InstallMethod::ResourceMount,
        &pak_source,
        None,
    )
    .unwrap_err();

    assert!(error.contains("cleanup_partial_failure"));
    assert!(error.contains("preserved"));
    assert!(loader_dll.is_dir());
}

#[test]
fn invalid_generated_hide_uid_pak_leaves_existing_installation_byte_for_byte_unchanged() {
    let (_temp, game) = setup_game();
    let valid_source = game.join("valid-source.pak");
    let hide_source = game.join("hide-source.pak");
    release_like_pak(&valid_source);
    create_hide_uid_source_pak(&hide_source);
    let source_hash = downloader::compute_sha256(&hide_source).unwrap();
    let derived_cache = game.join("derived-cache");
    let invalid_derived =
        patch_asset::prepare_hide_uid_pak(&hide_source, &source_hash, &derived_cache).unwrap();
    fs::write(&invalid_derived, b"not a valid V12 pak").unwrap();

    installer::install_patch_transaction(&game, InstallMethod::ResourceMount, &valid_source, None)
        .unwrap();
    let plan = installer::probe_resource_mount(&game).unwrap();
    let tracked = vec![
        signature::get_sig_path(&game),
        plan.pak_path.clone(),
        plan.sig_path.clone(),
        plan.mount_path.clone(),
        plan.owner_marker_path.clone(),
    ];
    let before = snapshot_files(&tracked);

    let error = installer::install_patch_transaction(
        &game,
        InstallMethod::ResourceMount,
        &invalid_derived,
        None,
    )
    .unwrap_err();

    assert!(error.contains("footer/index"));
    assert_snapshot_unchanged(&tracked, &before);
}

#[test]
fn metadata_commit_failure_restores_every_deployed_artifact_byte_for_byte() {
    let (_temp, game) = setup_game();
    let original_source = game.join("original-source.pak");
    let replacement_source = game.join("replacement-source.pak");
    release_like_pak_with_content(&original_source, b"original patch");
    release_like_pak_with_content(&replacement_source, b"replacement patch");

    installer::install_patch_transaction(
        &game,
        InstallMethod::ResourceMount,
        &original_source,
        None,
    )
    .unwrap();
    let plan = installer::probe_resource_mount(&game).unwrap();
    let tracked = vec![
        signature::get_sig_path(&game),
        plan.pak_path.clone(),
        plan.sig_path.clone(),
        plan.mount_path.clone(),
        plan.owner_marker_path.clone(),
    ];
    let before = snapshot_files(&tracked);

    let error = installer::install_patch_transaction_with_commit(
        &game,
        InstallMethod::ResourceMount,
        &replacement_source,
        None,
        || Err("forced metadata failure".to_string()),
    )
    .unwrap_err();

    assert!(error.contains("metadata_commit_failed"));
    assert_snapshot_unchanged(&tracked, &before);
}

#[test]
fn repeated_cleanup_is_idempotent_and_reports_owned_artifacts() {
    let (_temp, game) = setup_game();
    let pak_source = game.join("source.pak");
    release_like_pak(&pak_source);

    installer::install_patch_transaction(&game, InstallMethod::ResourceMount, &pak_source, None)
        .unwrap();
    let first = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(!first.removed.is_empty());
    assert!(!installer::probe_resource_mount(&game)
        .unwrap()
        .pak_path
        .exists());

    let second = installer::cleanup_owned_artifacts(&game).unwrap();
    assert!(second.removed.is_empty());
    assert!(second.failures.is_empty());
}
