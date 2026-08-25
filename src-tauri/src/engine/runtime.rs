use crate::engine::{
    method::InstallMethod,
    patch_status::{self, LocalPatchState},
    path::{self, get_binary_dir, GAME_EXE_RELATIVE},
};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Seek, SeekFrom};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub creation_time: u64,
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
        Self::new_with_options(executable, working_directory, dx11, false)
    }

    pub fn new_with_options(
        executable: &Path,
        working_directory: &Path,
        dx11: bool,
        csharp_environment: bool,
    ) -> Self {
        let mut arguments = Vec::with_capacity(2);
        if dx11 {
            arguments.push("-dx11".to_string());
        }
        if csharp_environment {
            arguments.push("-ForceEnableCSharpEnvironment".to_string());
        }
        Self {
            executable: executable.to_path_buf(),
            working_directory: working_directory.to_path_buf(),
            arguments,
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
        let reason = self
            .error
            .as_deref()
            .map(compact_detail)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_failure_reason(self.failure_kind).to_string());
        format!(
            "launch_failure: kind={}; exit_code={}; reason={}; evidence_path={}",
            self.failure_kind
                .map(SpawnFailureKind::as_str)
                .unwrap_or("none"),
            self.exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            reason,
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

    pub fn finalize(&mut self) -> Result<ProcessResult, String> {
        match &mut self.process {
            ManagedProcess::Direct(process) => process.finalize_result(),
            #[cfg(windows)]
            ManagedProcess::Elevated(process) => process
                .try_wait()?
                .ok_or_else(|| "process_finalize_failed: process has not exited".to_string()),
        }
    }

    #[cfg(windows)]
    pub fn duplicate_termination_handle(&self) -> Result<Option<usize>, String> {
        use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE};
        use windows::Win32::System::Threading::GetCurrentProcess;

        let ManagedProcess::Elevated(process) = &self.process else {
            return Ok(None);
        };
        let mut duplicate = HANDLE::default();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                process.handle,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
            .map_err(|error| format!("process_handle_duplicate_failed: {error}"))?;
        }
        Ok(Some(duplicate.0 as usize))
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

fn compact_detail(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_LENGTH: usize = 180;
    if compact.chars().count() > MAX_LENGTH {
        format!(
            "{}…",
            compact.chars().take(MAX_LENGTH - 1).collect::<String>()
        )
    } else {
        compact
    }
}

fn default_failure_reason(kind: Option<SpawnFailureKind>) -> &'static str {
    match kind {
        Some(SpawnFailureKind::ElevationRequired) => "Administrator permission required",
        Some(SpawnFailureKind::ElevationCancelled) => "Administrator permission cancelled",
        Some(SpawnFailureKind::SpawnFailed) => "Process could not start",
        Some(SpawnFailureKind::ImmediateExit) => "Process exited immediately",
        Some(SpawnFailureKind::ProcessNotDetected) => "Process was not detected",
        Some(SpawnFailureKind::ProcessCrashed) => "Process exited unexpectedly",
        None => "Unknown launch failure",
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
    build_launch_command_with_options(game_path, dx11, false)
}

pub fn build_launch_command_with_options(
    game_path: &Path,
    dx11: bool,
    csharp_environment: bool,
) -> LaunchCommand {
    let executable = game_path.join(GAME_EXE_RELATIVE);
    let work_dir = get_binary_dir(game_path);
    LaunchCommand::new_with_options(&executable, &work_dir, dx11, csharp_environment)
}

pub fn collect_game_log_tail(game_path: &Path) -> String {
    let candidates = [
        game_path.join("game.log"),
        game_path.join("wuwaid_loader_log.txt"),
        game_path
            .join("Client")
            .join("Saved")
            .join("Logs")
            .join("Client.log"),
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
    let Ok(mut file) = std::fs::File::open(&path) else {
        return String::new();
    };
    let Ok(file_len) = file.metadata().map(|metadata| metadata.len()) else {
        return String::new();
    };
    let offset = file_len.saturating_sub(MAX_OUTPUT_TAIL_BYTES as u64);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::with_capacity((file_len - offset) as usize);
    if file
        .take(MAX_OUTPUT_TAIL_BYTES as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return String::new();
    }
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
        let mut output = Vec::with_capacity(MAX_OUTPUT_TAIL_BYTES);
        let mut buffer = [0u8; 16 * 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => append_output_tail(&mut output, &buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        output
    })
}

fn append_output_tail(output: &mut Vec<u8>, chunk: &[u8]) {
    if chunk.len() >= MAX_OUTPUT_TAIL_BYTES {
        output.clear();
        output.extend_from_slice(&chunk[chunk.len() - MAX_OUTPUT_TAIL_BYTES..]);
        return;
    }

    let overflow = output
        .len()
        .saturating_add(chunk.len())
        .saturating_sub(MAX_OUTPUT_TAIL_BYTES);
    if overflow > 0 {
        output.drain(..overflow);
    }
    output.extend_from_slice(chunk);
}

fn join_capture(capture: Option<JoinHandle<Vec<u8>>>) -> String {
    capture
        .and_then(|thread| thread.join().ok())
        .map(|output| bounded_output_tail(&output))
        .unwrap_or_default()
}

fn join_finished_capture(capture: &mut Option<JoinHandle<Vec<u8>>>) -> String {
    if capture.as_ref().is_some_and(|thread| thread.is_finished()) {
        join_capture(capture.take())
    } else {
        String::new()
    }
}

impl DirectProcess {
    fn completed_result(&mut self, status: ExitStatus) -> ProcessResult {
        if let Some(result) = self.completed.clone() {
            return result;
        }

        let result = ProcessResult {
            exit_code: status.code(),
            // Do not join here: a handed-off child can inherit the pipe and
            // keep the reader alive after this root process exits.
            stdout: join_finished_capture(&mut self.stdout),
            stderr: join_finished_capture(&mut self.stderr),
        };
        self.completed = Some(result.clone());
        result
    }

    fn finalize_result(&mut self) -> Result<ProcessResult, String> {
        let Some(mut result) = self.completed.clone() else {
            return Err("process_finalize_failed: process has not exited".to_string());
        };
        // Reader threads may still be held open by a descendant that inherited
        // the pipe. Never block lifecycle cleanup on that descendant.
        result.stdout = join_finished_capture(&mut self.stdout);
        result.stderr = join_finished_capture(&mut self.stderr);
        self.completed = Some(result.clone());
        Ok(result)
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
        if self.completed.is_none() {
            let status = self
                .child
                .wait()
                .map_err(|error| format!("process_wait_failed: {error}"))?;
            self.completed_result(status);
        }
        self.finalize_result()
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

pub fn launch_game(game_path: &Path, dx11: bool) -> Result<LaunchedGame, Box<LaunchFailure>> {
    launch_game_with_options(game_path, dx11, false)
}

pub fn launch_game_with_options(
    game_path: &Path,
    dx11: bool,
    csharp_environment: bool,
) -> Result<LaunchedGame, Box<LaunchFailure>> {
    let command = build_launch_command_with_options(game_path, dx11, csharp_environment);
    let executable = command.executable.clone();
    if !executable.is_file() {
        return Err(Box::new(LaunchFailure::new_with_mode(
            command,
            SpawnFailureKind::SpawnFailed,
            format!("executable_missing: {:?}", executable),
            Some(LaunchMode::Direct),
        )));
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
            Err(Box::new(LaunchFailure::new_with_mode(
                command,
                kind,
                error.to_string(),
                Some(LaunchMode::Direct),
            )))
        }
    }
}

/// Launches through the real Windows `runas` path for lifecycle acceptance
/// coverage. Production launch still chooses direct mode first and only falls
/// back here when Windows reports that elevation is required.
#[cfg(windows)]
pub fn launch_game_elevated(
    game_path: &Path,
    dx11: bool,
) -> Result<LaunchedGame, Box<LaunchFailure>> {
    launch_game_elevated_with_options(game_path, dx11, false)
}

#[cfg(windows)]
pub fn launch_game_elevated_with_options(
    game_path: &Path,
    dx11: bool,
    csharp_environment: bool,
) -> Result<LaunchedGame, Box<LaunchFailure>> {
    let command = build_launch_command_with_options(game_path, dx11, csharp_environment);
    if !command.executable.is_file() {
        return Err(Box::new(LaunchFailure::new_with_mode(
            command,
            SpawnFailureKind::SpawnFailed,
            format!(
                "executable_missing: {:?}",
                game_path.join(GAME_EXE_RELATIVE)
            ),
            Some(LaunchMode::Elevated),
        )));
    }
    spawn_elevated(&command)
}

#[cfg(windows)]
fn spawn_elevated(command: &LaunchCommand) -> Result<LaunchedGame, Box<LaunchFailure>> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::System::Threading::GetProcessId;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

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

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR::from_raw(verb.as_ptr()),
        lpFile: PCWSTR::from_raw(executable.as_ptr()),
        lpParameters: PCWSTR::from_raw(arguments.as_ptr()),
        lpDirectory: PCWSTR::from_raw(working_directory.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    let executed = unsafe { ShellExecuteExW(&mut info).is_ok() };
    if !executed {
        let raw_code = unsafe { GetLastError().0 as i32 };
        let kind = classify_spawn_error(Some(raw_code));
        return Err(Box::new(LaunchFailure::new_with_mode(
            command.clone(),
            if raw_code == 1223 {
                SpawnFailureKind::ElevationCancelled
            } else {
                kind
            },
            format!("ShellExecuteExW error {raw_code}"),
            Some(LaunchMode::Elevated),
        )));
    }

    let handle = info.hProcess;
    let pid = unsafe { GetProcessId(handle) };
    if pid == 0 {
        return Err(Box::new(LaunchFailure::new_with_mode(
            command.clone(),
            SpawnFailureKind::SpawnFailed,
            "ShellExecuteExW returned no process id",
            Some(LaunchMode::Elevated),
        )));
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

pub fn reconcile_runtime_state_with_owned(
    launcher_pid: Option<u32>,
    detected_pid: Option<u32>,
    owned_pid: Option<u32>,
) -> RuntimeState {
    let active = detected_pid.or(owned_pid).is_some();
    RuntimeState {
        active,
        origin: if owned_pid.is_some()
            || (active && launcher_pid.is_some() && launcher_pid == detected_pid)
        {
            ProcessOrigin::Launcher
        } else {
            ProcessOrigin::External
        },
    }
}

/// Returns whether `candidate_pid` is the root or a descendant of `root_pid`.
/// The parent list is a snapshot, so a missing root entry is still valid when
/// the child retains the root PID as its parent after the root exits.
pub fn process_tree_contains(
    root_pid: u32,
    candidate_pid: u32,
    parent_pids: &[(u32, u32)],
) -> bool {
    if root_pid == candidate_pid {
        return true;
    }

    let mut current = candidate_pid;
    for _ in 0..=parent_pids.len() {
        let Some((_, parent_pid)) = parent_pids
            .iter()
            .find(|(process_pid, _)| *process_pid == current)
        else {
            return false;
        };
        if *parent_pid == root_pid {
            return true;
        }
        if *parent_pid == 0 || *parent_pid == current {
            return false;
        }
        current = *parent_pid;
    }
    false
}

pub fn process_identity(pid: u32) -> Option<ProcessIdentity> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let identity = process_identity_from_handle_value(handle.0 as usize, pid);
            let _ = CloseHandle(handle);
            return identity;
        }
    }

    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

#[cfg(windows)]
fn process_identity_from_handle_value(raw_handle: usize, pid: u32) -> Option<ProcessIdentity> {
    use windows::Win32::Foundation::{FILETIME, HANDLE};
    use windows::Win32::System::Threading::GetProcessTimes;

    unsafe {
        let handle = HANDLE(raw_handle as *mut std::ffi::c_void);
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
            .ok()
            .map(|_| ProcessIdentity {
                pid,
                creation_time: (u64::from(creation.dwHighDateTime) << 32)
                    | u64::from(creation.dwLowDateTime),
            })
    }
}

#[cfg(windows)]
fn process_identity_from_termination_handle(raw_handle: usize) -> Option<ProcessIdentity> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::GetProcessId;

    let pid = unsafe { GetProcessId(HANDLE(raw_handle as *mut std::ffi::c_void)) };
    if pid == 0 {
        return None;
    }
    process_identity_from_handle_value(raw_handle, pid)
}

#[cfg(windows)]
const MAX_CACHED_PROCESS_PATHS: usize = 256;

#[derive(Debug, Default)]
pub struct ProcessSnapshotCache {
    #[cfg(windows)]
    executable_paths: HashMap<(u32, u64), Option<String>>,
}

#[cfg(windows)]
struct ProcessSnapshotEntry {
    pid: u32,
    parent_pid: u32,
    executable_name: String,
    executable_path: Option<String>,
}

#[cfg(windows)]
struct ProcessSnapshot {
    entries: Vec<ProcessSnapshotEntry>,
    parent_pids: HashMap<u32, u32>,
}

#[cfg(windows)]
impl ProcessSnapshot {
    fn contains(&self, root_pid: u32, candidate_pid: u32) -> bool {
        if root_pid == candidate_pid {
            return true;
        }

        let mut current = candidate_pid;
        for _ in 0..=self.entries.len() {
            let Some(parent_pid) = self.parent_pids.get(&current).copied() else {
                return false;
            };
            if parent_pid == root_pid {
                return true;
            }
            if parent_pid == 0 || parent_pid == current {
                return false;
            }
            current = parent_pid;
        }
        false
    }

    fn depth(&self, root_pid: u32, candidate_pid: u32) -> Option<usize> {
        if root_pid == candidate_pid {
            return Some(0);
        }

        let mut current = candidate_pid;
        for depth in 1..=self.entries.len() {
            let parent_pid = self.parent_pids.get(&current).copied()?;
            if parent_pid == root_pid {
                return Some(depth);
            }
            if parent_pid == 0 || parent_pid == current {
                return None;
            }
            current = parent_pid;
        }
        None
    }

    fn has_descendant(&self, root_pid: u32) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.pid != root_pid && self.contains(root_pid, entry.pid))
    }
}

#[cfg(windows)]
fn process_snapshot() -> Option<ProcessSnapshot> {
    process_snapshot_with_cache(None)
}

#[cfg(windows)]
fn process_snapshot_with_cache(
    mut cache: Option<&mut ProcessSnapshotCache>,
) -> Option<ProcessSnapshot> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()? };
    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut entries = Vec::new();

    unsafe {
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(entry.szExeFile.len());
                entries.push(ProcessSnapshotEntry {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    executable_name: String::from_utf16_lossy(&entry.szExeFile[..name_len]),
                    executable_path: None,
                });
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    for entry in &mut entries {
        if !is_game_process_name(&entry.executable_name) {
            continue;
        }

        // Cache by PID plus creation time, never by PID alone. This removes
        // repeated path probes during the short handoff window without
        // weakening PID-reuse protection.
        let cache_key = cache.as_ref().and_then(|_| {
            process_identity(entry.pid).map(|identity| (identity.pid, identity.creation_time))
        });
        let cached_path = cache_key.and_then(|key| {
            cache
                .as_mut()
                .and_then(|cache| cache.executable_paths.get(&key).cloned())
        });
        let path = cached_path.unwrap_or_else(|| process_executable_path(entry.pid));
        if let (Some(key), Some(cache)) = (cache_key, cache.as_mut()) {
            if !cache.executable_paths.contains_key(&key)
                && cache.executable_paths.len() >= MAX_CACHED_PROCESS_PATHS
            {
                cache.executable_paths.clear();
            }
            cache.executable_paths.insert(key, path.clone());
        }
        entry.executable_path = path;
    }

    let parent_pids = entries
        .iter()
        .map(|entry| (entry.pid, entry.parent_pid))
        .collect();
    Some(ProcessSnapshot {
        entries,
        parent_pids,
    })
}

#[cfg(windows)]
fn launcher_identity_matches_snapshot(pid: u32, identity: ProcessIdentity) -> bool {
    let Some(snapshot) = process_snapshot() else {
        return false;
    };
    if !snapshot.entries.iter().any(|entry| entry.pid == pid) {
        return true;
    }
    process_identity(pid) == Some(identity)
}

#[cfg(windows)]
fn find_game_process_id_in_snapshot(
    snapshot: &ProcessSnapshot,
    expected_executable: Option<&Path>,
) -> Option<u32> {
    snapshot
        .entries
        .iter()
        .filter(|entry| is_verified_game_snapshot_entry(entry, expected_executable))
        .map(|entry| entry.pid)
        .next()
}

#[cfg(windows)]
fn find_launcher_game_process_id_in_snapshot(
    snapshot: &ProcessSnapshot,
    launcher_pid: u32,
    launcher_identity: ProcessIdentity,
    expected_executable: Option<&Path>,
) -> Option<u32> {
    if launcher_identity.pid != launcher_pid {
        return None;
    }
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.pid == launcher_pid)
        && process_identity(launcher_pid) != Some(launcher_identity)
    {
        return None;
    }

    let mut candidates: Vec<_> = snapshot
        .entries
        .iter()
        .filter(|entry| snapshot.contains(launcher_pid, entry.pid))
        .filter(|entry| is_verified_game_snapshot_entry(entry, expected_executable))
        .filter_map(|entry| {
            snapshot
                .depth(launcher_pid, entry.pid)
                .map(|depth| (depth, entry.pid))
        })
        .collect();
    candidates.sort_unstable();
    candidates.first().map(|(_, pid)| *pid)
}

pub fn has_process_tree_descendant(root_pid: u32) -> Option<bool> {
    #[cfg(windows)]
    {
        return Some(process_snapshot()?.has_descendant(root_pid));
    }

    #[cfg(not(windows))]
    {
        let _ = root_pid;
        None
    }
}

pub fn find_launcher_game_process_id(
    launcher_pid: u32,
    expected_executable: Option<&Path>,
) -> Option<u32> {
    find_launcher_game_process_id_with_identity(launcher_pid, None, expected_executable)
}

pub fn find_launcher_game_process_id_with_identity(
    launcher_pid: u32,
    launcher_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
) -> Option<u32> {
    #[cfg(windows)]
    {
        let launcher_identity = launcher_identity?;
        return find_launcher_game_process_id_in_snapshot(
            &process_snapshot()?,
            launcher_pid,
            launcher_identity,
            expected_executable,
        );
    }

    #[cfg(not(windows))]
    {
        let _ = (launcher_pid, launcher_identity, expected_executable);
        None
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeProcessInspection {
    pub detected_pid: Option<u32>,
    pub owned_pid: Option<u32>,
    pub has_descendant: Option<bool>,
}

pub fn inspect_runtime_processes(
    launcher_pid: Option<u32>,
    launcher_identity: Option<ProcessIdentity>,
    current_pid: Option<u32>,
    current_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    include_descendant: bool,
) -> RuntimeProcessInspection {
    inspect_runtime_processes_inner(
        launcher_pid,
        launcher_identity,
        current_pid,
        current_identity,
        expected_executable,
        include_descendant,
        None,
    )
}

pub fn inspect_runtime_processes_with_cache(
    launcher_pid: Option<u32>,
    launcher_identity: Option<ProcessIdentity>,
    current_pid: Option<u32>,
    current_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    include_descendant: bool,
    cache: &mut ProcessSnapshotCache,
) -> RuntimeProcessInspection {
    inspect_runtime_processes_inner(
        launcher_pid,
        launcher_identity,
        current_pid,
        current_identity,
        expected_executable,
        include_descendant,
        Some(cache),
    )
}

fn inspect_runtime_processes_inner(
    launcher_pid: Option<u32>,
    launcher_identity: Option<ProcessIdentity>,
    current_pid: Option<u32>,
    current_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    include_descendant: bool,
    cache: Option<&mut ProcessSnapshotCache>,
) -> RuntimeProcessInspection {
    #[cfg(windows)]
    {
        let Some(snapshot) = process_snapshot_with_cache(cache) else {
            return RuntimeProcessInspection::default();
        };
        let detected_pid = find_game_process_id_in_snapshot(&snapshot, expected_executable);
        let owned_pid = launcher_pid.and_then(|root_pid| {
            launcher_identity.and_then(|identity| {
                find_launcher_game_process_id_in_snapshot(
                    &snapshot,
                    root_pid,
                    identity,
                    expected_executable,
                )
                .or_else(|| {
                    current_pid.filter(|pid| {
                        is_launcher_owned_game_process_in_snapshot(
                            &snapshot,
                            root_pid,
                            Some(identity),
                            *pid,
                            current_identity,
                            expected_executable,
                        )
                    })
                })
            })
        });
        let has_descendant = include_descendant
            .then(|| launcher_pid.map(|root_pid| snapshot.has_descendant(root_pid)))
            .flatten();
        return RuntimeProcessInspection {
            detected_pid,
            owned_pid,
            has_descendant,
        };
    }

    #[cfg(not(windows))]
    {
        let _ = (
            launcher_pid,
            launcher_identity,
            current_pid,
            current_identity,
            expected_executable,
            include_descendant,
            cache,
        );
        RuntimeProcessInspection::default()
    }
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

pub fn find_game_process_id_for_path(expected_executable: Option<&Path>) -> Option<u32> {
    #[cfg(windows)]
    {
        return find_game_process_id_in_snapshot(&process_snapshot()?, expected_executable);
    }

    #[cfg(not(windows))]
    {
        let _ = expected_executable;
        None
    }
}

pub fn is_game_running_for_path(expected_executable: Option<&Path>) -> bool {
    find_game_process_id_for_path(expected_executable).is_some()
}

#[cfg(windows)]
fn normalize_process_path(path: &std::ffi::OsStr) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

pub fn select_force_quit_pid(tracked_pid: Option<u32>, detected_pid: Option<u32>) -> Option<u32> {
    tracked_pid.or(detected_pid)
}

#[cfg(windows)]
fn is_game_process_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Client-Win64-Shipping.exe")
        || name.eq_ignore_ascii_case("WutheringWaves.exe")
}

#[cfg(windows)]
fn process_executable_path_from_handle(
    handle: windows::Win32::Foundation::HANDLE,
) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;

    let mut path_buf = [0u16; 32_768];
    let path_len = unsafe { GetModuleFileNameExW(handle, None, &mut path_buf) };
    if path_len == 0 {
        return None;
    }
    let actual = OsString::from_wide(&path_buf[..path_len as usize]);
    Some(normalize_process_path(&actual))
}

#[cfg(windows)]
fn process_executable_path(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
        let path = process_executable_path_from_handle(handle);
        let _ = CloseHandle(handle);
        path
    }
}

#[cfg(windows)]
fn process_path_matches_handle(
    handle: windows::Win32::Foundation::HANDLE,
    expected_executable: Option<&Path>,
) -> bool {
    let Some(expected) = expected_executable else {
        return true;
    };
    let Some(actual) = process_executable_path_from_handle(handle) else {
        return false;
    };
    actual == normalize_process_path(expected.as_os_str())
}

#[cfg(windows)]
fn is_verified_game_snapshot_entry(
    entry: &ProcessSnapshotEntry,
    expected_executable: Option<&Path>,
) -> bool {
    if !is_game_process_name(&entry.executable_name) {
        return false;
    }
    let Some(expected_executable) = expected_executable else {
        return true;
    };
    let expected = normalize_process_path(expected_executable.as_os_str());
    entry.executable_path.as_deref() == Some(expected.as_str())
}

#[cfg(windows)]
fn is_verified_game_pid(pid: u32, expected_executable: Option<&Path>) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
        else {
            return false;
        };
        let mut name_buf = [0u16; 260];
        let name_len = GetModuleBaseNameW(handle, None, &mut name_buf);
        if name_len == 0 {
            let _ = CloseHandle(handle);
            return false;
        }
        let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        let is_game_name = is_game_process_name(&name);
        let path_matches = is_game_name && process_path_matches_handle(handle, expected_executable);
        let _ = CloseHandle(handle);
        path_matches
    }
}

pub fn is_verified_game_process(
    pid: u32,
    identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
) -> bool {
    #[cfg(windows)]
    {
        if identity.is_some_and(|expected| process_identity(pid) != Some(expected)) {
            return false;
        }
        return is_verified_game_pid(pid, expected_executable);
    }

    #[cfg(not(windows))]
    {
        let _ = (pid, identity, expected_executable);
        false
    }
}

#[cfg(windows)]
fn is_launcher_owned_game_process_in_snapshot(
    snapshot: &ProcessSnapshot,
    launcher_pid: u32,
    launcher_identity: Option<ProcessIdentity>,
    game_pid: u32,
    game_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
) -> bool {
    let Some(launcher_identity) = launcher_identity else {
        return false;
    };
    if launcher_identity.pid != launcher_pid {
        return false;
    }
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.pid == launcher_pid)
        && process_identity(launcher_pid) != Some(launcher_identity)
    {
        return false;
    }
    if game_identity.is_some_and(|expected| process_identity(game_pid) != Some(expected)) {
        return false;
    }
    snapshot
        .entries
        .iter()
        .find(|entry| entry.pid == game_pid)
        .is_some_and(|entry| {
            is_verified_game_snapshot_entry(entry, expected_executable)
                && snapshot.contains(launcher_pid, game_pid)
        })
}

/// Returns true only when a verified game process is still in the tracked
/// launcher tree. The launcher identity is mandatory: a path/name match alone
/// must never be allowed to claim or terminate an unrelated instance.
pub fn is_launcher_owned_game_process(
    launcher_pid: u32,
    launcher_identity: Option<ProcessIdentity>,
    game_pid: u32,
    game_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
) -> bool {
    #[cfg(windows)]
    {
        let Some(snapshot) = process_snapshot() else {
            return false;
        };
        return is_launcher_owned_game_process_in_snapshot(
            &snapshot,
            launcher_pid,
            launcher_identity,
            game_pid,
            game_identity,
            expected_executable,
        );
    }

    #[cfg(not(windows))]
    {
        let _ = (
            launcher_pid,
            launcher_identity,
            game_pid,
            game_identity,
            expected_executable,
        );
        false
    }
}

#[cfg(windows)]
fn verified_game_tree(
    game_pid: u32,
    game_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    allow_unverified_root: bool,
) -> Result<Vec<u32>, String> {
    if game_identity.is_some_and(|expected| {
        process_identity(game_pid).is_some_and(|current| current != expected)
    }) {
        return Err(format!(
            "force_quit_target_stale: PID {game_pid} identity no longer cocok"
        ));
    }
    if !allow_unverified_root && !is_verified_game_pid(game_pid, expected_executable) {
        return Err(format!(
            "force_quit_target_not_verified: PID {game_pid} bukan proses game yang terverifikasi"
        ));
    }

    let snapshot =
        process_snapshot().ok_or_else(|| "force_quit_process_snapshot_failed".to_string())?;
    let mut pids: Vec<_> = snapshot
        .entries
        .iter()
        .filter(|entry| snapshot.contains(game_pid, entry.pid))
        .map(|entry| entry.pid)
        .collect();
    if !pids.contains(&game_pid) {
        pids.push(game_pid);
    }
    pids.sort_by_key(|pid| std::cmp::Reverse(snapshot.depth(game_pid, *pid).unwrap_or(0)));
    pids.dedup();
    Ok(pids)
}

#[cfg(windows)]
fn terminate_process_pid(pid: u32) -> Result<bool, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
            .map_err(|error| format!("force_quit_open_failed: PID {pid}: {error}"))?;
        let result = (|| -> Result<bool, String> {
            TerminateProcess(handle, 1)
                .map_err(|error| format!("force_quit_terminate_failed: PID {pid}: {error}"))?;
            let _ = WaitForSingleObject(handle, 5_000);
            Ok(true)
        })();
        let _ = CloseHandle(handle);
        result
    }
}

#[cfg(windows)]
fn terminate_process_handle(raw_handle: usize) -> Result<bool, String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};

    let handle = HANDLE(raw_handle as *mut std::ffi::c_void);
    unsafe {
        TerminateProcess(handle, 1)
            .map_err(|error| format!("force_quit_handle_terminate_failed: {error}"))?;
        let _ = WaitForSingleObject(handle, 5_000);
    }
    Ok(true)
}

#[cfg(windows)]
pub fn close_termination_handle(raw_handle: usize) {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};

    unsafe {
        let _ = CloseHandle(HANDLE(raw_handle as *mut std::ffi::c_void));
    }
}

#[cfg(windows)]
fn terminate_process_tree_elevated(game_pid: u32) -> Result<bool, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GetLastError};
    use windows::Win32::System::Threading::WaitForSingleObject;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let taskkill = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("taskkill.exe");
    let verb: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let executable: Vec<u16> = taskkill.as_os_str().encode_wide().chain(Some(0)).collect();
    let parameters: Vec<u16> = std::ffi::OsStr::new(&format!("/PID {game_pid} /T /F"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR::from_raw(verb.as_ptr()),
        lpFile: PCWSTR::from_raw(executable.as_ptr()),
        lpParameters: PCWSTR::from_raw(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    if !unsafe { ShellExecuteExW(&mut info).is_ok() } {
        return Err(format!("force_quit_elevated_failed: {}", unsafe {
            GetLastError().0
        }));
    }
    let handle = info.hProcess;
    unsafe {
        let _ = WaitForSingleObject(handle, 10_000);
        let _ = CloseHandle(handle);
    }
    Ok(true)
}

#[cfg(windows)]
fn terminate_verified_game_tree(
    game_pid: u32,
    game_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    allow_unverified_root: bool,
    termination_handle: Option<usize>,
) -> Result<bool, String> {
    let pids = verified_game_tree(
        game_pid,
        game_identity,
        expected_executable,
        allow_unverified_root,
    )?;
    let mut terminated = false;
    for pid in pids {
        if pid == game_pid {
            if let Some(handle) = termination_handle {
                terminated |= terminate_process_handle(handle)?;
                continue;
            }
        } else if let Some(snapshot) = process_snapshot() {
            if !snapshot.contains(game_pid, pid) {
                continue;
            }
        } else {
            return terminate_process_tree_elevated(game_pid);
        }
        match terminate_process_pid(pid) {
            Ok(value) => terminated |= value,
            Err(error) => {
                log::warn!("Force quit PID {pid} membutuhkan elevated fallback: {error}");
                return terminate_process_tree_elevated(game_pid);
            }
        }
    }
    Ok(terminated)
}

pub fn force_quit_game_with_ownership(
    tracked_pid: Option<u32>,
    tracked_identity: Option<ProcessIdentity>,
    game_pid: Option<u32>,
    game_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    termination_handle: Option<usize>,
) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let Some(root_pid) = tracked_pid else {
            return Ok(false);
        };
        let handle_identity = termination_handle.and_then(process_identity_from_termination_handle);
        if tracked_identity
            .zip(handle_identity)
            .is_some_and(|(tracked, handle)| tracked != handle)
        {
            return Err(
                "force_quit_target_stale: retained process handle identity mismatch".to_string(),
            );
        }
        let root_identity = tracked_identity.or(handle_identity);
        let root_identity_matches = root_identity
            .is_some_and(|identity| launcher_identity_matches_snapshot(root_pid, identity));
        let retained_handle_matches_root = termination_handle.is_some()
            && handle_identity.is_some()
            && handle_identity == root_identity;
        let resolved_game_pid = find_launcher_game_process_id_with_identity(
            root_pid,
            root_identity,
            expected_executable,
        )
        .or_else(|| {
            game_pid.filter(|pid| {
                is_launcher_owned_game_process(
                    root_pid,
                    root_identity,
                    *pid,
                    game_identity,
                    expected_executable,
                )
            })
        })
        .or_else(|| {
            (root_identity_matches
                && (retained_handle_matches_root
                    || is_verified_game_pid(root_pid, expected_executable)))
            .then_some(root_pid)
        });
        let Some(resolved_game_pid) = resolved_game_pid else {
            return Err(
                "force_quit_target_not_verified: launcher-owned game process tidak ditemukan"
                    .to_string(),
            );
        };
        let resolved_identity = if game_pid == Some(resolved_game_pid) {
            game_identity
        } else if resolved_game_pid == root_pid {
            root_identity
        } else {
            None
        };
        let root_handle = (resolved_game_pid == root_pid && retained_handle_matches_root)
            .then_some(termination_handle)
            .flatten();
        return terminate_verified_game_tree(
            resolved_game_pid,
            resolved_identity,
            expected_executable,
            root_handle.is_some(),
            root_handle,
        );
    }

    #[cfg(not(windows))]
    {
        let _ = (
            tracked_pid,
            tracked_identity,
            game_pid,
            game_identity,
            expected_executable,
            termination_handle,
        );
        Ok(false)
    }
}

pub fn force_quit_game_with_identity(
    tracked_pid: Option<u32>,
    tracked_identity: Option<ProcessIdentity>,
    expected_executable: Option<&Path>,
    termination_handle: Option<usize>,
) -> Result<bool, String> {
    force_quit_game_with_ownership(
        tracked_pid,
        tracked_identity,
        None,
        None,
        expected_executable,
        termination_handle,
    )
}

pub fn force_quit_game_with_pid(
    tracked_pid: Option<u32>,
    expected_executable: Option<&Path>,
) -> Result<bool, String> {
    force_quit_game_with_identity(tracked_pid, None, expected_executable, None)
}

pub fn force_quit_game() -> Result<bool, String> {
    force_quit_game_with_pid(None, None)
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

        assert_eq!(
            command.executable,
            PathBuf::from(r"C:\Games\Client-Win64-Shipping.exe")
        );
        assert_eq!(command.working_directory, PathBuf::from(r"C:\Games"));
        assert_eq!(command.arguments, vec!["-dx11"]);

        let combined = LaunchCommand::new_with_options(
            Path::new(r"C:\Games\Client-Win64-Shipping.exe"),
            Path::new(r"C:\Games"),
            true,
            true,
        );
        assert_eq!(
            combined.arguments,
            vec!["-dx11", "-ForceEnableCSharpEnvironment"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn direct_process_try_wait_does_not_join_inherited_pipes() {
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
        let shell_path = PathBuf::from(shell);
        let mut command = LaunchCommand::new(&shell_path, temp.path(), false);
        command.arguments = vec![
            "/C".to_string(),
            "start".to_string(),
            "".to_string(),
            "/B".to_string(),
            shell_path.display().to_string(),
            "/C".to_string(),
            "timeout /T 2 /NOBREAK >NUL".to_string(),
        ];
        let managed = spawn_direct(&command).unwrap();
        let pid = match &managed {
            ManagedProcess::Direct(process) => process.child.id(),
            #[cfg(windows)]
            ManagedProcess::Elevated(_) => unreachable!(),
        };
        let mut launched = LaunchedGame {
            pid,
            mode: LaunchMode::Direct,
            process: managed,
        };
        let started = Instant::now();
        let deadline = started + Duration::from_secs(5);
        loop {
            if launched.try_wait().unwrap().is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "root process did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(started.elapsed() < Duration::from_secs(2));
        let _ = launched.finalize();
    }

    #[test]
    fn process_tree_matching_requires_verified_ancestry() {
        let parents = [(99, 42), (123, 99), (777, 500)];

        assert!(process_tree_contains(42, 42, &parents));
        assert!(process_tree_contains(42, 99, &parents));
        assert!(process_tree_contains(42, 123, &parents));
        assert!(!process_tree_contains(42, 777, &parents));
        assert!(!process_tree_contains(42, 500, &parents));
    }

    #[test]
    fn owned_runtime_state_survives_child_handoff_without_claiming_external_games() {
        assert_eq!(
            reconcile_runtime_state_with_owned(Some(42), Some(99), Some(99)),
            RuntimeState {
                active: true,
                origin: ProcessOrigin::Launcher,
            }
        );
        assert_eq!(
            reconcile_runtime_state_with_owned(None, Some(99), None),
            RuntimeState {
                active: true,
                origin: ProcessOrigin::External,
            }
        );
        assert_eq!(
            reconcile_runtime_state_with_owned(Some(42), None, Some(99)),
            RuntimeState {
                active: true,
                origin: ProcessOrigin::Launcher,
            }
        );
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
        assert_eq!(classify_spawn_error(Some(2)), SpawnFailureKind::SpawnFailed);
    }

    #[test]
    fn launch_failure_message_is_compact_and_keeps_evidence_local() {
        let command = LaunchCommand::new(
            Path::new(r"C:\Games\Client-Win64-Shipping.exe"),
            Path::new(r"C:\Games"),
            false,
        );
        let evidence =
            LaunchEvidence::for_failure(command, SpawnFailureKind::ElevationRequired, None);
        let message = evidence.user_message();

        assert!(message.contains("elevation_required"));
        assert!(message.contains("exit_code=none"));
        assert!(message.contains("reason=Administrator permission required"));
        assert!(message.contains("evidence_path=none"));
        assert!(!message.contains("game_log_tail"));
        assert!(!message.contains("stderr="));
        assert!(!message.contains("stdout="));
    }

    #[test]
    fn compact_failure_detail_handles_unicode_without_panicking() {
        let detail = compact_detail(&"é".repeat(200));
        assert!(detail.chars().count() <= 180);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn output_tail_keeps_only_the_last_8_kib() {
        let output = "a".repeat(10 * 1024);
        let tail = bounded_output_tail(output.as_bytes());

        assert_eq!(tail.len(), 8 * 1024);
        assert!(tail.chars().all(|value| value == 'a'));
    }

    #[test]
    fn captured_process_output_is_bounded_while_the_process_runs() {
        let output = vec![b'a'; MAX_OUTPUT_TAIL_BYTES + 1024];
        let captured = capture_stream(std::io::Cursor::new(output)).join().unwrap();

        assert_eq!(captured.len(), MAX_OUTPUT_TAIL_BYTES);
        assert!(captured.iter().all(|value| *value == b'a'));
    }

    #[test]
    fn game_log_capture_reads_only_the_last_8_kib() {
        let directory = tempfile::tempdir().unwrap();
        let mut content = vec![b'a'; 1024];
        content.extend(std::iter::repeat_n(b'b', MAX_OUTPUT_TAIL_BYTES));
        std::fs::write(directory.path().join("game.log"), content).unwrap();

        let tail = collect_game_log_tail(directory.path());

        assert!(tail.ends_with(&"b".repeat(MAX_OUTPUT_TAIL_BYTES)));
        assert!(!tail.ends_with(&"a".repeat(1024)));
    }
}
