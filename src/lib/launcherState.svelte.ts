import { bridge, setupEventBridge } from "./bridge";
import type {
  ILauncherState,
  LauncherConfig,
  LauncherUpdatePayload,
  PageId,
  PatchState,
  ReleaseNotePayload,
  VisualMode,
} from "./types";

export class LauncherState implements ILauncherState {
  page: PageId = $state<PageId>("home");
  installing: boolean = $state<boolean>(false);
  installed: boolean = $state<boolean>(false);
  launching: boolean = $state<boolean>(false);
  gameRunning: boolean = $state<boolean>(false);
  gameOrigin: "launcher" | "external" = $state<"launcher" | "external">(
    "external",
  );
  gamePath: string = $state<string>("");
  patchState: PatchState = $state<PatchState>("unchecked");
  progressPercent: number = $state<number>(0);
  progressStatus: string = $state<string>("");
  appVersion: string = $state<string>("2.6.1");
  vhVersion: string = $state<string>("");
  visualMode: VisualMode = $state<VisualMode>("full");
  statusMessage: string = $state<string>("");
  logUploadActive: boolean = $state<boolean>(false);
  logUploadStatus: string = $state<string>("");
  bgmPlaying: boolean = $state<boolean>(false);
  bgmVolume: number = $state<number>(0.5);

  // Live Assets & Release Notes
  bgmUrl: string = $state<string>("");
  videoUrl: string = $state<string>("");
  updateDate: string = $state<string>("");
  releaseNotes: ReleaseNotePayload | null = $state<ReleaseNotePayload | null>(
    null,
  );
  releaseNotesLoading: boolean = $state<boolean>(true);

  // Launcher Self-Update
  launcherUpdateAvailable: boolean = $state<boolean>(false);
  launcherUpdatePayload: LauncherUpdatePayload | null =
    $state<LauncherUpdatePayload | null>(null);

  config: LauncherConfig = $state<LauncherConfig>({
    gamePath: "",
    installMethod: "method3",
    launcherVisualMode: "full" as VisualMode,
    dx11: false,
    autoCheckUpdate: true,
    bgmVolume: 0.35,
    bgmEnabled: true,
  });

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
      const raw = await bridge.loadSettings();
      if (raw) {
        const parsed = JSON.parse(raw);
        this.config = { ...this.config, ...parsed };
        this.gamePath = this.config.gamePath || "";
      }
    } catch {
      // ignore
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
        if (payload.status === "ready") {
          this.patchState = "ready";
          this.installed = true;
        } else if (payload.status === "needs_update") {
          this.patchState = "needs_update";
          this.installed = false;
        } else if (payload.status === "not_installed") {
          this.patchState = "not_installed";
          this.installed = false;
        }
      },
      onProgressUpdate: (payload) => {
        this.progressPercent = payload.percent;
        this.progressStatus = payload.status;
      },
      onInstallComplete: () => {
        this.installing = false;
        this.installed = true;
        this.patchState = "ready";
        bridge.getVhVersion().then((v) => {
          this.vhVersion = v;
        });
      },
      onInstallError: (err) => {
        this.installing = false;
        this.statusMessage = `Error: ${err}`;
      },
      onGameLaunchStarted: () => {
        this.launching = true;
      },
      onGameLaunchFinished: () => {
        this.launching = false;
      },
      onLogUploadStarted: () => {
        this.logUploadActive = true;
        this.logUploadStatus = "Mengunggah log...";
      },
      onLogUploadFinished: (res) => {
        this.logUploadActive = false;
        this.logUploadStatus = res.success
          ? "Log berhasil diunggah!"
          : `Gagal: ${res.message || ""}`;
      },
      onLauncherUpdateAvailable: (payload) => {
        this.launcherUpdateAvailable = true;
        this.launcherUpdatePayload = payload;
      },
      onMediaReady: (payload) => {
        if (payload.bgmUrl) this.bgmUrl = payload.bgmUrl;
        if (payload.videoUrl) this.videoUrl = payload.videoUrl;
      },
      onUpdateDate: (dateStr) => {
        this.updateDate = dateStr;
      },
      onVHReleaseNotes: (payload) => {
        this.releaseNotes = payload;
        this.releaseNotesLoading = false;
      },
    });

    // Notify backend that UI is interactive
    try {
      await bridge.notifyUiInteractive();
    } catch {
      // ignore
    }

    // Trigger initial background syncs
    try {
      await bridge.checkAndSyncMedia();
      await bridge.getVhReleaseNotes();
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
    await bridge.saveSettings(JSON.stringify(this.config));
  }
}

export const appState: ILauncherState = new LauncherState();
