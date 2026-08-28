export type InstallMethod = "resource_mount" | "loader";

export type LauncherOperation =
  | "install"
  | "launch"
  | "uninstall"
  | "method-switch"
  | "folder"
  | "cache-reset"
  | "media-sync"
  | "force-quit"
  | "launcher-update"
  | "restart-as-admin"
  | "close";

export interface OperationToken {
  readonly id: number;
  readonly kind: LauncherOperation;
}

export interface CleanupReport {
  removed: string[];
  preserved: string[];
  failures: string[];
}

export interface InstallMethodOption {
  value: InstallMethod;
  title: string;
  description: string;
}

export const INSTALL_METHOD_OPTIONS: readonly InstallMethodOption[] = [
  {
    value: "resource_mount",
    title: "Metode 1",
    description: "Resource Mount · instalasi terisolasi",
  },
  {
    value: "loader",
    title: "Metode 2",
    description: "winhttp.dll loader",
  },
];

export type ToastKind = "ok" | "err" | "info";

export interface ToastMessage {
  id: number;
  message: string;
  kind: ToastKind;
}

export type PatchState =
  | "unchecked"
  | "ready"
  | "needs_update"
  | "not_installed"
  | "invalid"
  | "error";

export interface LauncherConfig {
  gamePath: string;
  installMethod: InstallMethod;
  dx11: boolean;
  csharpEnvironment: boolean;
  hideUid: boolean;
  bgmVolume: number;
  bgmEnabled: boolean;
}

export const DEFAULT_LAUNCHER_CONFIG: LauncherConfig = {
  gamePath: "",
  installMethod: "resource_mount",
  dx11: false,
  csharpEnvironment: false,
  hideUid: false,
  bgmVolume: 0.35,
  bgmEnabled: true,
};

export interface SettingsLoadResult {
  settings: LauncherConfig;
  repaired: boolean;
  diagnostics: string[];
}

export interface NormalizedConfigResult {
  config: LauncherConfig;
  repaired: boolean;
  diagnostics: string[];
}

export function normalizeInstallMethod(value: unknown): InstallMethod {
  switch (value) {
    case "resource_mount":
    case "method3":
      return "resource_mount";
    case "loader":
    case "method2":
      return "loader";
    default:
      return DEFAULT_LAUNCHER_CONFIG.installMethod;
  }
}

export function normalizeLauncherConfig(raw: unknown): NormalizedConfigResult {
  const value =
    raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const diagnostics: string[] = [];
  let repaired = raw === null || typeof raw !== "object";
  const config: LauncherConfig = { ...DEFAULT_LAUNCHER_CONFIG };

  if (typeof value.gamePath === "string") config.gamePath = value.gamePath;
  else if ("gamePath" in value) {
    repaired = true;
    diagnostics.push("Path game tidak valid dan dikosongkan.");
  }

  const method = normalizeInstallMethod(value.installMethod);
  if (
    "installMethod" in value &&
    !["resource_mount", "loader", "method2", "method3"].includes(
      value.installMethod as string,
    )
  ) {
    repaired = true;
    diagnostics.push("Metode instalasi tidak valid; memakai resource_mount.");
  }
  config.installMethod = method;

  if ("launcherVisualMode" in value || "perf" in value) {
    repaired = true;
    diagnostics.push(
      "Pengaturan performa lama dihapus; launcher memakai mode Penuh.",
    );
  }

  if ("autoCheckUpdate" in value) {
    repaired = true;
    diagnostics.push(
      "Pemeriksaan update otomatis selalu aktif; pengaturan lama dihapus.",
    );
  }

  for (const [key, fallback] of [
    ["dx11", config.dx11],
    ["csharpEnvironment", config.csharpEnvironment],
    ["hideUid", config.hideUid],
    ["bgmEnabled", config.bgmEnabled],
  ] as const) {
    if (typeof value[key] === "boolean") config[key] = value[key] as boolean;
    else if (key in value) {
      config[key] = fallback;
      repaired = true;
      diagnostics.push(`Field settings ${key} tidak valid; memakai default.`);
    }
  }

  if (typeof value.bgmVolume === "number" && Number.isFinite(value.bgmVolume)) {
    config.bgmVolume = Math.min(1, Math.max(0, value.bgmVolume));
    if (config.bgmVolume !== value.bgmVolume) {
      repaired = true;
      diagnostics.push("Volume BGM berada di luar 0..1 dan telah dibatasi.");
    }
  } else if ("bgmVolume" in value) {
    repaired = true;
    diagnostics.push("Field settings bgmVolume tidak valid; memakai default.");
  }

  return { config, repaired, diagnostics };
}

export interface ProgressPayload {
  percent: number;
  status: string;
  downloadedBytes?: number;
  totalBytes?: number;
  speedMbps?: number;
}

export interface PatchStatusPayload {
  status: PatchState;
  gamePath: string;
  installMethod: InstallMethod;
  hideUid: boolean;
  currentVersion?: string;
  latestVersion?: string;
  message?: string;
}

export interface LauncherUpdateStatusPayload {
  kind: "ok" | "info";
  message: string;
}

export interface GameExitPayload {
  id: string;
  status: "normal" | "crashed" | "force_quit";
  reason: string;
}

export interface MediaReadyPayload {
  bgmUrl: string;
  videoUrl: string;
}

export interface MediaProgressPayload {
  percent: number;
  text: string;
  speed: number;
  size: string;
}

export interface MediaStatusPayload {
  status: "checking" | "downloading" | "ready" | "offline" | "error";
  message: string;
}

export interface ReleaseNotePayload {
  tag: string;
  date: string;
  title: string;
  body: string;
  author: string;
}

export function launcherReleaseNotesSeenStorageKey(tag: string): string {
  return `wuwaid-launcher.launcher-release-notes-seen.${encodeURIComponent(tag.trim())}`;
}

export interface LauncherUpdatePayload extends ReleaseNotePayload {
  version: string;
}

export interface ILauncherState {
  installing: boolean;
  installed: boolean;
  launching: boolean;
  gameRunning: boolean;
  launcherInTray: boolean;
  gameOrigin: "launcher" | "external";
  gamePath: string;
  patchState: PatchState;
  patchStatusCheckPending: boolean;
  progressPercent: number;
  progressStatus: string;
  progressDownloadedBytes: number;
  progressTotalBytes: number;
  progressSpeedMbps: number;
  appVersion: string;
  vhVersion: string;
  statusMessage: string;
  diagnosticMessage: string;
  mediaStatus: MediaStatusPayload["status"] | "";
  mediaStatusMessage: string;
  mediaProgress: MediaProgressPayload | null;
  launcherUpdateProgress: number;
  launcherUpdateStatus: string;
  launcherUpdateError: string;
  launcherUpdateRestartCountdown: number | null;
  configSavePending: boolean;
  bgmPlaying: boolean;
  bgmVolume: number;
  bgmUrl: string;
  videoUrl: string;
  updateDate: string;
  releaseNotes: ReleaseNotePayload | null;
  releaseNotesLoading: boolean;
  firstLaunchLauncherReleaseNotes: ReleaseNotePayload | null;
  launcherUpdateAvailable: boolean;
  launcherUpdatePayload: LauncherUpdatePayload | null;
  toasts: ToastMessage[];
  adminPromptOpen: boolean;
  adminPromptPath: string;
  config: LauncherConfig;
  setStatus(message: string, diagnostic?: string): void;
  clearStatus(): void;
  showToast(message: string, kind?: ToastKind): void;
  openAdminPrompt(path: string): void;
  closeAdminPrompt(): void;
  dismissLauncherUpdate(): void;
  dismissFirstLaunchLauncherReleaseNotes(): void;
  init(): Promise<void>;
  dispose(): void;
  saveConfig(): Promise<void>;
  beginOperation(kind: LauncherOperation): OperationToken | null;
  endOperation(token: OperationToken): void;
  isOperationBlocked(kind: LauncherOperation): boolean;
  getOperationBusyMessage(kind: LauncherOperation): string;
  setGamePath(path: string): void;
  selectGameFolder(): Promise<boolean>;
  switchInstallMethod(method: InstallMethod): Promise<void>;
  requestPatchStatus(
    gamePath: string,
    installMethod: InstallMethod,
    manualCheck?: boolean,
  ): Promise<void>;
  invalidatePatchStatus(): void;
  startMediaSync(): Promise<void>;
  forceQuitGame(): Promise<boolean>;
  resetWebViewCache(): Promise<void>;
  performLauncherUpdate(version: string): Promise<void>;
  restartAsAdmin(): Promise<void>;
}
