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
    let skip_ready = std::env::var_os("WUWAID_LAUNCHER_UPDATE_SKIP_READY").is_some();
    let marker_pid = std::env::var("WUWAID_LAUNCHER_UPDATE_READY_PID")
        .ok()
        .map(|value| value.parse::<u32>().expect("update ready PID"))
        .unwrap_or(pid);
    if let Some(pid_path) = std::env::var_os("WUWAID_LAUNCHER_UPDATE_PID_FILE") {
        fs::write(pid_path, format!("{pid}\n")).expect("update fixture pid");
    }
    if !skip_ready {
        let marker_path = std::path::PathBuf::from(path);
        let marker_name = marker_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("launcher-update-ready.tag.tmp");
        let marker_temp =
            marker_path.with_file_name(format!(".{marker_name}.write-{}.tmp", std::process::id()));
        let marker = std::env::var("WUWAID_LAUNCHER_UPDATE_READY_TEXT")
            .unwrap_or_else(|_| format!("{marker_pid}\n"));
        fs::write(&marker_temp, marker).expect("update ready marker temp");
        fs::rename(&marker_temp, &marker_path).expect("update ready marker");
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

    let executable = std::env::current_exe().expect("fixture executable path");
    let child = Command::new(executable)
        .arg("--child")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fixture child process");
    if let Some(path) = std::env::var_os("WUWAID_LAUNCHER_UPDATE_CHILD_PID_FILE") {
        fs::write(path, child.id().to_string()).expect("update fixture child pid");
    }
    let update_mode = signal_launcher_update_ready();
    if update_mode {
        sleep(CHILD_LIFETIME);
    } else {
        sleep(ROOT_LIFETIME);
    }
}
