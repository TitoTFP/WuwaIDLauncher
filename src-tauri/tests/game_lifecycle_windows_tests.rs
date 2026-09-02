#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tempfile::{tempdir, TempDir};
use wuwaid_launcher_lib::engine::{atom_feed::ReleaseNoteEntry, runtime, updater};
use wuwaid_launcher_lib::launcher_update_state;

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
        let pid = self.0.id().to_string();
        let _ = Command::new(windows_system_executable("taskkill.exe"))
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
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

fn launcher_fixture_pid_path(root: &Path) -> PathBuf {
    root.join("launcher-fixture.pid")
}

struct LauncherFixtureCleanup {
    pid_file: PathBuf,
}

impl LauncherFixtureCleanup {
    fn new(root: &Path) -> Self {
        Self {
            pid_file: launcher_fixture_pid_path(root),
        }
    }
}

impl Drop for LauncherFixtureCleanup {
    fn drop(&mut self) {
        let Ok(pid) = fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .ok_or(())
        else {
            return;
        };
        let _ = Command::new(windows_system_executable("taskkill.exe"))
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn launcher_release_note(tag: &str) -> ReleaseNoteEntry {
    ReleaseNoteEntry {
        tag: tag.to_string(),
        date: "2026-08-28T12:00:00Z".to_string(),
        title: format!("WuwaID Launcher {tag}"),
        body: "## What's new\n- Verified update".to_string(),
        author: "WuwaID Team".to_string(),
    }
}

fn release_state_paths(root: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    (
        root.join("launcher-whats-new-transaction.json"),
        root.join("launcher-whats-new-pending.json"),
        root.join("launcher-whats-new-ready.tag"),
        root.join("launcher-whats-new-ready.tag.tmp"),
    )
}

fn write_release_note(path: &Path, note: &ReleaseNoteEntry) {
    fs::write(path, serde_json::to_vec(note).unwrap()).unwrap();
}

fn write_committed_release_note(root: &Path, tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (transaction, pending, ready, _) = release_state_paths(root);
    let note = launcher_release_note(tag);
    write_release_note(&pending, &note);
    fs::write(&ready, format!("{tag}\n")).unwrap();
    (transaction, pending, ready)
}

fn run_handoff_script(path: &Path) -> std::process::ExitStatus {
    let mut script = fs::read_to_string(path).unwrap();
    if std::env::var_os("WINE_HOST_HOME").is_some() {
        // Wine's bundled fc does not implement /B; native Windows runs the exact script.
        script = script.replace(
            "%SystemRoot%\\System32\\fc.exe /B",
            "%SystemRoot%\\System32\\fc.exe",
        );
    }
    fs::write(path, script).unwrap();
    let pid_file = launcher_fixture_pid_path(path.parent().unwrap());
    let mut command = Command::new(windows_system_executable("cmd.exe"));
    let mut child = command
        .env("WUWAID_LAUNCHER_UPDATE_PID_FILE", &pid_file)
        .args(["/D", "/C", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            if !status.success() {
                eprintln!(
                    "update handoff script failed with {status}:\n{}",
                    fs::read_to_string(path)
                        .unwrap_or_else(|error| format!("<unreadable: {error}>"))
                );
            }
            return status;
        }
        assert!(Instant::now() < deadline, "update handoff script timed out");
        sleep(Duration::from_millis(100));
    }
}

fn wait_for_launcher_pid_exit(pid_file: &Path) {
    let pid = fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let filter = format!("PID eq {pid}");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = Command::new(windows_system_executable("tasklist.exe"))
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        let running = String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""));
        if !running {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "failed launcher process did not exit"
        );
        sleep(Duration::from_millis(100));
    }
}

#[test]
fn windows_handoff_commits_whats_new_after_successful_restart() {
    let temp = tempdir().unwrap();
    let _launcher_cleanup = LauncherFixtureCleanup::new(temp.path());
    let current = temp.path().join("WuwaIDLauncher.exe");
    let staging = temp.path().join("staging");
    let handoff = temp.path().join("update-handoff.cmd");
    fs::create_dir_all(&staging).unwrap();
    fs::copy(fixture_binary(), &current).unwrap();
    fs::copy(fixture_binary(), staging.join("WuwaIDLauncher.exe")).unwrap();
    let (transaction, pending, ready, ready_temp) = release_state_paths(temp.path());
    write_release_note(&transaction, &launcher_release_note("v2.10.0"));

    updater::create_update_handoff_with_release_state(
        &staging,
        &current,
        &handoff,
        &transaction,
        &pending,
        &ready,
        "v2.10.0",
    )
    .unwrap();
    let status = run_handoff_script(&handoff);

    assert!(status.success(), "handoff failed with {status}");
    assert!(!handoff.exists());
    assert!(!staging.exists());
    assert!(!transaction.exists());
    assert!(!ready_temp.exists());
    assert!(pending.exists());
    assert_eq!(fs::read_to_string(&ready).unwrap().trim(), "v2.10.0");
    let committed = launcher_update_state::read_committed_release_note(
        &transaction,
        &pending,
        &ready,
        "2.10.0",
    )
    .unwrap();
    assert_eq!(committed.tag, "v2.10.0");
}

#[test]
fn windows_offline_startup_reads_matching_committed_whats_new() {
    let temp = tempdir().unwrap();
    let (transaction, pending, ready) = write_committed_release_note(temp.path(), "v2.10.0");

    let note = launcher_update_state::read_committed_release_note(
        &transaction,
        &pending,
        &ready,
        "2.10.0",
    )
    .expect("matching committed note should be available offline");
    assert_eq!(note.title, "WuwaID Launcher v2.10.0");
    assert_eq!(note.body, "## What's new\n- Verified update");
}

#[test]
fn windows_acknowledgement_is_once_per_tag() {
    let temp = tempdir().unwrap();
    let (transaction, pending, ready) = write_committed_release_note(temp.path(), "v2.10.0");

    launcher_update_state::acknowledge(&transaction, &pending, &ready, "v2.9.2").unwrap();
    assert!(pending.exists());
    launcher_update_state::acknowledge(&transaction, &pending, &ready, "2.10.0").unwrap();
    assert!(!transaction.exists());
    assert!(!pending.exists());
    assert!(!ready.exists());
    launcher_update_state::acknowledge(&transaction, &pending, &ready, "2.10.0").unwrap();
}

#[test]
fn windows_missing_transaction_rejects_handoff_without_display() {
    let temp = tempdir().unwrap();
    let current = temp.path().join("WuwaIDLauncher.exe");
    let staging = temp.path().join("staging");
    let handoff = temp.path().join("update-handoff.cmd");
    fs::create_dir_all(&staging).unwrap();
    fs::copy(fixture_binary(), &current).unwrap();
    fs::copy(fixture_binary(), staging.join("WuwaIDLauncher.exe")).unwrap();
    let current_bytes = fs::read(&current).unwrap();
    let (transaction, pending, ready, ready_temp) = release_state_paths(temp.path());
    write_committed_release_note(temp.path(), "v2.10.0");

    updater::create_update_handoff_with_release_state(
        &staging,
        &current,
        &handoff,
        &transaction,
        &pending,
        &ready,
        "v2.10.0",
    )
    .unwrap();
    let _status = run_handoff_script(&handoff);

    assert!(!handoff.exists());
    assert!(!staging.exists());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
    assert!(!transaction.exists());
    assert!(!pending.exists());
    assert!(!ready.exists());
    assert!(!ready_temp.exists());
    assert!(launcher_update_state::read_committed_release_note(
        &transaction,
        &pending,
        &ready,
        "2.10.0",
    )
    .is_none());
}

#[test]
fn windows_cancelled_update_invalidates_uncommitted_whats_new() {
    let temp = tempdir().unwrap();
    let (transaction, pending, ready, ready_temp) = release_state_paths(temp.path());
    fs::write(&transaction, "transaction").unwrap();
    fs::write(&pending, "pending").unwrap();
    fs::write(&ready, "v2.10.0\n").unwrap();
    fs::write(&ready_temp, "v2.10.0\n").unwrap();

    launcher_update_state::invalidate(&transaction, &pending, &ready).unwrap();
    assert!(!transaction.exists());
    assert!(!pending.exists());
    assert!(!ready.exists());
    assert!(!ready_temp.exists());
}

#[test]
fn windows_failed_update_does_not_display_whats_new() {
    let temp = tempdir().unwrap();
    let _launcher_cleanup = LauncherFixtureCleanup::new(temp.path());
    let current = temp.path().join("WuwaIDLauncher.exe");
    let staging = temp.path().join("staging");
    let handoff = temp.path().join("update-handoff.cmd");
    fs::create_dir_all(&staging).unwrap();
    fs::copy(fixture_binary(), &current).unwrap();
    let current_bytes = fs::read(&current).unwrap();
    let (transaction, pending, ready, _) = release_state_paths(temp.path());
    write_release_note(&transaction, &launcher_release_note("v2.10.0"));
    write_committed_release_note(temp.path(), "v2.10.0");

    updater::create_update_handoff_with_release_state(
        &staging,
        &current,
        &handoff,
        &transaction,
        &pending,
        &ready,
        "v2.10.0",
    )
    .unwrap();
    let _status = run_handoff_script(&handoff);

    assert!(!handoff.exists());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
    assert!(launcher_update_state::read_committed_release_note(
        &transaction,
        &pending,
        &ready,
        "2.10.0",
    )
    .is_none());
    assert!(!transaction.exists());
    assert!(!pending.exists());
    assert!(!ready.exists());
}

#[test]
fn windows_health_check_failure_rolls_back_and_discards_whats_new() {
    let temp = tempdir().unwrap();
    let _launcher_cleanup = LauncherFixtureCleanup::new(temp.path());
    let current = temp.path().join("WuwaIDLauncher.exe");
    let staging = temp.path().join("staging");
    let handoff = temp.path().join("update-handoff.cmd");
    fs::create_dir_all(&staging).unwrap();
    fs::copy(fixture_binary(), &current).unwrap();
    let current_bytes = fs::read(&current).unwrap();
    let unrelated_dir = temp.path().join("unrelated");
    fs::create_dir_all(&unrelated_dir).unwrap();
    let unrelated_launcher = unrelated_dir.join("WuwaIDLauncher.exe");
    fs::copy(fixture_binary(), &unrelated_launcher).unwrap();
    let _unrelated = ChildCleanup(
        Command::new(&unrelated_launcher)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    fs::copy(
        windows_system_executable("wscript.exe"),
        staging.join("WuwaIDLauncher.exe"),
    )
    .unwrap();
    let (transaction, pending, ready, ready_temp) = release_state_paths(temp.path());
    write_release_note(&transaction, &launcher_release_note("v2.10.0"));

    updater::create_update_handoff_with_release_state(
        &staging,
        &current,
        &handoff,
        &transaction,
        &pending,
        &ready,
        "v2.10.0",
    )
    .unwrap();
    let _status = run_handoff_script(&handoff);

    assert!(!handoff.exists());
    assert!(!staging.exists());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
    assert!(!transaction.exists());
    assert!(!pending.exists());
    assert!(!ready.exists());
    assert!(!ready_temp.exists());
    assert!(launcher_update_state::read_committed_release_note(
        &transaction,
        &pending,
        &ready,
        "2.10.0",
    )
    .is_none());
}

#[test]
fn windows_health_failure_stops_launched_process_before_rollback() {
    let temp = tempdir().unwrap();
    let _launcher_cleanup = LauncherFixtureCleanup::new(temp.path());
    let current = temp.path().join("FakeLauncher.exe");
    let staging = temp.path().join("staging");
    let handoff = temp.path().join("update-handoff.cmd");
    fs::create_dir_all(&staging).unwrap();
    fs::copy(fixture_binary(), &current).unwrap();
    fs::copy(fixture_binary(), staging.join("FakeLauncher.exe")).unwrap();
    let current_bytes = fs::read(&current).unwrap();
    let (transaction, pending, ready, ready_temp) = release_state_paths(temp.path());
    fs::create_dir(&pending).unwrap();
    write_release_note(&transaction, &launcher_release_note("v2.10.0"));

    updater::create_update_handoff_with_release_state(
        &staging,
        &current,
        &handoff,
        &transaction,
        &pending,
        &ready,
        "v2.10.0",
    )
    .unwrap();
    let _status = run_handoff_script(&handoff);

    wait_for_launcher_pid_exit(&launcher_fixture_pid_path(temp.path()));
    assert!(!handoff.exists());
    assert!(!staging.exists());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
    assert!(!transaction.exists());
    assert!(pending.is_dir());
    assert!(!ready.exists());
    assert!(!ready_temp.exists());
    fs::remove_dir_all(&pending).unwrap();
}
