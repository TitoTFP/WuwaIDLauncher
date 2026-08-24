#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tempfile::{tempdir, TempDir};
use wuwaid_launcher_lib::engine::runtime;

struct ProcessCleanup {
    root_pid: u32,
    root_identity: Option<runtime::ProcessIdentity>,
    expected: PathBuf,
}

impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        let _ = runtime::force_quit_game_with_identity(
            Some(self.root_pid),
            self.root_identity,
            Some(&self.expected),
            None,
        );
    }
}

struct ChildCleanup(Child);

impl Drop for ChildCleanup {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn windows_system_executable(name: &str) -> PathBuf {
    let windows_dir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(windows_dir).join("System32").join(name)
}

fn fake_game() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let temp = tempdir().unwrap();
    let exe_dir = temp.path().join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&exe_dir).unwrap();
    let expected = exe_dir.join("Client-Win64-Shipping.exe");
    fs::copy(windows_system_executable("wscript.exe"), &expected).unwrap();
    let root_script = temp.path().join("root.vbs");
    let child_script = temp.path().join("child.vbs");
    fs::write(
        &root_script,
        "Set shell = CreateObject(\"WScript.Shell\")\ncommand = Chr(34) & WScript.Arguments(0) & Chr(34) & \" //B \" & Chr(34) & WScript.Arguments(1) & Chr(34)\nshell.Run command, 0, False\n",
    )
    .unwrap();
    fs::write(&child_script, "WScript.Sleep 30000\n").unwrap();
    (temp, expected, root_script, child_script)
}

fn spawn_handoff(expected: &Path, root_script: &Path, child_script: &Path) -> Child {
    Command::new(expected)
        .args([
            "//B",
            root_script.to_str().unwrap(),
            expected.to_str().unwrap(),
            child_script.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn fixture_binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_wut_game_lifecycle_fixture") {
        return PathBuf::from(path);
    }

    let file_name = "wut-game-lifecycle-fixture.exe";
    let current = std::env::current_exe().unwrap();
    let candidates = [
        current.parent().map(Path::to_path_buf),
        current
            .parent()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf),
    ];
    candidates
        .into_iter()
        .flatten()
        .map(|directory| directory.join(file_name))
        .find(|path| path.is_file())
        .expect("Cargo did not expose the lifecycle fixture binary")
}

fn fixture_game() -> (TempDir, PathBuf) {
    let temp = tempdir().unwrap();
    let exe_dir = temp.path().join("Client").join("Binaries").join("Win64");
    fs::create_dir_all(&exe_dir).unwrap();
    let expected = exe_dir.join("Client-Win64-Shipping.exe");
    fs::copy(fixture_binary(), &expected).unwrap();
    (temp, expected)
}

fn wait_for_owned_child(
    root_pid: u32,
    root_identity: runtime::ProcessIdentity,
    expected: &Path,
) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = runtime::find_launcher_game_process_id_with_identity(
            root_pid,
            Some(root_identity),
            Some(expected),
        ) {
            if pid != root_pid {
                return pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "launcher child handoff was not detected"
        );
        sleep(Duration::from_millis(100));
    }
}

fn wait_for_owned_tree_exit(
    root_pid: u32,
    root_identity: runtime::ProcessIdentity,
    expected: &Path,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime::find_launcher_game_process_id_with_identity(
        root_pid,
        Some(root_identity),
        Some(expected),
    )
    .is_some()
    {
        assert!(
            Instant::now() < deadline,
            "launcher-owned game tree did not exit"
        );
        sleep(Duration::from_millis(100));
    }
}

fn assert_direct_handoff_and_force_quit(dx11: bool) {
    let (_temp, expected) = fixture_game();
    let game_path = expected
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf();
    let mut root = runtime::launch_game(&game_path, dx11).unwrap();
    assert_eq!(root.mode, runtime::LaunchMode::Direct);
    let root_pid = root.id();
    let root_identity = runtime::process_identity(root_pid).expect("direct root identity");
    let _cleanup = ProcessCleanup {
        root_pid,
        root_identity: Some(root_identity),
        expected: expected.clone(),
    };

    let child_pid = wait_for_owned_child(root_pid, root_identity, &expected);
    root.wait().unwrap();
    assert_ne!(child_pid, root_pid);
    assert!(runtime::is_launcher_owned_game_process(
        root_pid,
        Some(root_identity),
        child_pid,
        runtime::process_identity(child_pid),
        Some(&expected),
    ));
    assert!(runtime::force_quit_game_with_identity(
        Some(root_pid),
        Some(root_identity),
        Some(&expected),
        None,
    )
    .unwrap());
    wait_for_owned_tree_exit(root_pid, root_identity, &expected);
}

#[test]
fn direct_launch_without_dx11_handoffs_and_force_quits_verified_tree() {
    assert_direct_handoff_and_force_quit(false);
}

#[test]
fn direct_launch_with_dx11_handoffs_and_force_quits_verified_tree() {
    assert_direct_handoff_and_force_quit(true);
}

#[test]
fn elevated_uac_launch_handoffs_and_force_quits_with_retained_handle() {
    let (_temp, expected) = fixture_game();
    let game_path = expected
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf();
    let mut root = runtime::launch_game_elevated(&game_path, true).unwrap();
    assert_eq!(root.mode, runtime::LaunchMode::Elevated);
    let root_pid = root.id();
    let root_identity = runtime::process_identity(root_pid).expect("elevated root identity");
    let retained_handle = root
        .duplicate_termination_handle()
        .unwrap()
        .expect("elevated termination handle");
    let _cleanup = ProcessCleanup {
        root_pid,
        root_identity: Some(root_identity),
        expected: expected.clone(),
    };

    let child_pid = wait_for_owned_child(root_pid, root_identity, &expected);
    root.wait().unwrap();
    assert!(runtime::force_quit_game_with_ownership(
        Some(root_pid),
        Some(root_identity),
        Some(child_pid),
        runtime::process_identity(child_pid),
        Some(&expected),
        Some(retained_handle),
    )
    .unwrap());
    runtime::close_termination_handle(retained_handle);
    wait_for_owned_tree_exit(root_pid, root_identity, &expected);
}

#[test]
fn external_instance_is_not_claimed_or_killed_by_unrelated_launcher_tree() {
    let (_temp, expected, root_script, child_script) = fake_game();
    let host_script = _temp.path().join("launcher-host.vbs");
    fs::write(&host_script, "WScript.Sleep 30000\n").unwrap();
    let host = ChildCleanup(
        Command::new(windows_system_executable("wscript.exe"))
            .args(["//B", host_script.to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let host_pid = host.0.id();
    let host_identity = runtime::process_identity(host_pid).expect("host identity");
    let mut external = spawn_handoff(&expected, &root_script, &child_script);
    let external_pid = external.id();
    let external_identity = runtime::process_identity(external_pid).expect("external identity");

    assert!(!runtime::is_launcher_owned_game_process(
        host_pid,
        Some(host_identity),
        external_pid,
        Some(external_identity),
        Some(&expected),
    ));
    let error = runtime::force_quit_game_with_ownership(
        Some(host_pid),
        Some(host_identity),
        Some(external_pid),
        Some(external_identity),
        Some(&expected),
        None,
    )
    .expect_err("unrelated external game must not be force-quit");
    assert!(error.contains("force_quit_target_not_verified"));
    assert!(runtime::process_identity(external_pid).is_some());

    external.wait().unwrap();
    assert!(runtime::force_quit_game_with_identity(
        Some(external_pid),
        Some(external_identity),
        Some(&expected),
        None,
    )
    .unwrap());
}

#[test]
fn supported_launch_modes_and_dx11_arguments_are_explicit() {
    let game_path = Path::new(r"C:\Games");
    assert!(runtime::build_launch_command(game_path, false)
        .arguments
        .is_empty());
    assert_eq!(
        runtime::build_launch_command(game_path, true).arguments,
        vec!["-dx11"]
    );
    assert_eq!(runtime::LaunchMode::Direct.as_str(), "direct");
    assert_eq!(runtime::LaunchMode::Elevated.as_str(), "elevated");
}

#[test]
fn launcher_child_handoff_stays_owned_and_force_quit_cleans_the_tree() {
    let (_temp, expected, root_script, child_script) = fake_game();
    let mut root = spawn_handoff(&expected, &root_script, &child_script);
    let root_pid = root.id();
    let root_identity = runtime::process_identity(root_pid).expect("root identity");
    let _cleanup = ProcessCleanup {
        root_pid,
        root_identity: Some(root_identity),
        expected: expected.clone(),
    };

    root.wait().unwrap();
    let child_pid = wait_for_owned_child(root_pid, root_identity, &expected);
    assert_ne!(child_pid, root_pid);
    assert!(runtime::force_quit_game_with_identity(
        Some(root_pid),
        Some(root_identity),
        Some(&expected),
        None,
    )
    .unwrap());
    wait_for_owned_tree_exit(root_pid, root_identity, &expected);
}
