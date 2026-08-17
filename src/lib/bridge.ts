import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  LauncherUpdatePayload,
  LogUploadResult,
  MediaProgressPayload,
  MediaReadyPayload,
  MediaStatusPayload,
  PatchStatusPayload,
  ProgressPayload,
  ReleaseNotePayload,
} from "./types";

export const bridge = {
  // Window controls
  minimizeWindow: (): Promise<void> => invoke("minimize_window"),
  closeWindow: (): Promise<void> => invoke("close_window"),

  // Game & Settings
  isGameRunning: (): Promise<boolean> => invoke("is_game_running"),
  browseGameFolder: (): Promise<string> => invoke("browse_game_folder"),
  saveSettings: (settingsJson: string): Promise<void> =>
    invoke("save_settings", { settingsJson }),
  loadSettings: (): Promise<string> => invoke("load_settings"),
  getAppVersion: (): Promise<string> => invoke("get_app_version"),
  getVhVersion: (): Promise<string> => invoke("get_vh_version"),

  // Media & Assets
  checkAndSyncMedia: (): Promise<void> => invoke("check_and_sync_media"),

  // Update & Release Notes
  checkLauncherUpdate: (): Promise<void> => invoke("check_launcher_update"),
  getVhReleaseNotes: (): Promise<void> => invoke("get_vh_release_notes"),
  performLauncherUpdate: (version: string, zipUrl: string): Promise<void> =>
    invoke("perform_launcher_update", { version, zipUrl }),

  // Patch Management
  checkPatchStatus: (gamePath: string, installMethod: string): Promise<void> =>
    invoke("check_patch_status", { gamePath, installMethod }),
  switchMethod: (gamePath: string, newMethod: string): Promise<void> =>
    invoke("switch_method", { gamePath, newMethod }),
  startInstallation: (
    gamePath: string,
    vhMode: string,
    backup: boolean,
    installMethod: string,
  ): Promise<void> =>
    invoke("start_installation", { gamePath, vhMode, backup, installMethod }),
  checkGameFolderWriteAccess: (
    gamePath: string,
    installMethod: string,
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
    installMethod: string,
  ): Promise<void> => invoke("launch_game", { gamePath, dx11, installMethod }),
  forceQuitGame: (): Promise<void> => invoke("force_quit_game"),
  restartAsAdmin: (): Promise<void> => invoke("restart_as_admin"),

  // Diagnostic & Telemetry
  getLogUploadEnabled: (): Promise<boolean> => invoke("get_log_upload_enabled"),
  uploadLogs: (gamePath: string): Promise<void> =>
    invoke("upload_logs", { gamePath }),
  notifyUiInteractive: (): Promise<void> => invoke("notify_ui_interactive"),
  resetWebViewCache: (): Promise<void> => invoke("reset_webview_cache"),
};

// Event listener helper for Tauri events
export interface EventBridgeCallbacks {
  onGameRuntimeState?: (
    active: boolean,
    origin: "launcher" | "external",
  ) => void;
  onPatchStatus?: (payload: PatchStatusPayload) => void;
  onProgressUpdate?: (payload: ProgressPayload) => void;
  onInstallComplete?: () => void;
  onInstallError?: (error: string) => void;
  onGameLaunchStarted?: () => void;
  onGameLaunchFinished?: () => void;
  onLogUploadStarted?: () => void;
  onLogUploadFinished?: (result: LogUploadResult) => void;
  onLauncherUpdateProgress?: (percent: number, statusText: string) => void;
  onLauncherUpdateAvailable?: (payload: LauncherUpdatePayload) => void;
  onMediaReady?: (payload: MediaReadyPayload) => void;
  onMediaStatus?: (payload: MediaStatusPayload) => void;
  onMediaProgress?: (payload: MediaProgressPayload) => void;
  onUpdateDate?: (dateStr: string) => void;
  onVHReleaseNotes?: (payload: ReleaseNotePayload) => void;
}

export async function setupEventBridge(
  callbacks: EventBridgeCallbacks,
): Promise<UnlistenFn[]> {
  const unlisteners: UnlistenFn[] = [];

  const addListener = async <T>(
    event: string,
    handler?: (payload: T) => void,
  ) => {
    if (!handler) return;
    const unlisten = await listen<T>(event, (e) => handler(e.payload));
    unlisteners.push(unlisten);
  };

  await Promise.all([
    addListener<{ active: boolean; origin: "launcher" | "external" }>(
      "onGameRuntimeState",
      (p) => callbacks.onGameRuntimeState?.(p.active, p.origin),
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
    addListener<void>("onGameLaunchStarted", () =>
      callbacks.onGameLaunchStarted?.(),
    ),
    addListener<void>("onGameLaunchFinished", () =>
      callbacks.onGameLaunchFinished?.(),
    ),
    addListener<void>("onLogUploadStarted", () =>
      callbacks.onLogUploadStarted?.(),
    ),
    addListener<LogUploadResult>("onLogUploadFinished", (res) =>
      callbacks.onLogUploadFinished?.(res),
    ),
    addListener<LauncherUpdatePayload>("onLauncherUpdateAvailable", (p) =>
      callbacks.onLauncherUpdateAvailable?.(p),
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
  ]);

  return unlisteners;
}
