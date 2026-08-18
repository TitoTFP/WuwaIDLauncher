use crate::engine::{
    method::InstallMethod,
    patch_status::{self, LocalPatchState},
    path::{self, get_binary_dir, GAME_EXE_RELATIVE},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessOrigin {
    Launcher,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeState {
    pub active: bool,
    pub origin: ProcessOrigin,
}

const MAX_OUTPUT_TAIL_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchCommand {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub arguments: Vec<String>,
}

impl LaunchCommand {
    pub fn new(executable: &Path, working_directory: &Path, dx11: bool) -> Self {
        Self {
            executable: executable.to_path_buf(),
            working_directory: working_directory.to_path_buf(),
            arguments: if dx11 {
                vec!["-dx11".to_string()]
            } else {
                Vec::new()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnFailureKind {
    ElevationRequired,
    ElevationCancelled,
    SpawnFailed,
    ImmediateExit,
    ProcessNotDetected,
    ProcessCrashed,
}

impl SpawnFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ElevationRequired => "elevation_required",
            Self::ElevationCancelled => "elevation_cancelled",
            Self::SpawnFailed => "spawn_failed",
            Self::ImmediateExit => "immediate_exit",
            Self::ProcessNotDetected => "process_not_detected",
            Self::ProcessCrashed => "process_crashed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Direct,
    Elevated,
}

impl LaunchMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Elevated => "elevated",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchEvidence {
    pub command: LaunchCommand,
    pub launch_mode: Option<LaunchMode>,
    pub failure_kind: Option<SpawnFailureKind>,
    pub error: Option<String>,
    pub pid: Option<u32>,
    pub process_detected: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub game_log_tail: String,
    pub evidence_path: Option<PathBuf>,
    pub started_at_ms: u128,
    pub detected_at_ms: Option<u128>,
    pub finished_at_ms: Option<u128>,
}

impl LaunchEvidence {
    pub fn for_process(command: LaunchCommand, launch_mode: LaunchMode, pid: u32) -> Self {
        Self {
            command,
            launch_mode: Some(launch_mode),
            failure_kind: None,
            error: None,
            pid: Some(pid),
            process_detected: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            game_log_tail: String::new(),
            evidence_path: None,
            started_at_ms: now_millis(),
            detected_at_ms: None,
            finished_at_ms: None,
        }
    }

    pub fn for_failure(
        command: LaunchCommand,
        failure_kind: SpawnFailureKind,
        evidence_path: Option<PathBuf>,
    ) -> Self {
        let timestamp = now_millis();
        Self {
            command,
            launch_mode: None,
            failure_kind: Some(failure_kind),
            error: None,
            pid: None,
            process_detected: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            game_log_tail: String::new(),
            evidence_path,
            started_at_ms: timestamp,
            detected_at_ms: None,
            finished_at_ms: Some(timestamp),
        }
    }

    pub fn mark_detected(&mut self) {
        self.process_detected = true;
        self.detected_at_ms = Some(now_millis());
    }

    pub fn mark_finished(&mut self) {
        self.finished_at_ms = Some(now_millis());
    }

    pub fn user_message(&self) -> String {
        format!(
            "launch_failure: kind={}; mode={}; executable={}; args={}; pid={}; exit_code={}; error={}; stderr={}; stdout={}; game_log_tail={}; evidence_path={}",
            self.failure_kind
                .map(SpawnFailureKind::as_str)
                .unwrap_or("none"),
            self.launch_mode.map(LaunchMode::as_str).unwrap_or("none"),
            self.command.executable.to_string_lossy(),
            if self.command.arguments.is_empty() {
                "none".to_string()
            } else {
                self.command.arguments.join(" ")
            },
            self.pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            detail_or_none(self.error.as_deref().unwrap_or_default()),
            detail_or_none(&self.stderr),
            detail_or_none(&self.stdout),
            detail_or_none(&self.game_log_tail),
            self.evidence_path
                .as_ref()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "none".to_string()),
        )
    }
}

#[derive(Debug)]
pub struct LaunchFailure {
    pub evidence: LaunchEvidence,
}

impl LaunchFailure {
    fn new_with_mode(
        command: LaunchCommand,
        kind: SpawnFailureKind,
        error: impl Into<String>,
        launch_mode: Option<LaunchMode>,
    ) -> Self {
        let mut evidence = LaunchEvidence::for_failure(command, kind, None);
        evidence.launch_mode = launch_mode;
        evidence.error = Some(error.into());
        Self { evidence }
    }

    pub fn user_message(&self) -> String {
        self.evidence.user_message()
    }
}

impl fmt::Display for LaunchFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.user_message())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

struct DirectProcess {
    child: Child,
    stdout: Option<JoinHandle<Vec<u8>>>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    completed: Option<ProcessResult>,
}

#[cfg(windows)]
struct ElevatedProcess {
    handle: windows::Win32::Foundation::HANDLE,
    completed: Option<ProcessResult>,
}

#[cfg(windows)]
unsafe impl Send for ElevatedProcess {}

#[cfg(windows)]
impl Drop for ElevatedProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

enum ManagedProcess {
    Direct(DirectProcess),
    #[cfg(windows)]
    Elevated(ElevatedProcess),
}

pub struct LaunchedGame {
    pub pid: u32,
    pub mode: LaunchMode,
    process: ManagedProcess,
}

impl LaunchedGame {
    pub fn id(&self) -> u32 {
        self.pid
    }

    pub fn try_wait(&mut self) -> Result<Option<ProcessResult>, String> {
        match &mut self.process {
            ManagedProcess::Direct(process) => process.try_wait(),
            #[cfg(windows)]
            ManagedProcess::Elevated(process) => process.try_wait(),
        }
    }

    pub fn wait(&mut self) -> Result<ProcessResult, String> {
        match &mut self.process {
            ManagedProcess::Direct(process) => process.wait(),
            #[cfg(windows)]
            ManagedProcess::Elevated(process) => process.wait(),
        }
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

fn detail_or_none(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "none".to_string()
    } else {
        trimmed.replace('\n', "\\n").replace('\r', "\\r")
    }
}

pub fn bounded_output_tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_OUTPUT_TAIL_BYTES);
    String::from_utf8_lossy(&bytes[start..]).to_string()
}

pub fn classify_spawn_error(raw_code: Option<i32>) -> SpawnFailureKind {
    match raw_code {
        Some(740) => SpawnFailureKind::ElevationRequired,
        Some(1223) => SpawnFailureKind::ElevationCancelled,
        _ => SpawnFailureKind::SpawnFailed,
    }
}

pub fn build_launch_command(game_path: &Path, dx11: bool) -> LaunchCommand {
    let executable = game_path.join(GAME_EXE_RELATIVE);
    let work_dir = get_binary_dir(game_path);
    LaunchCommand::new(&executable, &work_dir, dx11)
}

pub fn collect_game_log_tail(game_path: &Path) -> String {
    let candidates = [
        game_path.join("game.log"),
        game_path.join("wuwaid_loader_log.txt"),
        game_path.join("Client").join("Saved").join("Logs").join("Client.log"),
    ];
    let newest = candidates
        .into_iter()
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);

    let Some(path) = newest else {
        return String::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return String::new();
    };
    format!(
        "{}: {}",
        path.to_string_lossy(),
        bounded_output_tail(&bytes)
    )
}

fn capture_stream<T>(mut stream: T) -> JoinHandle<Vec<u8>>
where
    T: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stream.read_to_end(&mut output);
        output
    })
}

fn join_capture(capture: Option<JoinHandle<Vec<u8>>>) -> String {
    capture
        .and_then(|thread| thread.join().ok())
        .map(|output| bounded_output_tail(&output))
        .unwrap_or_default()
}

impl DirectProcess {
    fn completed_result(&mut self, status: ExitStatus) -> ProcessResult {
        if let Some(result) = self.completed.clone() {
            return result;
        }

        let result = ProcessResult {
            exit_code: status.code(),
            stdout: join_capture(self.stdout.take()),
            stderr: join_capture(self.stderr.take()),
        };
        self.completed = Some(result.clone());
        result
    }

    fn try_wait(&mut self) -> Result<Option<ProcessResult>, String> {
        if let Some(result) = self.completed.clone() {
            return Ok(Some(result));
        }

        self.child
            .try_wait()
            .map_err(|error| format!("process_try_wait_failed: {error}"))
            .map(|status| status.map(|value| self.completed_result(value)))
    }

    fn wait(&mut self) -> Result<ProcessResult, String> {
        if let Some(result) = self.completed.clone() {
            return Ok(result);
        }

        let status = self
            .child
            .wait()
            .map_err(|error| format!("process_wait_failed: {error}"))?;
        Ok(self.completed_result(status))
    }
}

#[cfg(windows)]
impl ElevatedProcess {
    fn exit_code(&self) -> Result<Option<i32>, String> {
        use windows::Win32::System::Threading::GetExitCodeProcess;

        let mut code = 0u32;
        unsafe {
            GetExitCodeProcess(self.handle, &mut code)
                .map_err(|error| format!("process_exit_code_failed: {error}"))?;
        }
        if code == 259 {
            Ok(None)
        } else {
            Ok(Some(code as i32))
        }
    }

    fn try_wait(&mut self) -> Result<Option<ProcessResult>, String> {
        if let Some(result) = self.completed.clone() {
            return Ok(Some(result));
        }

        if let Some(exit_code) = self.exit_code()? {
            let result = ProcessResult {
                exit_code: Some(exit_code),
                stdout: String::new(),
                stderr: String::new(),
            };
            self.completed = Some(result.clone());
            return Ok(Some(result));
        }
        Ok(None)
    }

    fn wait(&mut self) -> Result<ProcessResult, String> {
        if let Some(result) = self.completed.clone() {
            return Ok(result);
        }

        use windows::Win32::System::Threading::WaitForSingleObject;
        unsafe {
            let _ = WaitForSingleObject(self.handle, u32::MAX);
        }
        self.try_wait()?.ok_or_else(|| {
            "process_wait_failed: process handle completed without an exit code".to_string()
        })
    }
}

fn spawn_direct(command: &LaunchCommand) -> Result<ManagedProcess, std::io::Error> {
    let mut child = Command::new(&command.executable);
    child
        .current_dir(&command.working_directory)
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = child.spawn()?;
    let stdout = child.stdout.take().map(capture_stream);
    let stderr = child.stderr.take().map(capture_stream);
    Ok(ManagedProcess::Direct(DirectProcess {
        child,
        stdout,
        stderr,
        completed: None,
    }))
}

pub fn launch_game(game_path: &Path, dx11: bool) -> Result<LaunchedGame, LaunchFailure> {
    let command = build_launch_command(game_path, dx11);
    let executable = command.executable.clone();
    if !executable.exists() {
        return Err(LaunchFailure::new_with_mode(
            command,
            SpawnFailureKind::SpawnFailed,
            format!("executable_missing: {:?}", executable),
            Some(LaunchMode::Direct),
        ));
    }

    match spawn_direct(&command) {
        Ok(process) => {
            let pid = match &process {
                ManagedProcess::Direct(process) => process.child.id(),
                #[cfg(windows)]
                ManagedProcess::Elevated(_) => unreachable!(),
            };
            Ok(LaunchedGame {
                pid,
                mode: LaunchMode::Direct,
                process,
            })
        }
        Err(error) => {
            let kind = classify_spawn_error(error.raw_os_error());
            #[cfg(windows)]
            if kind == SpawnFailureKind::ElevationRequired {
                return spawn_elevated(&command);
            }
            Err(LaunchFailure::new_with_mode(
                command,
                kind,
                error.to_string(),
                Some(LaunchMode::Direct),
            ))
        }
    }
}

#[cfg(windows)]
fn spawn_elevated(command: &LaunchCommand) -> Result<LaunchedGame, LaunchFailure> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::Win32::System::Threading::GetProcessId;

    let verb: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let executable: Vec<u16> = command
        .executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let working_directory: Vec<u16> = command
        .working_directory
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let arguments = command.arguments.join(" ");
    let arguments: Vec<u16> = std::ffi::OsStr::new(&arguments)
        .encode_wide()
        .chain(Some(0))
        .collect();

    let mut info = SHELLEXECUTEINFOW::default();
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = PCWSTR::from_raw(verb.as_ptr());
    info.lpFile = PCWSTR::from_raw(executable.as_ptr());
    info.lpParameters = PCWSTR::from_raw(arguments.as_ptr());
    info.lpDirectory = PCWSTR::from_raw(working_directory.as_ptr());
    info.nShow = SW_SHOWNORMAL.0;

    let executed = unsafe { ShellExecuteExW(&mut info).is_ok() };
    if !executed {
        let raw_code = unsafe { GetLastError().0 as i32 };
        let kind = classify_spawn_error(Some(raw_code));
        return Err(LaunchFailure::new_with_mode(
            command.clone(),
            if raw_code == 1223 {
                SpawnFailureKind::ElevationCancelled
            } else {
                kind
            },
            format!("ShellExecuteExW error {raw_code}"),
            Some(LaunchMode::Elevated),
        ));
    }

    let handle = info.hProcess;
    let pid = unsafe { GetProcessId(handle) };
    if pid == 0 {
        return Err(LaunchFailure::new_with_mode(
            command.clone(),
            SpawnFailureKind::SpawnFailed,
            "ShellExecuteExW returned no process id",
            Some(LaunchMode::Elevated),
        ));
    }

    Ok(LaunchedGame {
        pid,
        mode: LaunchMode::Elevated,
        process: ManagedProcess::Elevated(ElevatedProcess {
            handle,
            completed: None,
        }),
    })
}

pub fn reconcile_runtime_state(
    launcher_pid: Option<u32>,
    detected_pid: Option<u32>,
) -> RuntimeState {
    RuntimeState {
        active: detected_pid.is_some(),
        origin: if detected_pid.is_some() && launcher_pid == detected_pid {
            ProcessOrigin::Launcher
        } else {
            ProcessOrigin::External
        },
    }
}

pub fn should_restore_signature(process_running: bool, timeout_elapsed: bool) -> bool {
    timeout_elapsed && !process_running
}

pub fn validate_launch_preconditions(
    game_path: &str,
    method: InstallMethod,
) -> Result<std::path::PathBuf, String> {
    let normalized = path::normalize_game_path(game_path)
        .ok_or_else(|| "invalid_game_path: executable game tidak ditemukan".to_string())?;
    let local = patch_status::classify_installation(&normalized, method)
        .map_err(|error| format!("patch_status_failed: {error}"))?;
    if !matches!(local, LocalPatchState::Ready) {
        return Err(format!(
            "patch_not_ready: status patch lokal adalah {:?}",
            local
        ));
    }
    let executable = normalized.join(GAME_EXE_RELATIVE);
    if !executable.is_file() {
        return Err(format!("executable_missing: {:?}", executable));
    }
    Ok(normalized)
}

pub fn find_game_process_id() -> Option<u32> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::EnumProcesses;
        use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };

        let mut process_ids = [0u32; 1024];
        let mut bytes_returned = 0u32;

        unsafe {
            if EnumProcesses(
                process_ids.as_mut_ptr(),
                (process_ids.len() * std::mem::size_of::<u32>()) as u32,
                &mut bytes_returned,
            )
            .is_ok()
            {
                let count = bytes_returned as usize / std::mem::size_of::<u32>();
                for &pid in &process_ids[..count] {
                    if pid == 0 {
                        continue;
                    }
                    if let Ok(handle) =
                        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                    {
                        let mut name_buf = [0u16; 260];
                        let len = GetModuleBaseNameW(handle, None, &mut name_buf);
                        let _ = CloseHandle(handle);
                        if len > 0 {
                            let name = String::from_utf16_lossy(&name_buf[..len as usize]);
                            if name.eq_ignore_ascii_case("Client-Win64-Shipping.exe")
                                || name.eq_ignore_ascii_case("WutheringWaves.exe")
                            {
                                return Some(pid);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

pub fn is_game_running() -> bool {
    find_game_process_id().is_some()
}

pub fn force_quit_game() -> Result<bool, String> {
    #[cfg(windows)]
    {
        if find_game_process_id().is_none() {
            return Ok(false);
        }

        let names = ["Client-Win64-Shipping.exe", "WutheringWaves.exe"];
        let mut found = false;
        let mut errors = Vec::new();
        for name in names {
            match Command::new("taskkill")
                .args(["/F", "/IM", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                Ok(status) if status.success() => found = true,
                Ok(_) => {}
                Err(error) => errors.push(format!("{name}: {error}")),
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("; "));
        }
        if !found && find_game_process_id().is_some() {
            return Err("taskkill tidak menghentikan proses game.".to_string());
        }
        return Ok(found);
    }

    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

pub fn trim_memory_working_set() {
    #[cfg(windows)]
    {
        use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
        use windows::Win32::System::Threading::GetCurrentProcess;

        unsafe {
            let current = GetCurrentProcess();
            let _ = EmptyWorkingSet(current);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_game_running_smoke() {
        // Should not panic
        let _ = is_game_running();
    }

    #[test]
    fn test_trim_working_set_smoke() {
        trim_memory_working_set();
    }

    #[test]
    fn launch_command_contains_working_directory_and_dx11_argument() {
        let command = LaunchCommand::new(
            Path::new(r"C:\Games\Client-Win64-Shipping.exe"),
            Path::new(r"C:\Games"),
            true,
        );

        assert_eq!(command.executable, PathBuf::from(r"C:\Games\Client-Win64-Shipping.exe"));
        assert_eq!(command.working_directory, PathBuf::from(r"C:\Games"));
        assert_eq!(command.arguments, vec!["-dx11"]);
    }

    #[test]
    fn spawn_error_classification_distinguishes_elevation_and_cancel() {
        assert_eq!(
            classify_spawn_error(Some(740)),
            SpawnFailureKind::ElevationRequired
        );
        assert_eq!(
            classify_spawn_error(Some(1223)),
            SpawnFailureKind::ElevationCancelled
        );
        assert_eq!(
            classify_spawn_error(Some(2)),
            SpawnFailureKind::SpawnFailed
        );
    }

    #[test]
    fn launch_failure_message_keeps_actionable_evidence_fields_non_empty() {
        let command = LaunchCommand::new(
            Path::new(r"C:\Games\Client-Win64-Shipping.exe"),
            Path::new(r"C:\Games"),
            false,
        );
        let evidence = LaunchEvidence::for_failure(
            command,
            SpawnFailureKind::ElevationRequired,
            None,
        );
        let message = evidence.user_message();

        assert!(message.contains("elevation_required"));
        assert!(message.contains("pid=none"));
        assert!(message.contains("exit_code=none"));
        assert!(message.contains("stderr=none"));
        assert!(message.contains("stdout=none"));
        assert!(message.contains("game_log_tail=none"));
    }

    #[test]
    fn output_tail_keeps_only_the_last_8_kib() {
        let output = "a".repeat(10 * 1024);
        let tail = bounded_output_tail(output.as_bytes());

        assert_eq!(tail.len(), 8 * 1024);
        assert!(tail.chars().all(|value| value == 'a'));
    }
}
