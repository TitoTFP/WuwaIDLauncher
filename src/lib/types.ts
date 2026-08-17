export type PageId = "home" | "settings" | "logs" | "about";

export type InstallMethod = "method1" | "method2" | "method3";

export type VisualMode = "full";

export type PatchState =
  | "unchecked"
  | "ready"
  | "needs_update"
  | "not_installed"
  | "error";

export interface LauncherConfig {
  gamePath: string;
  installMethod: InstallMethod;
  launcherVisualMode?: VisualMode;
  dx11?: boolean;
  autoCheckUpdate?: boolean;
  bgmVolume?: number;
  bgmEnabled?: boolean;
}

export interface ProgressPayload {
  percent: number;
  status: string;
}

export interface PatchStatusPayload {
  status: string;
  gamePath?: string;
  installMethod?: string;
  currentVersion?: string;
  latestVersion?: string;
}

export interface LogUploadResult {
  success: boolean;
  message?: string;
  url?: string;
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
}

export interface ILauncherState {
  page: PageId;
  installing: boolean;
  installed: boolean;
  launching: boolean;
  gameRunning: boolean;
  gameOrigin: "launcher" | "external";
  gamePath: string;
  patchState: PatchState;
  progressPercent: number;
  progressStatus: string;
  appVersion: string;
  vhVersion: string;
  visualMode: VisualMode;
  statusMessage: string;
  logUploadActive: boolean;
  logUploadStatus: string;
  bgmPlaying: boolean;
  bgmVolume: number;
  bgmUrl: string;
  videoUrl: string;
  updateDate: string;
  releaseNotes: ReleaseNotePayload | null;
  releaseNotesLoading: boolean;
  launcherUpdateAvailable: boolean;
  launcherUpdatePayload: LauncherUpdatePayload | null;
  config: LauncherConfig;
  init(): Promise<void>;
  saveConfig(): Promise<void>;
}
