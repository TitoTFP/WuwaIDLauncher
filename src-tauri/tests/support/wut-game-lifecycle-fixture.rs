use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

const ROOT_LIFETIME: Duration = Duration::from_millis(500);
const CHILD_LIFETIME: Duration = Duration::from_secs(30);

fn main() {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--child")
    {
        sleep(CHILD_LIFETIME);
        return;
    }

    let executable = std::env::current_exe().expect("fixture executable path");
    Command::new(executable)
        .arg("--child")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("fixture child process");
    sleep(ROOT_LIFETIME);
}
