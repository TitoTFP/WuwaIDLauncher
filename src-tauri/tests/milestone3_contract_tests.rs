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
fn runtime_reconciliation_accepts_verified_descendant_handoff() {
    let parents = [(99, 42), (123, 99)];

    assert!(runtime::process_tree_contains(42, 123, &parents));
    assert_eq!(
        runtime::reconcile_runtime_state_with_owned(Some(42), Some(123), Some(123)),
        RuntimeState {
            active: true,
            origin: ProcessOrigin::Launcher,
        }
    );
    assert_eq!(
        runtime::reconcile_runtime_state_with_owned(Some(42), Some(777), None),
        RuntimeState {
            active: true,
            origin: ProcessOrigin::External,
        }
    );
}

#[test]
fn launch_preconditions_reject_invalid_and_uninstalled_patch() {
    let (_temp, game) = mock_game();
    let not_ready =
        runtime::validate_launch_preconditions(&game.to_string_lossy(), InstallMethod::Loader)
            .unwrap_err();
    assert!(not_ready.contains("patch_not_ready"));

    let invalid_dir = tempfile::tempdir().unwrap();
    let invalid = runtime::validate_launch_preconditions(
        &invalid_dir.path().to_string_lossy(),
        InstallMethod::ResourceMount,
    )
    .unwrap_err();
    assert!(invalid.contains("invalid_game_path"));
}
