use std::fs;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

const ROOT_LIFETIME: Duration = Duration::from_millis(500);
const CHILD_LIFETIME: Duration = Duration::from_secs(30);

fn fixture_child_lifetime() -> Duration {
    std::env::var("WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (1..=3600).contains(seconds))
        .map(Duration::from_secs)
        .unwrap_or(CHILD_LIFETIME)
}

fn signal_launcher_update_ready() -> bool {
    let Some(path) = std::env::var_os("WUWAID_LAUNCHER_UPDATE_READY") else {
        return false;
    };
    let pid = std::process::id();
    fs::write(path, format!("{pid}\n")).expect("update ready marker");
    if let Some(pid_path) = std::env::var_os("WUWAID_LAUNCHER_UPDATE_PID_FILE") {
        fs::write(pid_path, format!("{pid}\n")).expect("update fixture pid");
    }
    true
}

// This fixture intentionally leaves the child alive while the parent remains
// active so lifecycle tests can inspect and terminate the process tree.
#[allow(clippy::zombie_processes)]
fn main() {
    let child_lifetime = fixture_child_lifetime();
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--child")
    {
        sleep(child_lifetime);
        return;
    }

    let update_mode = signal_launcher_update_ready();
    let executable = std::env::current_exe().expect("fixture executable path");
    // Model the real root bootstrap: when a Shipping executable sits beside it
    // under `Client/Binaries/Win64`, hand off to that instead of re-running.
    let child = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .map(|dir| {
            dir.join("Client")
                .join("Binaries")
                .join("Win64")
                .join("Client-Win64-Shipping.exe")
        })
        .filter(|path| path.is_file())
        .unwrap_or(executable);
    Command::new(child)
        .arg("--child")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fixture child process");
    if update_mode {
        sleep(child_lifetime);
    } else {
        sleep(ROOT_LIFETIME);
    }
}
