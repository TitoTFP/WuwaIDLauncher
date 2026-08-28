import { bridge, setupEventBridge } from "./bridge";
import { isTauriRuntime } from "./runtime";
import {
  compactText,
  createGameExitDeduper,
  gameExitToast,
} from "./gameExitNotice.js";
import { createPatchStatusWaiter } from "./patchStatusWait.js";
import { samePath } from "./pathIdentity.js";
import {
  DEFAULT_LAUNCHER_CONFIG,
  launcherReleaseNotesSeenStorageKey,
  normalizeLauncherConfig,
} from "./types";
import type {
  ILauncherState,
  InstallMethod,
  LauncherConfig,
  LauncherOperation,
  LauncherUpdatePayload,
  MediaProgressPayload,
  MediaStatusPayload,
  OperationToken,
  PatchStatusPayload,
  PatchState,
  ReleaseNotePayload,
  ToastKind,
  ToastMessage,
} from "./types";

interface ActiveOperation {
  token: OperationToken;
}

interface PatchStatusRequest {
  generation: number;
  gamePath: string;
  installMethod: InstallMethod;
  hideUid: boolean;
  manualCheck: boolean;
}

const OPERATION_LABELS: Record<LauncherOperation, string> = {
  install: "patch installation",
  launch: "game launch",
  uninstall: "uninstall",
  "method-switch": "method switch",
  folder: "folder change",
  "cache-reset": "cache reset",
  "media-sync": "media sync",
  "force-quit": "force quit",
  "launcher-update": "launcher update",
  "restart-as-admin": "administrator restart",
  close: "launcher close",
};

const PATCH_STATUS_EVENT_TIMEOUT_MS = 15_000;

function operationsConflict(
  left: LauncherOperation,
  right: LauncherOperation,
): boolean {
  if (left === right) return true;

  // The backend deliberately permits media sync alongside patch install and launch.
  if (
    (left === "media-sync" && (right === "install" || right === "launch")) ||
    (right === "media-sync" && (left === "install" || left === "launch"))
  ) {
    return false;
  }

  if (
    (left === "launch" && right === "force-quit") ||
    (right === "launch" && left === "force-quit")
  ) {
    return false;
  }

  return true;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class LauncherState implements ILauncherState {
  installing: boolean = $state<boolean>(false);
  installed: boolean = $state<boolean>(false);
  launching: boolean = $state<boolean>(false);
  gameRunning: boolean = $state<boolean>(false);
  launcherInTray: boolean = $state<boolean>(false);
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
  appVersion: string = $state<string>("2.9.2");
  vhVersion: string = $state<string>("");
  statusMessage: string = $state<string>("");
  diagnosticMessage: string = $state<string>("");
  mediaStatus: MediaStatusPayload["status"] | "" = $state<
    MediaStatusPayload["status"] | ""
  >("");
  mediaStatusMessage: string = $state<string>("");
  mediaProgress: MediaProgressPayload | null =
    $state<MediaProgressPayload | null>(null);
  launcherUpdateProgress: number = $state<number>(0);
  launcherUpdateStatus: string = $state<string>("");
  launcherUpdateError: string = $state<string>("");
  launcherUpdateRestartCountdown: number | null = $state<number | null>(null);
  configSavePending: boolean = $state<boolean>(false);
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
  firstLaunchLauncherReleaseNotes: ReleaseNotePayload | null =
    $state<ReleaseNotePayload | null>(null);

  // Launcher Self-Update
  launcherUpdateAvailable: boolean = $state<boolean>(false);
  launcherUpdatePayload: LauncherUpdatePayload | null =
    $state<LauncherUpdatePayload | null>(null);
  toasts: ToastMessage[] = $state<ToastMessage[]>([]);
  private toastSequence = 0;
  private acceptGameExit = createGameExitDeduper();
  private operationSequence = 0;
  private activeOperations: ActiveOperation[] = $state<ActiveOperation[]>([]);
  private operationByKind = new Map<LauncherOperation, OperationToken>();
  private eventUnlisteners: Array<() => void> = [];
  private initPromise: Promise<void> | null = null;
  private initialized = false;
  private lifecycleGeneration = 0;
  private pendingConfigSaveCount = 0;
  private saveQueue: Promise<void> = Promise.resolve();
  private patchStatusGeneration = 0;
  private latestPatchStatusRequest: PatchStatusRequest | null = null;
  private activePatchStatusRequest: PatchStatusRequest | null = null;
  private patchStatusQueue: Promise<void> = Promise.resolve();
  private patchStatusWaiters = new Map<number, () => void>();
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
    const text =
      diagnostic && diagnostic !== message
        ? `${message}\n${diagnostic}`
        : message;
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

  beginOperation(kind: LauncherOperation): OperationToken | null {
    if (this.isOperationBlocked(kind)) return null;

    const token = Object.freeze({
      id: ++this.operationSequence,
      kind,
    });
    this.activeOperations = [...this.activeOperations, { token }];
    this.operationByKind.set(kind, token);
    return token;
  }

  endOperation(token: OperationToken) {
    if (!this.activeOperations.some((active) => active.token.id === token.id)) {
      return;
    }
    this.activeOperations = this.activeOperations.filter(
      (active) => active.token.id !== token.id,
    );
    if (this.operationByKind.get(token.kind)?.id === token.id) {
      this.operationByKind.delete(token.kind);
    }
  }

  private endCurrentOperation(kind: LauncherOperation): boolean {
    const token = this.operationByKind.get(kind);
    if (!token) return false;
    this.endOperation(token);
    return true;
  }

  private transitionOperation(
    token: OperationToken,
    nextKind: LauncherOperation,
  ): boolean {
    const current = this.activeOperations.find(
      (active) => active.token.id === token.id,
    );
    if (!current) return false;

    const conflicting = this.activeOperations.some(
      (active) =>
        active.token.id !== token.id &&
        operationsConflict(active.token.kind, nextKind),
    );
    if (conflicting) return false;

    const nextToken = Object.freeze({
      id: token.id,
      kind: nextKind,
    });
    this.activeOperations = this.activeOperations.map((active) =>
      active.token.id === token.id ? { token: nextToken } : active,
    );
    this.operationByKind.delete(token.kind);
    this.operationByKind.set(nextKind, nextToken);
    return true;
  }

  isOperationBlocked(kind: LauncherOperation): boolean {
    return this.activeOperations.some((active) =>
      operationsConflict(active.token.kind, kind),
    );
  }

  getOperationBusyMessage(kind: LauncherOperation): string {
    const active = this.activeOperations.find((operation) =>
      operationsConflict(operation.token.kind, kind),
    );
    if (!active) {
      return `busy: ${OPERATION_LABELS[kind]} is already in progress`;
    }
    return `busy: ${OPERATION_LABELS[active.token.kind]} is already in progress; cannot start ${OPERATION_LABELS[kind]}`;
  }

  setGamePath(path: string) {
    this.gamePath = path;
    this.config.gamePath = path;
    this.invalidatePatchStatus();
  }

  invalidatePatchStatus() {
    this.patchStatusGeneration += 1;
    this.latestPatchStatusRequest = null;
    this.patchStatusCheckPending = false;
    for (const resolve of this.patchStatusWaiters.values()) resolve();
    this.patchStatusWaiters.clear();
  }

  private patchSelectionMatches(request: PatchStatusRequest): boolean {
    return (
      samePath(this.gamePath, request.gamePath) &&
      this.config.installMethod === request.installMethod &&
      this.config.hideUid === request.hideUid
    );
  }

  async requestPatchStatus(
    gamePath: string,
    installMethod: InstallMethod,
    manualCheck = false,
  ) {
    if (!isTauriRuntime()) return;

    const request: PatchStatusRequest = {
      generation: ++this.patchStatusGeneration,
      gamePath,
      installMethod,
      hideUid: this.config.hideUid,
      manualCheck,
    };
    this.latestPatchStatusRequest = request;
    this.patchStatusCheckPending = manualCheck;

    const run = async () => {
      if (
        this.latestPatchStatusRequest !== request ||
        !this.patchSelectionMatches(request)
      ) {
        return;
      }

      this.activePatchStatusRequest = request;
      const eventWaiter = createPatchStatusWaiter(
        PATCH_STATUS_EVENT_TIMEOUT_MS,
      );
      this.patchStatusWaiters.set(request.generation, eventWaiter.resolve);

      try {
        await bridge.checkPatchStatus(
          request.gamePath,
          request.installMethod,
          request.hideUid,
        );
        // The backend emits the result before the command completes. Waiting for
        // that event keeps a same-identity slow response from being attributed to
        // the next generation. A command error exits through the finally block;
        // a successful command is required to emit this payload by contract.
        await eventWaiter.promise;
      } finally {
        eventWaiter.resolve();
        this.patchStatusWaiters.delete(request.generation);
        if (this.activePatchStatusRequest === request) {
          this.activePatchStatusRequest = null;
        }
        if (this.latestPatchStatusRequest === request) {
          this.patchStatusCheckPending = false;
        }
      }
    };

    const task = this.patchStatusQueue.then(run, run);
    this.patchStatusQueue = task.then(
      () => undefined,
      () => undefined,
    );
    return task;
  }

  async selectGameFolder(): Promise<boolean> {
    if (!isTauriRuntime()) return false;

    const token = this.beginOperation("folder");
    if (!token) {
      this.setStatus(
        "Operasi folder tidak dapat dimulai.",
        this.getOperationBusyMessage("folder"),
      );
      return false;
    }

    const previousPath = this.gamePath;
    let pathChanged = false;
    let persisted = false;
    try {
      const selected = await bridge.browseGameFolder();
      if (selected === "") return false;
      if (selected === "?INVALID") {
        this.setStatus("Folder game yang dipilih tidak valid.");
        return false;
      }

      this.setGamePath(selected);
      pathChanged = true;
      await this.saveConfig();
      persisted = true;

      let statusChecked = true;
      try {
        await this.requestPatchStatus(selected, this.config.installMethod);
      } catch (error) {
        statusChecked = false;
        this.setStatus(
          "Folder game diperbarui, tetapi status patch tidak dapat diperiksa.",
          errorMessage(error),
        );
      }
      if (statusChecked) this.setStatus("Folder game diperbarui.");
      return true;
    } catch (error) {
      if (pathChanged && !persisted) this.setGamePath(previousPath);
      this.setStatus("Gagal memilih folder game.", errorMessage(error));
      return false;
    } finally {
      this.endOperation(token);
    }
  }

  async switchInstallMethod(method: InstallMethod) {
    if (method === this.config.installMethod) return;
    if (!isTauriRuntime()) {
      this.config.installMethod = method;
      return;
    }

    const token = this.beginOperation("method-switch");
    if (!token) throw new Error(this.getOperationBusyMessage("method-switch"));

    const previousMethod = this.config.installMethod;
    const gamePath = this.gamePath;
    let backendChanged = false;
    try {
      this.invalidatePatchStatus();
      this.config.installMethod = method;
      await this.saveConfig();

      if (gamePath) {
        const report = await bridge.switchMethod(gamePath, method);
        if (report.failures.length || report.preserved.length) {
          throw new Error([...report.failures, ...report.preserved].join("; "));
        }
        backendChanged = true;
        await this.requestPatchStatus(gamePath, method);
      }
    } catch (error) {
      // Persist first, then clean up the backend. If cleanup rejects, restore the
      // previous setting so memory and disk remain aligned with the backend.
      if (!backendChanged) {
        this.config.installMethod = previousMethod;
        try {
          await this.saveConfig();
        } catch (rollbackError) {
          throw new Error(
            `${errorMessage(error)}; pengaturan sebelumnya juga tidak dapat dipulihkan: ${errorMessage(rollbackError)}`,
          );
        }
      }
      throw error;
    } finally {
      this.endOperation(token);
    }
  }

  async startMediaSync() {
    if (!isTauriRuntime()) return;

    const token = this.beginOperation("media-sync");
    if (!token) throw new Error(this.getOperationBusyMessage("media-sync"));
    try {
      await bridge.checkAndSyncMedia();
    } catch (error) {
      this.endOperation(token);
      throw error;
    }
  }

  async forceQuitGame(): Promise<boolean> {
    if (!isTauriRuntime()) return false;

    const token = this.beginOperation("force-quit");
    if (!token) throw new Error(this.getOperationBusyMessage("force-quit"));
    try {
      return await bridge.forceQuitGame();
    } finally {
      this.endOperation(token);
    }
  }

  async resetWebViewCache() {
    if (!isTauriRuntime()) return;

    const token = this.beginOperation("cache-reset");
    if (!token) throw new Error(this.getOperationBusyMessage("cache-reset"));
    try {
      await bridge.resetWebViewCache();
      this.transitionOperation(token, "media-sync");
    } catch (error) {
      this.endOperation(token);
      throw error;
    }
  }

  async performLauncherUpdate(version: string) {
    if (!isTauriRuntime()) return;

    const token = this.beginOperation("launcher-update");
    if (!token) {
      throw new Error(this.getOperationBusyMessage("launcher-update"));
    }
    try {
      await bridge.performLauncherUpdate(version);
    } catch (error) {
      this.endOperation(token);
      throw error;
    }
  }

  async restartAsAdmin() {
    if (!isTauriRuntime()) return;

    const token = this.beginOperation("restart-as-admin");
    if (!token) {
      throw new Error(this.getOperationBusyMessage("restart-as-admin"));
    }
    try {
      await bridge.restartAsAdmin();
    } finally {
      this.endOperation(token);
    }
  }

  init(): Promise<void> {
    if (!isTauriRuntime()) {
      this.releaseNotesLoading = false;
      return Promise.resolve();
    }
    if (this.initialized) return Promise.resolve();
    if (this.initPromise) return this.initPromise;

    const generation = ++this.lifecycleGeneration;
    const initialization = this.initialize(generation);
    let tracked: Promise<void>;
    tracked = initialization.catch((error) => {
      if (this.initPromise === tracked) this.initPromise = null;
      throw error;
    });
    this.initPromise = tracked;
    return tracked;
  }

  private async initialize(generation: number) {
    const isCurrent = () => this.lifecycleGeneration === generation;

    try {
      this.appVersion = await bridge.getAppVersion();
      this.vhVersion = await bridge.getVhVersion();
    } catch {
      // The launcher can still start without version metadata.
    }
    if (!isCurrent()) return;

    try {
      const result = await bridge.loadSettings();
      const normalized = normalizeLauncherConfig(result.settings);
      this.config = normalized.config;
      this.gamePath = this.config.gamePath;
      this.bgmVolume = this.config.bgmVolume;
      if (result.diagnostics.length || normalized.diagnostics.length) {
        const diagnostics = [...result.diagnostics, ...normalized.diagnostics];
        this.setStatus(
          "Konfigurasi launcher dipulihkan ke nilai aman.",
          diagnostics.join(" "),
        );
      }
    } catch {
      this.setStatus(
        "Konfigurasi launcher tidak dapat dimuat; memakai default.",
      );
    }
    if (!isCurrent()) return;

    try {
      this.gameRunning = await bridge.isGameRunning();
    } catch {
      // The runtime event will correct this when available.
    }
    if (!isCurrent()) return;

    const unlisteners = await setupEventBridge({
      onGameRuntimeState: (active, origin) => {
        this.gameRunning = active;
        this.gameOrigin = origin;
      },
      onLauncherTrayState: (inTray) => {
        this.launcherInTray = inTray;
      },
      onPatchStatus: (payload: PatchStatusPayload) => {
        const request = this.activePatchStatusRequest;
        if (
          !request ||
          !samePath(payload.gamePath, request.gamePath) ||
          payload.installMethod !== request.installMethod ||
          payload.hideUid !== request.hideUid
        ) {
          return;
        }

        this.patchStatusWaiters.get(request.generation)?.();
        if (
          this.latestPatchStatusRequest !== request ||
          !this.patchSelectionMatches(request)
        ) {
          return;
        }
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
          this.vhVersion = "";
        } else if (payload.status === "invalid" || payload.status === "error") {
          this.patchState = payload.status;
          this.installed = false;
          if (payload.message) this.setStatus(payload.message, payload.message);
        } else {
          this.patchState = payload.status;
          this.installed = false;
        }
        if (payload.status === "not_installed" || !payload.currentVersion) {
          this.vhVersion = "";
        } else {
          this.vhVersion = payload.currentVersion;
        }

        if (request.manualCheck) {
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
        if (!this.endCurrentOperation("install")) return;
        this.installing = false;
        this.installed = true;
        this.patchState = "ready";
        this.progressPercent = 0;
        this.progressStatus = "";
        this.progressDownloadedBytes = 0;
        this.progressTotalBytes = 0;
        this.progressSpeedMbps = 0;
        this.clearStatus();
        const completedPath = this.gamePath;
        const completedMethod = this.config.installMethod;
        bridge
          .getVhVersion()
          .then((version) => {
            if (
              this.gamePath === completedPath &&
              this.config.installMethod === completedMethod
            ) {
              this.vhVersion = version;
            }
          })
          .catch(() => {});
      },
      onInstallError: (err) => {
        if (!this.endCurrentOperation("install")) return;
        this.installing = false;
        this.patchState = "error";
        this.progressPercent = 0;
        this.progressStatus = "";
        this.progressDownloadedBytes = 0;
        this.progressTotalBytes = 0;
        this.progressSpeedMbps = 0;
        this.setStatus(
          "Operasi gagal. Silakan coba lagi.",
          err || "Tidak ada detail error.",
        );
      },
      onLaunchError: (err) => {
        this.endCurrentOperation("launch");
        this.launching = false;
        this.clearStatus();
        const detail = compactText(err || "");
        this.showToast(
          detail
            ? `Game tidak dapat dijalankan: ${detail}`
            : "Game tidak dapat dijalankan.",
          "err",
        );
      },
      onGameLaunchStarted: () => {
        if (this.operationByKind.has("launch")) this.launching = true;
      },
      onGameLaunchFinished: () => {
        if (!this.endCurrentOperation("launch")) return;
        this.launching = false;
      },
      onGameExit: (payload) => {
        if (!this.acceptGameExit(payload)) return;
        this.endCurrentOperation("launch");
        this.launching = false;
        this.gameRunning = false;
        this.clearStatus();
        const toast = gameExitToast(payload);
        this.showToast(toast.message, toast.kind);
      },
      onLauncherUpdateProgress: (percent, statusText) => {
        this.launcherUpdateProgress = percent;
        this.launcherUpdateStatus = statusText;
        this.launcherUpdateError = "";
      },
      onLauncherUpdateRestarting: (remainingSeconds) => {
        const safeSeconds = Math.max(0, Math.floor(remainingSeconds));
        this.launcherUpdateError = "";
        this.launcherUpdateRestartCountdown = safeSeconds;
        this.launcherUpdateStatus =
          safeSeconds === 0
            ? "Launcher sedang dimulai ulang..."
            : `Update selesai diunduh. Launcher akan tertutup otomatis dan dibuka kembali dalam ${safeSeconds} detik.`;
        if (safeSeconds === 0) this.endCurrentOperation("launcher-update");
        else
          this.setStatus(
            "Launcher akan tertutup otomatis lalu dibuka kembali.",
          );
      },
      onLauncherUpdateError: (error) => {
        this.endCurrentOperation("launcher-update");
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
        if (["ready", "offline", "error"].includes(payload.status)) {
          this.endCurrentOperation("media-sync");
        }
        if (payload.status === "error" || payload.status === "offline") {
          this.setStatus(
            payload.status === "offline"
              ? "Media offline; launcher tetap dapat digunakan."
              : "Sinkronisasi media gagal.",
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
            if (
              !localStorage.getItem(
                launcherReleaseNotesSeenStorageKey(payload.tag),
              )
            ) {
              this.firstLaunchLauncherReleaseNotes = payload;
            }
          } catch {
            this.firstLaunchLauncherReleaseNotes = payload;
          }
        }
      },
    });

    if (!isCurrent()) {
      for (const unlisten of unlisteners) unlisten();
      return;
    }
    this.eventUnlisteners = unlisteners;

    try {
      await bridge.notifyUiInteractive(this.config.installMethod);
    } catch {
      // The UI remains usable if the optional heartbeat cannot start.
    }
    if (!isCurrent()) return;

    try {
      await this.startMediaSync();
    } catch {
      // Media is optional; its event reports the detailed failure when possible.
    }
    if (!isCurrent()) return;

    try {
      await bridge.getVhReleaseNotes();
      await bridge.getLauncherReleaseNotes();
      await bridge.checkLauncherUpdate();
      if (this.gamePath) {
        await this.requestPatchStatus(this.gamePath, this.config.installMethod);
      }
    } catch {
      // Background refresh failures must not prevent the launcher from opening.
    }
    if (isCurrent()) this.initialized = true;
  }

  dispose() {
    this.lifecycleGeneration += 1;
    for (const unlisten of this.eventUnlisteners) unlisten();
    this.eventUnlisteners = [];
    this.initialized = false;
    this.initPromise = null;
    for (const resolve of this.patchStatusWaiters.values()) resolve();
    this.patchStatusWaiters.clear();
    this.activePatchStatusRequest = null;
    this.latestPatchStatusRequest = null;
    this.patchStatusCheckPending = false;
    this.patchStatusQueue = Promise.resolve();
    this.activeOperations = [];
    this.operationByKind.clear();
    this.installing = false;
    this.launching = false;
  }

  async saveConfig() {
    this.config.gamePath = this.gamePath;
    this.config = normalizeLauncherConfig(this.config).config;
    this.gamePath = this.config.gamePath;
    this.bgmVolume = this.config.bgmVolume;
    if (!isTauriRuntime()) return;

    const serialized = JSON.stringify(this.config);
    this.pendingConfigSaveCount += 1;
    this.configSavePending = true;

    const save = this.saveQueue.then(
      () => bridge.saveSettings(serialized),
      () => bridge.saveSettings(serialized),
    );
    this.saveQueue = save.then(
      () => undefined,
      () => undefined,
    );

    try {
      await save;
    } finally {
      this.pendingConfigSaveCount -= 1;
      this.configSavePending = this.pendingConfigSaveCount > 0;
    }
  }
}

export const appState: ILauncherState = new LauncherState();
