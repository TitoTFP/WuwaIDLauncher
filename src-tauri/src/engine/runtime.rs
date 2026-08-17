use crate::engine::path::*;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn is_game_running() -> bool {
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
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    false
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

pub fn force_quit_game() {
    #[cfg(windows)]
    {
        let names = ["Client-Win64-Shipping.exe", "WutheringWaves.exe"];
        for name in names {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
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
