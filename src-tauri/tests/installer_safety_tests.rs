use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use wuwaid_launcher_lib::engine::{downloader, installer, pak, path, signature};

fn game_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempdir().unwrap();
    let game = tmp.path().join("Wuthering Waves");
    let resources = game.join("Client/Saved/Resources/2.6.0");
    let mount_dir = resources.join("Mount");
    let official_dir = resources.join("Lang_en/Base");
    fs::create_dir_all(game.join("Client/Binaries/Win64")).unwrap();
    fs::create_dir_all(&mount_dir).unwrap();
    fs::create_dir_all(&official_dir).unwrap();
    fs::write(
        game.join("Client/Binaries/Win64/Client-Win64-Shipping.exe"),
        b"exe",
    )
    .unwrap();
    fs::write(resources.join("ResManifest"), b"manifest").unwrap();
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
    fs::create_dir_all(path::get_pak_dir(&game)).unwrap();
    fs::write(signature::get_sig_path(&game), b"original-signature").unwrap();
    (tmp, game)
}

fn valid_pak(path: &Path) {
    let bytes = pak::pack(
        "../../../",
        0,
        &[(
            "Content/Localization/id.txt".into(),
            b"Bahasa Indonesia".to_vec(),
        )],
    )
    .unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn invalid_pak_is_rejected_before_mutation() {
    let (_tmp, game) = game_dir();
    let plan = installer::probe_resource_mount(&game).unwrap();
    let source = game.join("invalid.pak");
    fs::write(&source, b"not-a-pak").unwrap();

    assert!(installer::deploy_resource_mount(&plan, &source, &game).is_err());
    assert!(!plan.pak_path.exists());
    assert!(!plan.mount_path.exists());
}

#[test]
fn failed_deploy_restores_foreign_mount_targets() {
    let (_tmp, game) = game_dir();
    let plan = installer::probe_resource_mount(&game).unwrap();
    fs::create_dir_all(plan.mount_path.parent().unwrap()).unwrap();
    fs::create_dir_all(plan.pak_path.parent().unwrap()).unwrap();
    fs::write(&plan.pak_path, b"foreign-pak").unwrap();
    fs::write(&plan.sig_path, b"foreign-sig").unwrap();
    fs::write(&plan.owner_marker_path, b"foreign-marker").unwrap();
    fs::write(&plan.mount_path, b"foreign-mount").unwrap();

    let source = game.join("valid.pak");
    valid_pak(&source);
    fs::remove_file(&plan.source_signature_path).unwrap();

    assert!(installer::deploy_resource_mount(&plan, &source, &game).is_err());
    assert_eq!(fs::read(&plan.pak_path).unwrap(), b"foreign-pak");
    assert_eq!(fs::read(&plan.sig_path).unwrap(), b"foreign-sig");
    assert_eq!(
        fs::read(&plan.owner_marker_path).unwrap(),
        b"foreign-marker"
    );
    assert_eq!(fs::read(&plan.mount_path).unwrap(), b"foreign-mount");
}

#[test]
fn loader_cleanup_removes_only_owned_launcher_artifacts() {
    let temp = tempdir().unwrap();
    let game = temp.path();
    let loader_folder = signature::get_loader_folder(game);
    fs::create_dir_all(&loader_folder).unwrap();
    let loader_pak = signature::get_loader_pak_path(game);
    let loader_dll = signature::get_loader_dll_path(game);

    valid_pak(&loader_pak);
    fs::write(&loader_dll, b"launcher-loader").unwrap();
    fs::write(
        signature::get_loader_marker_path(game),
        format!(
            "wuwaid-managed-loader:pak-sha256={};loader-sha256={}",
            downloader::compute_sha256(&loader_pak).unwrap(),
            downloader::compute_sha256(&loader_dll).unwrap()
        ),
    )
    .unwrap();
    installer::remove_all_owned_artifacts(game);
    assert!(!loader_pak.exists());
    assert!(!signature::get_loader_marker_path(game).exists());

    fs::create_dir_all(&loader_folder).unwrap();
    fs::write(&loader_pak, b"foreign-pak").unwrap();
    fs::write(&loader_dll, b"foreign-loader").unwrap();
    installer::remove_all_owned_artifacts(game);
    assert!(loader_pak.exists());
    assert!(loader_dll.exists());
}

#[test]
fn remove_keeps_foreign_resource_mount_without_valid_ownership() {
    let (_tmp, game) = game_dir();
    let plan = installer::probe_resource_mount(&game).unwrap();
    fs::create_dir_all(plan.mount_path.parent().unwrap()).unwrap();
    fs::create_dir_all(plan.pak_path.parent().unwrap()).unwrap();
    fs::write(&plan.pak_path, b"foreign-pak").unwrap();
    fs::write(&plan.sig_path, b"foreign-sig").unwrap();
    fs::write(&plan.owner_marker_path, b"wrong-owner").unwrap();
    fs::write(&plan.mount_path, b"foreign-mount").unwrap();

    installer::remove_all_owned_artifacts(&game);
    assert!(plan.pak_path.exists());
    assert!(plan.sig_path.exists());
    assert!(plan.owner_marker_path.exists());
    assert!(plan.mount_path.exists());
}
