use super::installer;
use super::method::InstallMethod;
use super::updater::{is_newer_version, parse_version};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPatchState {
    NotInstalled,
    Invalid,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchStatus {
    NotInstalled,
    NeedsUpdate,
    Ready,
    Invalid,
}

impl PatchStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::NeedsUpdate => "needs_update",
            Self::Ready => "ready",
            Self::Invalid => "invalid",
        }
    }
}

pub fn classify_installation(
    game_path: &Path,
    method: InstallMethod,
) -> Result<LocalPatchState, String> {
    match method {
        InstallMethod::ResourceMount => {
            let plan = match installer::probe_resource_mount(game_path) {
                Ok(plan) => plan,
                Err(_) => return Ok(LocalPatchState::NotInstalled),
            };
            let any_artifact = [
                &plan.pak_path,
                &plan.sig_path,
                &plan.owner_marker_path,
                &plan.mount_path,
            ]
            .iter()
            .any(|path| path.exists());
            if !any_artifact {
                return Ok(LocalPatchState::NotInstalled);
            }
            Ok(if installer::validate_installed_resource_mount(&plan)? {
                LocalPatchState::Ready
            } else {
                LocalPatchState::Invalid
            })
        }
        InstallMethod::Loader => {
            let any_artifact = [
                installer::loader_pak_path(game_path),
                installer::loader_dll_path(game_path),
                installer::loader_marker_path(game_path),
            ]
            .iter()
            .any(|path| path.exists());
            if !any_artifact {
                return Ok(LocalPatchState::NotInstalled);
            }
            Ok(if installer::validate_installed_loader(game_path)? {
                LocalPatchState::Ready
            } else {
                LocalPatchState::Invalid
            })
        }
        InstallMethod::SignatureBypass => {
            let any_artifact = [
                installer::signature_bypass_pak_path(game_path),
                installer::signature_bypass_marker_path(game_path),
            ]
            .iter()
            .any(|path| path.exists());
            if !any_artifact {
                return Ok(LocalPatchState::NotInstalled);
            }
            Ok(
                if installer::validate_installed_signature_bypass(game_path)? {
                    LocalPatchState::Ready
                } else {
                    LocalPatchState::Invalid
                },
            )
        }
    }
}

fn is_newer_release(current: &str, latest: &str) -> bool {
    let current = current.trim().trim_start_matches('v');
    let latest = latest.trim().trim_start_matches('v');
    if current.is_empty() || matches!(current, "latest" | "unknown") {
        return true;
    }
    if is_newer_version(current, latest) {
        return true;
    }
    parse_version(current) == parse_version(latest) && latest > current
}

pub fn resolve_patch_status(
    local: LocalPatchState,
    current_version: Option<&str>,
    latest_version: Option<&str>,
) -> PatchStatus {
    match local {
        LocalPatchState::NotInstalled => PatchStatus::NotInstalled,
        LocalPatchState::Invalid => PatchStatus::NeedsUpdate,
        LocalPatchState::Ready => {
            if let Some(latest) = latest_version {
                if current_version
                    .map(|current| is_newer_release(current, latest))
                    .unwrap_or(true)
                {
                    return PatchStatus::NeedsUpdate;
                }
            }
            PatchStatus::Ready
        }
    }
}
