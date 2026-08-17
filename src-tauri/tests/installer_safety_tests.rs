use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use wuwaid_launcher_lib::engine::{installer, pak, path, signature};

fn game_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempdir().unwrap();
    let game = tmp.path().join("Wuthering Waves");
    let resources = game.join("Client/Saved/Resources/2.6.0");
    fs::create_dir_all(game.join("Client/Binaries/Win64")).unwrap();
    fs::create_dir_all(&resources).unwrap();
    fs::write(
        game.join("Client/Binaries/Win64/Client-Win64-Shipping.exe"),
        b"exe",
    )
    .unwrap();
    fs::write(resources.join("ResManifest"), b"manifest").unwrap();
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
    fs::remove_file(signature::get_sig_path(&game)).unwrap();

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
fn method2_partial_cleanup_removes_only_marked_launcher_artifacts() {
    let temp = tempdir().unwrap();
    let game = temp.path();
    let method2_folder = signature::get_method2_folder(game);
    fs::create_dir_all(&method2_folder).unwrap();
    let method2_pak = signature::get_method2_pak_path(game);
    let method2_loader = signature::get_method2_loader_path(game);

    fs::write(&method2_pak, b"launcher-pak").unwrap();
    fs::write(
        signature::get_method2_marker_path(game),
        "wuwaid-managed-method2",
    )
    .unwrap();
    installer::remove_all_owned_artifacts(game);
    assert!(!method2_pak.exists());
    assert!(!signature::get_method2_marker_path(game).exists());

    fs::create_dir_all(&method2_folder).unwrap();
    fs::write(&method2_pak, b"foreign-pak").unwrap();
    fs::write(&method2_loader, b"foreign-loader").unwrap();
    installer::remove_all_owned_artifacts(game);
    assert!(method2_pak.exists());
    assert!(method2_loader.exists());
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
