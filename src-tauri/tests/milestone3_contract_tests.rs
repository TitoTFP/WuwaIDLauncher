use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use wuwaid_launcher_lib::engine::{
    method::InstallMethod,
    runtime::{self, ProcessOrigin, RuntimeState},
};

fn mock_game() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let game = temp.path().to_path_buf();
    let exe_dir = game.join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&exe_dir).unwrap();
    fs::write(exe_dir.join("Client-Win64-Shipping.exe"), b"mock exe").unwrap();
    (temp, game)
}

#[test]
fn runtime_reconciliation_distinguishes_launcher_external_and_idle() {
    assert_eq!(
        runtime::reconcile_runtime_state(Some(42), Some(42)),
        RuntimeState {
            active: true,
            origin: ProcessOrigin::Launcher,
        }
    );
    assert_eq!(
        runtime::reconcile_runtime_state(Some(42), Some(99)),
        RuntimeState {
            active: true,
            origin: ProcessOrigin::External,
        }
    );
    assert_eq!(
        runtime::reconcile_runtime_state(None, None),
        RuntimeState {
            active: false,
            origin: ProcessOrigin::External,
        }
    );
}

#[test]
fn launch_preconditions_reject_invalid_and_uninstalled_patch() {
    let (_temp, game) = mock_game();
    let not_ready = runtime::validate_launch_preconditions(&game.to_string_lossy(), InstallMethod::Loader)
        .unwrap_err();
    assert!(not_ready.contains("patch_not_ready"));

    let invalid_dir = tempfile::tempdir().unwrap();
    let invalid = runtime::validate_launch_preconditions(
        &invalid_dir.path().to_string_lossy(),
        InstallMethod::SignatureBypass,
    )
    .unwrap_err();
    assert!(invalid.contains("invalid_game_path"));
}

#[test]
fn signature_restore_fallback_only_triggers_after_process_exit() {
    assert!(!runtime::should_restore_signature(true, true));
    assert!(!runtime::should_restore_signature(true, false));
    assert!(runtime::should_restore_signature(false, true));
}
