use crate::engine::{
    method::InstallMethod,
    patch_status::{self, LocalPatchState},
    path::{self, get_binary_dir, GAME_EXE_RELATIVE},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};

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

pub fn launch_game(game_path: &Path, dx11: bool) -> Result<std::process::Child, String> {
    let exe = game_path.join(GAME_EXE_RELATIVE);
    if !exe.exists() {
        return Err(format!("Executable file not found: {:?}", exe));
    }

    let work_dir = get_binary_dir(game_path);
    let mut cmd = Command::new(&exe);
    cmd.current_dir(work_dir);

    if dx11 {
        cmd.arg("-dx11");
    }

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn game process: {}", e))
}

pub fn force_quit_game() -> Result<bool, String> {
    #[cfg(windows)]
    {
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

    #[test]
    fn test_is_game_running_smoke() {
        // Should not panic
        let _ = is_game_running();
    }

    #[test]
    fn test_trim_working_set_smoke() {
        trim_memory_working_set();
    }
}
