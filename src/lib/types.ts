export type InstallMethod = "resource_mount" | "loader" | "signature_bypass";

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
    description: "Resource Mount · tanpa signature bypass",
  },
  {
    value: "loader",
    title: "Metode 2",
    description: "winhttp.dll loader",
  },
  {
    value: "signature_bypass",
    title: "Metode 3",
    description: "Signature bypass",
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
  autoCheckUpdate: boolean;
  bgmVolume: number;
  bgmEnabled: boolean;
  diagnosticsUploadEnabled: boolean;
  telemetryEnabled: boolean;
}

export const DEFAULT_LAUNCHER_CONFIG: LauncherConfig = {
  gamePath: "",
  installMethod: "resource_mount",
  dx11: false,
  autoCheckUpdate: true,
  bgmVolume: 0.35,
  bgmEnabled: true,
  diagnosticsUploadEnabled: false,
  telemetryEnabled: false,
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
    case "signature_bypass":
    case "method1":
      return "signature_bypass";
    default:
      return DEFAULT_LAUNCHER_CONFIG.installMethod;
  }
}

export function normalizeLauncherConfig(raw: unknown): NormalizedConfigResult {
  const value = raw && typeof raw === "object" ? raw as Record<string, unknown> : {};
  const diagnostics: string[] = [];
  let repaired = raw === null || typeof raw !== "object";
  const config: LauncherConfig = { ...DEFAULT_LAUNCHER_CONFIG };

  if (typeof value.gamePath === "string") config.gamePath = value.gamePath;
  else if ("gamePath" in value) {
    repaired = true;
    diagnostics.push("Path game tidak valid dan dikosongkan.");
  }

  const method = normalizeInstallMethod(value.installMethod);
  if ("installMethod" in value && !["resource_mount", "loader", "signature_bypass", "method1", "method2", "method3"].includes(value.installMethod as string)) {
    repaired = true;
    diagnostics.push("Metode instalasi tidak valid; memakai resource_mount.");
  }
  config.installMethod = method;

  if ("launcherVisualMode" in value || "perf" in value) {
    repaired = true;
    diagnostics.push("Pengaturan performa lama dihapus; launcher memakai mode Penuh.");
  }

  for (const [key, fallback] of [["dx11", config.dx11], ["autoCheckUpdate", config.autoCheckUpdate], ["bgmEnabled", config.bgmEnabled], ["diagnosticsUploadEnabled", config.diagnosticsUploadEnabled], ["telemetryEnabled", config.telemetryEnabled]] as const) {
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
  gamePath?: string;
  installMethod?: InstallMethod;
  currentVersion?: string;
  latestVersion?: string;
  message?: string;
}

export interface LogUploadResult {
  success: boolean;
  message?: string;
  url?: string;
  localPath?: string;
}

export interface TelemetryStatusPayload {
  status: "disabled" | "sent" | "error";
  message: string;
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

export interface LauncherUpdatePayload {
  version: string;
  tag: string;
  body: string;
  zipUrl: string;
  checksumsUrl?: string;
}

export interface ILauncherState {
  page: "home";
  installing: boolean;
  installed: boolean;
  launching: boolean;
  gameRunning: boolean;
  gameOrigin: "launcher" | "external";
  gamePath: string;
  patchState: PatchState;
  progressPercent: number;
  progressStatus: string;
  progressDownloadedBytes: number;
  progressTotalBytes: number;
  progressSpeedMbps: number;
  appVersion: string;
  vhVersion: string;
  statusMessage: string;
  diagnosticMessage: string;
  logUploadActive: boolean;
  logUploadStatus: string;
  logUploadLocalPath: string;
  telemetryStatus: string;
  telemetryStatusMessage: string;
  mediaStatus: MediaStatusPayload["status"] | "";
  mediaStatusMessage: string;
  mediaProgress: MediaProgressPayload | null;
  launcherUpdateProgress: number;
  launcherUpdateStatus: string;
  launcherUpdateError: string;
  bgmPlaying: boolean;
  bgmVolume: number;
  bgmUrl: string;
  videoUrl: string;
  updateDate: string;
  releaseNotes: ReleaseNotePayload | null;
  releaseNotesLoading: boolean;
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
  init(): Promise<void>;
  saveConfig(): Promise<void>;
}
