use std::sync::{Arc, Mutex, OnceLock};

/// Operations which can change launcher or game files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    PatchInstall,
    MethodSwitch,
    Uninstall,
    LauncherUpdate,
    CacheReset,
    MediaSync,
    GameLaunch,
    ForceQuit,
}

impl OperationKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::PatchInstall => "patch installation",
            Self::MethodSwitch => "method switch",
            Self::Uninstall => "uninstall",
            Self::LauncherUpdate => "launcher update",
            Self::CacheReset => "cache reset",
            Self::MediaSync => "media sync",
            Self::GameLaunch => "game launch",
            Self::ForceQuit => "force quit",
        }
    }

    pub const fn blocks_close(self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
struct OperationState {
    active: Vec<OperationKind>,
    closing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OperationCoordinator {
    state: Arc<Mutex<OperationState>>,
}

#[derive(Debug)]
pub struct OperationGuard {
    coordinator: OperationCoordinator,
    kind: OperationKind,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut state = self
            .coordinator
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = state.active.iter().position(|kind| *kind == self.kind) {
            state.active.remove(index);
        }
    }
}

fn operations_conflict(left: OperationKind, right: OperationKind) -> bool {
    if left == right {
        return true;
    }

    match (left, right) {
        // Media downloads use a separate cache and are deliberately allowed to
        // run alongside a patch install.  A cache reset must never race them.
        (OperationKind::MediaSync, OperationKind::PatchInstall)
        | (OperationKind::PatchInstall, OperationKind::MediaSync) => false,
        (OperationKind::MediaSync, OperationKind::GameLaunch)
        | (OperationKind::GameLaunch, OperationKind::MediaSync) => false,
        (OperationKind::GameLaunch, OperationKind::PatchInstall)
        | (OperationKind::PatchInstall, OperationKind::GameLaunch)
        | (OperationKind::GameLaunch, OperationKind::MethodSwitch)
        | (OperationKind::MethodSwitch, OperationKind::GameLaunch)
        | (OperationKind::GameLaunch, OperationKind::Uninstall)
        | (OperationKind::Uninstall, OperationKind::GameLaunch) => true,
        (OperationKind::GameLaunch, OperationKind::ForceQuit)
        | (OperationKind::ForceQuit, OperationKind::GameLaunch) => false,
        _ => true,
    }
}

impl OperationCoordinator {
    pub fn try_acquire(&self, kind: OperationKind) -> Result<OperationGuard, String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if state.closing {
            return Err("busy: launcher is closing".to_string());
        }

        if let Some(active) = state
            .active
            .iter()
            .copied()
            .find(|active| operations_conflict(*active, kind))
        {
            return Err(format!(
                "busy: {} is already in progress; cannot start {}",
                active.label(),
                kind.label()
            ));
        }

        state.active.push(kind);
        Ok(OperationGuard {
            coordinator: self.clone(),
            kind,
        })
    }

    pub fn request_close(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closing {
            return Err("busy: launcher close is already in progress".to_string());
        }
        if let Some(active) = state
            .active
            .iter()
            .copied()
            .find(|active| active.blocks_close())
        {
            return Err(format!(
                "busy: cannot close while {} is in progress",
                active.label()
            ));
        }
        state.closing = true;
        Ok(())
    }

    /// Allows the launcher-update guard itself to remain held while the close
    /// transition is committed, eliminating the race between dropping that
    /// guard and requesting application shutdown.
    pub fn request_close_for_launcher_update(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closing {
            return Err("busy: launcher close is already in progress".to_string());
        }
        if !state.active.contains(&OperationKind::LauncherUpdate) {
            return Err("busy: launcher update is not in progress".to_string());
        }
        if let Some(active) = state
            .active
            .iter()
            .copied()
            .find(|active| *active != OperationKind::LauncherUpdate)
        {
            return Err(format!(
                "busy: cannot close while {} is in progress",
                active.label()
            ));
        }
        state.closing = true;
        Ok(())
    }

    pub fn request_close_for_tray(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closing {
            return Err("busy: launcher close is already in progress".to_string());
        }
        if let Some(active) = state
            .active
            .iter()
            .copied()
            .find(|active| *active != OperationKind::GameLaunch)
        {
            return Err(format!(
                "busy: cannot close while {} is in progress",
                active.label()
            ));
        }
        state.closing = true;
        Ok(())
    }

    pub fn active_operation(&self) -> Option<OperationKind> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active
            .first()
            .copied()
    }
}

static GLOBAL_COORDINATOR: OnceLock<OperationCoordinator> = OnceLock::new();

pub fn global() -> OperationCoordinator {
    GLOBAL_COORDINATOR
        .get_or_init(OperationCoordinator::default)
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_and_media_can_coexist_but_cache_reset_cannot() {
        let coordinator = OperationCoordinator::default();
        let patch = coordinator
            .try_acquire(OperationKind::PatchInstall)
            .unwrap();
        let media = coordinator.try_acquire(OperationKind::MediaSync).unwrap();

        let error = coordinator
            .try_acquire(OperationKind::CacheReset)
            .unwrap_err();
        assert!(error.starts_with("busy:"));
        assert!(error.contains("patch installation"));

        drop(media);
        drop(patch);
        assert!(coordinator.try_acquire(OperationKind::CacheReset).is_ok());
    }

    #[test]
    fn destructive_operations_conflict_and_guards_release() {
        let coordinator = OperationCoordinator::default();
        let switch = coordinator
            .try_acquire(OperationKind::MethodSwitch)
            .unwrap();
        let error = coordinator
            .try_acquire(OperationKind::Uninstall)
            .unwrap_err();
        assert!(error.contains("method switch"));
        drop(switch);
        assert!(coordinator.try_acquire(OperationKind::Uninstall).is_ok());
    }

    #[test]
    fn close_reports_busy_and_blocks_new_operations_after_success() {
        let coordinator = OperationCoordinator::default();
        let install = coordinator
            .try_acquire(OperationKind::PatchInstall)
            .unwrap();
        assert!(coordinator.request_close().unwrap_err().contains("busy:"));
        drop(install);
        coordinator.request_close().unwrap();
        assert!(coordinator
            .try_acquire(OperationKind::MediaSync)
            .unwrap_err()
            .contains("closing"));
    }

    #[test]
    fn close_reports_busy_during_game_launch() {
        let coordinator = OperationCoordinator::default();
        let launch = coordinator.try_acquire(OperationKind::GameLaunch).unwrap();

        let error = coordinator.request_close().unwrap_err();
        assert!(error.contains("game launch"));
        drop(launch);
        assert!(coordinator.request_close().is_ok());
    }

    #[test]
    fn launcher_update_can_commit_close_without_dropping_its_guard() {
        let coordinator = OperationCoordinator::default();
        let update = coordinator
            .try_acquire(OperationKind::LauncherUpdate)
            .unwrap();

        coordinator.request_close_for_launcher_update().unwrap();
        assert!(coordinator
            .try_acquire(OperationKind::MediaSync)
            .unwrap_err()
            .contains("closing"));
        drop(update);
    }

    #[test]
    fn tray_close_allows_game_launch_but_still_blocks_other_operations() -> Result<(), String> {
        let coordinator = OperationCoordinator::default();
        let launch = coordinator.try_acquire(OperationKind::GameLaunch)?;
        assert!(coordinator.request_close_for_tray().is_ok());
        drop(launch);

        let coordinator = OperationCoordinator::default();
        let media = coordinator.try_acquire(OperationKind::MediaSync)?;
        let error = match coordinator.request_close_for_tray() {
            Ok(()) => return Err("media sync should block tray close".to_string()),
            Err(error) => error,
        };
        assert!(error.contains("media sync"));
        drop(media);
        Ok(())
    }

    #[test]
    fn force_quit_can_interrupt_game_launch() {
        let coordinator = OperationCoordinator::default();
        let launch = coordinator.try_acquire(OperationKind::GameLaunch).unwrap();
        assert!(coordinator.try_acquire(OperationKind::ForceQuit).is_ok());
        drop(launch);
    }
}
