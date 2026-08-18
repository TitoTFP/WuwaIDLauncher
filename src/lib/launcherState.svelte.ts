import { bridge, setupEventBridge } from "./bridge";
import {
  DEFAULT_LAUNCHER_CONFIG,
  launcherReleaseNotesSeenStorageKey,
  normalizeLauncherConfig,
} from "./types";
import type {
  ILauncherState,
  LauncherConfig,
  LauncherUpdatePayload,
  MediaProgressPayload,
  MediaStatusPayload,
  PatchState,
  ReleaseNotePayload,
  ToastKind,
  ToastMessage,
} from "./types";

export class LauncherState implements ILauncherState {
  page: "home" = $state("home");
  installing: boolean = $state<boolean>(false);
  installed: boolean = $state<boolean>(false);
  launching: boolean = $state<boolean>(false);
  signatureRestoreCountdown: number | null = $state<number | null>(null);
  gameRunning: boolean = $state<boolean>(false);
  gameOrigin: "launcher" | "external" = $state<"launcher" | "external">(
    "external",
  );
  gamePath: string = $state<string>("");
  patchState: PatchState = $state<PatchState>("unchecked");
  patchStatusCheckPending: boolean = $state<boolean>(false);
  progressPercent: number = $state<number>(0);
  progressStatus: string = $state<string>("");
  progressDownloadedBytes: number = $state<number>(0);
  progressTotalBytes: number = $state<number>(0);
  progressSpeedMbps: number = $state<number>(0);
  appVersion: string = $state<string>("2.8.0");
  vhVersion: string = $state<string>("");
  statusMessage: string = $state<string>("");
  diagnosticMessage: string = $state<string>("");
  mediaStatus: MediaStatusPayload["status"] | "" = $state<MediaStatusPayload["status"] | "">("");
  mediaStatusMessage: string = $state<string>("");
  mediaProgress: MediaProgressPayload | null = $state<MediaProgressPayload | null>(null);
  launcherUpdateProgress: number = $state<number>(0);
  launcherUpdateStatus: string = $state<string>("");
  launcherUpdateError: string = $state<string>("");
  launcherUpdateRestartCountdown: number | null = $state<number | null>(null);
  bgmPlaying: boolean = $state<boolean>(false);
  bgmVolume: number = $state<number>(DEFAULT_LAUNCHER_CONFIG.bgmVolume);

  // Live Assets & Release Notes
  bgmUrl: string = $state<string>("");
  videoUrl: string = $state<string>("");
  updateDate: string = $state<string>("");
  releaseNotes: ReleaseNotePayload | null = $state<ReleaseNotePayload | null>(
    null,
  );
  releaseNotesLoading: boolean = $state<boolean>(true);
  firstLaunchLauncherReleaseNotes: ReleaseNotePayload | null = $state<ReleaseNotePayload | null>(null);

  // Launcher Self-Update
  launcherUpdateAvailable: boolean = $state<boolean>(false);
  launcherUpdatePayload: LauncherUpdatePayload | null =
    $state<LauncherUpdatePayload | null>(null);
  toasts: ToastMessage[] = $state<ToastMessage[]>([]);
  private toastSequence = 0;
  adminPromptOpen: boolean = $state(false);
  adminPromptPath: string = $state("");

  config: LauncherConfig = $state<LauncherConfig>({
    ...DEFAULT_LAUNCHER_CONFIG,
  });

  setStatus(message: string, diagnostic = message) {
    this.statusMessage = message;
    this.diagnosticMessage = diagnostic;
    const kind = /gagal|tidak|error|invalid|konflik|failed/i.test(message)
      ? "err"
      : "info";
    const text = diagnostic && diagnostic !== message ? `${message}\n${diagnostic}` : message;
    this.showToast(text, kind);
  }

  clearStatus() {
    this.statusMessage = "";
    this.diagnosticMessage = "";
  }

  showToast(message: string, kind: ToastKind = "info") {
    const toast = { id: ++this.toastSequence, message, kind };
    this.toasts = [...this.toasts, toast];
    window.setTimeout(() => {
      this.toasts = this.toasts.filter((item) => item.id !== toast.id);
    }, 4200);
  }

  openAdminPrompt(path: string) {
    this.adminPromptPath = path;
    this.adminPromptOpen = true;
  }

  closeAdminPrompt() {
    this.adminPromptOpen = false;
  }

  dismissLauncherUpdate() {
    this.launcherUpdateAvailable = false;
    this.launcherUpdatePayload = null;
    this.launcherUpdateProgress = 0;
    this.launcherUpdateStatus = "";
    this.launcherUpdateError = "";
    this.launcherUpdateRestartCountdown = null;
  }

  dismissFirstLaunchLauncherReleaseNotes() {
    const tag = this.firstLaunchLauncherReleaseNotes?.tag.trim();
    if (tag) {
      try {
        localStorage.setItem(launcherReleaseNotesSeenStorageKey(tag), "1");
      } catch {
        // A restricted WebView storage must not prevent the modal from closing.
      }
    }
    this.firstLaunchLauncherReleaseNotes = null;
  }

  async init() {
    // Load version
    try {
      this.appVersion = await bridge.getAppVersion();
      this.vhVersion = await bridge.getVhVersion();
    } catch {
      // ignore
    }

    // Load settings
    try {
      const result = await bridge.loadSettings();
      const normalized = normalizeLauncherConfig(result.settings);
      this.config = normalized.config;
      this.gamePath = this.config.gamePath;
      this.bgmVolume = this.config.bgmVolume;
      if (result.diagnostics.length || normalized.diagnostics.length) {
        const diagnostics = [...result.diagnostics, ...normalized.diagnostics];
        this.setStatus("Konfigurasi launcher dipulihkan ke nilai aman.", diagnostics.join(" "));
      }
    } catch {
      this.setStatus("Konfigurasi launcher tidak dapat dimuat; memakai default.");
    }

    // Check game running
    try {
      this.gameRunning = await bridge.isGameRunning();
    } catch {
      // ignore
    }

    // Setup event listeners
    await setupEventBridge({
      onGameRuntimeState: (active, origin) => {
        this.gameRunning = active;
        this.gameOrigin = origin;
      },
      onPatchStatus: (payload) => {
        const manualCheck = this.patchStatusCheckPending;
        this.patchStatusCheckPending = false;
        if (payload.status === "ready") {
          this.patchState = "ready";
          this.installed = true;
        } else if (payload.status === "needs_update") {
          this.patchState = "needs_update";
          this.installed = false;
        } else if (payload.status === "not_installed") {
          this.patchState = "not_installed";
          this.installed = false;
        } else if (payload.status === "invalid" || payload.status === "error") {
          this.patchState = payload.status;
          this.installed = false;
          if (payload.message) this.setStatus(payload.message, payload.message);
        }
        if (payload.currentVersion) this.vhVersion = payload.currentVersion;
        if (manualCheck) {
          if (payload.status === "ready") {
            this.showToast(
              payload.latestVersion
                ? "Patch ID sudah terbaru."
                : "Patch ID aktif; versi terbaru belum dapat diverifikasi.",
              payload.latestVersion ? "ok" : "info",
            );
          } else if (payload.status === "needs_update") {
            this.showToast("Versi Patch ID baru tersedia.", "info");
          } else if (payload.status === "not_installed") {
            this.showToast("Patch ID belum terpasang.", "info");
          }
        }
      },
      onProgressUpdate: (payload) => {
        this.progressPercent = payload.percent;
        this.progressStatus = payload.status;
        this.progressDownloadedBytes = payload.downloadedBytes ?? 0;
        this.progressTotalBytes = payload.totalBytes ?? 0;
        this.progressSpeedMbps = payload.speedMbps ?? 0;
      },
      onInstallComplete: () => {
        this.installing = false;
        this.installed = true;
        this.patchState = "ready";
        this.progressPercent = 0;
        this.progressStatus = "";
        this.progressDownloadedBytes = 0;
        this.progressTotalBytes = 0;
        this.progressSpeedMbps = 0;
        this.clearStatus();
        bridge.getVhVersion().then((v) => {
          this.vhVersion = v;
        });
      },
      onInstallError: (err) => {
        const wasLaunching = this.launching;
        this.installing = false;
        this.launching = false;
        if (!wasLaunching) this.patchState = "error";
        this.progressPercent = 0;
        this.progressStatus = "";
        this.progressDownloadedBytes = 0;
        this.progressTotalBytes = 0;
        this.progressSpeedMbps = 0;
        this.setStatus("Operasi gagal. Silakan coba lagi.", err || "Tidak ada detail error.");
      },
      onLaunchError: (err) => {
        this.launching = false;
        this.setStatus("Game tidak dapat dijalankan.", err || "Tidak ada detail error.");
      },
      onGameLaunchStarted: () => {
        this.launching = true;
      },
      onGameLaunchFinished: () => {
        this.launching = false;
      },
      onSignatureRestoreCountdown: (remainingSeconds, active) => {
        this.signatureRestoreCountdown = active && remainingSeconds > 0
          ? remainingSeconds
          : null;
      },
      onLauncherUpdateProgress: (percent, statusText) => {
        this.launcherUpdateProgress = percent;
        this.launcherUpdateStatus = statusText;
        this.launcherUpdateError = "";
      },
      onLauncherUpdateRestarting: (remainingSeconds) => {
        this.launcherUpdateError = "";
        this.launcherUpdateRestartCountdown = remainingSeconds;
        this.launcherUpdateStatus = `Update selesai diunduh. Launcher akan tertutup otomatis dan dibuka kembali dalam ${remainingSeconds} detik.`;
        this.setStatus("Launcher akan tertutup otomatis lalu dibuka kembali.");
      },
      onLauncherUpdateError: (error) => {
        this.dismissLauncherUpdate();
        this.showToast(`Pembaruan gagal: ${error}`, "err");
      },
      onLauncherUpdateAvailable: (payload) => {
        this.launcherUpdateAvailable = true;
        this.launcherUpdatePayload = payload;
        this.launcherUpdateProgress = 0;
        this.launcherUpdateError = "";
        this.launcherUpdateStatus = "Menunggu konfirmasi.";
        this.launcherUpdateRestartCountdown = null;
      },
      onLauncherUpdateStatus: (payload) => {
        this.showToast(payload.message, payload.kind);
      },
      onLauncherUpdateStaged: () => {
        this.launcherUpdateStatus = "Update terverifikasi dan siap diterapkan.";
        this.showToast("Update launcher sudah diverifikasi.", "ok");
      },
      onMediaReady: (payload) => {
        this.mediaStatus = "ready";
        this.mediaStatusMessage = "Media siap digunakan.";
        this.mediaProgress = null;
        if (payload.bgmUrl) this.bgmUrl = payload.bgmUrl;
        if (payload.videoUrl) this.videoUrl = payload.videoUrl;
      },
      onMediaStatus: (payload) => {
        this.mediaStatus = payload.status;
        this.mediaStatusMessage = payload.message;
        if (payload.status === "checking" || payload.status === "downloading") {
          this.bgmPlaying = false;
          this.bgmUrl = "";
          this.videoUrl = "";
        }
        if (payload.status === "error" || payload.status === "offline") {
          this.setStatus(
            payload.status === "offline" ? "Media offline; launcher tetap dapat digunakan." : "Sinkronisasi media gagal.",
            payload.message,
          );
        }
      },
      onMediaProgress: (payload) => {
        this.mediaProgress = payload;
      },
      onUpdateDate: (dateStr) => {
        this.updateDate = dateStr;
      },
      onVHReleaseNotes: (payload) => {
        this.releaseNotes = payload;
        this.releaseNotesLoading = false;
      },
      onLauncherReleaseNotes: (payload) => {
        if (payload.tag.trim()) {
          try {
            if (!localStorage.getItem(launcherReleaseNotesSeenStorageKey(payload.tag))) {
              this.firstLaunchLauncherReleaseNotes = payload;
            }
          } catch {
            this.firstLaunchLauncherReleaseNotes = payload;
          }
        }
      },
    });

    // Notify backend that UI is interactive
    try {
      await bridge.notifyUiInteractive(this.config.installMethod);
    } catch {
      // ignore
    }

    // Trigger initial background syncs
    try {
      await bridge.checkAndSyncMedia();
      await bridge.getVhReleaseNotes();
      await bridge.getLauncherReleaseNotes();
      if (this.config.autoCheckUpdate) {
        await bridge.checkLauncherUpdate();
      }
      if (this.gamePath) {
        await bridge.checkPatchStatus(this.gamePath, this.config.installMethod);
      }
    } catch {
      // ignore
    }
  }

  async saveConfig() {
    this.config.gamePath = this.gamePath;
    this.config = normalizeLauncherConfig(this.config).config;
    await bridge.saveSettings(JSON.stringify(this.config));
  }
}

export const appState: ILauncherState = new LauncherState();
