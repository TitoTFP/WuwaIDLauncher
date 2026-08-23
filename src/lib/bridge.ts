import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CleanupReport,
  InstallMethod,
  LauncherUpdatePayload,
  LauncherUpdateStatusPayload,
  MediaProgressPayload,
  MediaReadyPayload,
  MediaStatusPayload,
  PatchStatusPayload,
  ProgressPayload,
  ReleaseNotePayload,
  SettingsLoadResult,
} from "./types";

export const bridge = {
  // Window controls
  minimizeWindow: (): Promise<void> => invoke("minimize_window"),
  closeWindow: (): Promise<void> => invoke("close_window"),

  // Game & Configuration
  isGameRunning: (): Promise<boolean> => invoke("is_game_running"),
  browseGameFolder: (): Promise<string> => invoke("browse_game_folder"),
  saveSettings: (settingsJson: string): Promise<void> =>
    invoke("save_settings", { settingsJson }),
  loadSettings: (): Promise<SettingsLoadResult> => invoke("load_settings"),
  getAppVersion: (): Promise<string> => invoke("get_app_version"),
  getVhVersion: (): Promise<string> => invoke("get_vh_version"),

  // Media & Assets
  checkAndSyncMedia: (): Promise<void> => invoke("check_and_sync_media"),

  // Update & Release Notes
  checkLauncherUpdate: (): Promise<void> => invoke("check_launcher_update"),
  getVhReleaseNotes: (): Promise<void> => invoke("get_vh_release_notes"),
  getLauncherReleaseNotes: (): Promise<void> => invoke("get_launcher_release_notes"),
  performLauncherUpdate: (version: string): Promise<void> =>
    invoke("perform_launcher_update", { version }),

  // Patch Management
  checkPatchStatus: (gamePath: string, installMethod: InstallMethod): Promise<void> =>
    invoke("check_patch_status", { gamePath, installMethod }),
  switchMethod: (gamePath: string, newMethod: InstallMethod): Promise<CleanupReport> =>
    invoke("switch_method", { gamePath, newMethod }),
  startInstallation: (
    gamePath: string,
    vhMode: string,
    installMethod: InstallMethod,
  ): Promise<void> =>
    invoke("start_installation", { gamePath, vhMode, installMethod }),
  checkGameFolderWriteAccess: (
    gamePath: string,
    installMethod: InstallMethod,
    forInstallation: boolean,
  ): Promise<string> =>
    invoke("check_game_folder_write_access", {
      gamePath,
      installMethod,
      forInstallation,
    }),
  uninstall: (gamePath: string): Promise<string> =>
    invoke("uninstall", { gamePath }),

  // Launch & Process
  launchGame: (
    gamePath: string,
    dx11: boolean,
    installMethod: InstallMethod,
  ): Promise<void> => invoke("launch_game", { gamePath, dx11, installMethod }),
  forceQuitGame: (): Promise<boolean> => invoke("force_quit_game"),
  restartAsAdmin: (): Promise<void> => invoke("restart_as_admin"),
  openSupport: (): Promise<void> => invoke("open_support"),

  // Local UI maintenance
  notifyUiInteractive: (installMethod: InstallMethod): Promise<void> =>
    invoke("notify_ui_interactive", { installMethod }),
  resetWebViewCache: (): Promise<void> => invoke("reset_webview_cache"),

};

// Event listener helper for Tauri events
export interface EventBridgeCallbacks {
  onGameRuntimeState?: (
    active: boolean,
    origin: "launcher" | "external",
  ) => void;
  onLauncherTrayState?: (inTray: boolean) => void;
  onPatchStatus?: (payload: PatchStatusPayload) => void;
  onProgressUpdate?: (payload: ProgressPayload) => void;
  onInstallComplete?: () => void;
  onInstallError?: (error: string) => void;
  onLaunchError?: (error: string) => void;
  onGameLaunchStarted?: () => void;
  onGameLaunchFinished?: () => void;
  onLauncherUpdateProgress?: (percent: number, statusText: string) => void;
  onLauncherUpdateAvailable?: (payload: LauncherUpdatePayload) => void;
  onLauncherUpdateStatus?: (payload: LauncherUpdateStatusPayload) => void;
  onLauncherUpdateStaged?: () => void;
  onLauncherUpdateRestarting?: (remainingSeconds: number) => void;
  onLauncherUpdateError?: (error: string) => void;
  onMediaReady?: (payload: MediaReadyPayload) => void;
  onMediaStatus?: (payload: MediaStatusPayload) => void;
  onMediaProgress?: (payload: MediaProgressPayload) => void;
  onUpdateDate?: (dateStr: string) => void;
  onVHReleaseNotes?: (payload: ReleaseNotePayload) => void;
  onLauncherReleaseNotes?: (payload: ReleaseNotePayload) => void;
}

function restartSecondsFromPayload(payload: unknown): number | null {
  if (!payload || typeof payload !== "object") return null;
  const remainingSeconds = (payload as { remainingSeconds?: unknown })
    .remainingSeconds;
  if (
    typeof remainingSeconds !== "number" ||
    !Number.isFinite(remainingSeconds) ||
    remainingSeconds < 0
  ) {
    return null;
  }
  return Math.floor(remainingSeconds);
}

export async function setupEventBridge(
  callbacks: EventBridgeCallbacks,
): Promise<UnlistenFn[]> {
  const unlisteners: UnlistenFn[] = [];
  let setupFailed = false;

  const addListener = async <T>(
    event: string,
    handler?: (payload: T) => void,
  ) => {
    if (!handler) return;
    const unlisten = await listen<T>(event, (e) => handler(e.payload));
    if (setupFailed) unlisten();
    else unlisteners.push(unlisten);
  };

  await Promise.all([
    addListener<{ active: boolean; origin: "launcher" | "external" }>(
      "onGameRuntimeState",
      (p) => callbacks.onGameRuntimeState?.(p.active, p.origin),
    ),
    addListener<{ inTray: boolean }>("onLauncherTrayState", (p) =>
      callbacks.onLauncherTrayState?.(p.inTray),
    ),
    addListener<PatchStatusPayload>("onPatchStatus", (p) =>
      callbacks.onPatchStatus?.(p),
    ),
    addListener<ProgressPayload>("onProgressUpdate", (p) =>
      callbacks.onProgressUpdate?.(p),
    ),
    addListener<void>("onInstallComplete", () =>
      callbacks.onInstallComplete?.(),
    ),
    addListener<string>("onInstallError", (err) =>
      callbacks.onInstallError?.(err),
    ),
    addListener<string>("onLaunchError", (err) =>
      callbacks.onLaunchError?.(err),
    ),
    addListener<void>("onGameLaunchStarted", () =>
      callbacks.onGameLaunchStarted?.(),
    ),
    addListener<void>("onGameLaunchFinished", () =>
      callbacks.onGameLaunchFinished?.(),
    ),
    addListener<{ percent: number; status: string }>(
      "onLauncherUpdateProgress",
      (p) => callbacks.onLauncherUpdateProgress?.(p.percent, p.status),
    ),
    addListener<LauncherUpdatePayload>("onLauncherUpdateAvailable", (p) =>
      callbacks.onLauncherUpdateAvailable?.(p),
    ),
    addListener<LauncherUpdateStatusPayload>("onLauncherUpdateStatus", (p) =>
      callbacks.onLauncherUpdateStatus?.(p),
    ),
    addListener<void>("onLauncherUpdateStaged", () =>
      callbacks.onLauncherUpdateStaged?.(),
    ),
    addListener<unknown>("onLauncherUpdateRestarting", (p) => {
      const remainingSeconds = restartSecondsFromPayload(p);
      if (remainingSeconds !== null) {
        callbacks.onLauncherUpdateRestarting?.(remainingSeconds);
      }
    }),
    addListener<string>("onLauncherUpdateError", (error) =>
      callbacks.onLauncherUpdateError?.(error),
    ),
    addListener<MediaReadyPayload>("onMediaReady", (p) =>
      callbacks.onMediaReady?.(p),
    ),
    addListener<MediaStatusPayload>("onMediaStatus", (p) =>
      callbacks.onMediaStatus?.(p),
    ),
    addListener<MediaProgressPayload>("onMediaProgress", (p) =>
      callbacks.onMediaProgress?.(p),
    ),
    addListener<string>("onUpdateDate", (p) => callbacks.onUpdateDate?.(p)),
    addListener<ReleaseNotePayload>("onVHReleaseNotes", (p) =>
      callbacks.onVHReleaseNotes?.(p),
    ),
    addListener<ReleaseNotePayload>("onLauncherReleaseNotes", (p) =>
      callbacks.onLauncherReleaseNotes?.(p),
    ),
  ]).catch((error) => {
    setupFailed = true;
    for (const unlisten of unlisteners) unlisten();
    throw error;
  });

  return unlisteners;
}
