use std::fs;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

const ROOT_LIFETIME: Duration = Duration::from_millis(500);
const CHILD_LIFETIME: Duration = Duration::from_secs(30);

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
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--child")
    {
        sleep(CHILD_LIFETIME);
        return;
    }

    let update_mode = signal_launcher_update_ready();
    let executable = std::env::current_exe().expect("fixture executable path");
    Command::new(executable)
        .arg("--child")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fixture child process");
    if update_mode {
        sleep(CHILD_LIFETIME);
    } else {
        sleep(ROOT_LIFETIME);
    }
}
