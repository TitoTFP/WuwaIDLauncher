import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { isTauriRuntime } from "./runtime";
import {
  DEFAULT_LAUNCHER_CONFIG,
  QUALITY_TIERS,
  type LauncherConfig,
  type QualityLevel,
  type SettingsLoadResult,
  type ThemePayload,
} from "./types";

/**
 * Browser-side stand-in for the Tauri IPC surface, so `npm run dev` can render
 * and drive the real frontend without a native backend.
 *
 * Two query-string knobs keep the mock useful while iterating on a screen:
 *
 *   ?game=C:\Games\Wuthering Waves   game directory the mocked settings report
 *   ?tiers=SD,HD,UHD                 tiers `detect_quality_levels` reports
 *
 * Only the commands the UI needs to render are answered. Anything else rejects
 * with a message naming the command, so a missing handler surfaces in the
 * console instead of leaving a request hanging.
 *
 * This is a development aid: it installs only under `import.meta.env.DEV`, and
 * never when the real Tauri runtime is already present, so it cannot shadow a
 * native backend. The guard is a literal `false` in a production build, so
 * Rollup drops the module and this code never reaches `dist/`.
 */
let installed = false;

export function installDevIpcMock(): boolean {
  if (installed || !import.meta.env.DEV || isTauriRuntime()) return false;
  installed = true;

  // Declares the window label so `getCurrentWindow()` resolves. It registers no
  // command handlers: window actions still arrive as `plugin:window|*` invokes
  // and are handled in the switch below.
  mockWindows("main");

  // The unbranded payload, matching the non-Tauri fallback in
  // `setThemePreference`. Production serves a verified WuwaID theme when one is
  // published, so this deliberately reproduces the general-theme path — which is
  // also what a fresh install, or an offline launcher, actually paints. The
  // alternative of letting this command reject is worse: `themeRuntime` would
  // never be initialised, and the preview would show an uninitialised render
  // that no production launch can reach.
  const generalTheme: ThemePayload = {
    id: "general",
    name: "Tema Umum",
    keyId: "",
    tokens: {},
    css: "",
    backgroundFile: "",
    status: "general",
  };

  const params = new URLSearchParams(window.location.search);
  const gamePath = params.get("game") ?? "C:\\Games\\Wuthering Waves";
  const tiers = parseTiers(params.get("tiers"));

  let settings: LauncherConfig = {
    ...DEFAULT_LAUNCHER_CONFIG,
    gamePath,
    qualityLevel: "auto",
  };
  let nextEventId = 1;

  mockIPC((cmd, payload) => {
    // Dev-only trace: makes it possible to confirm from the console that a
    // command path was actually reached (a silent no-op and an unhandled one
    // look identical from the UI), without instrumenting the page by hand.
    console.debug("[dev IPC mock]", cmd);
    switch (cmd) {
      case "load_settings":
        return {
          settings,
          repaired: false,
          diagnostics: [],
        } satisfies SettingsLoadResult;

      case "save_settings": {
        const { settingsJson } = (payload ?? {}) as { settingsJson?: string };
        if (typeof settingsJson === "string") {
          try {
            settings = {
              ...settings,
              ...(JSON.parse(settingsJson) as Partial<LauncherConfig>),
            };
          } catch (error) {
            throw new Error(`invalid settingsJson (${String(error)})`);
          }
        }
        return null;
      }

      case "detect_quality_levels":
        return tiers;

      case "get_active_theme":
        return generalTheme;

      case "get_app_version":
      case "get_vh_version":
        return "2.11.1-dev";

      case "browse_game_folder":
        return gamePath;

      // The rest of `initialize()`'s background chain. `get_vh_release_notes`
      // is deliberately first in the real order, so letting it reject would
      // short-circuit the calls after it and silently skip them.
      case "check_and_sync_media":
      case "get_vh_release_notes":
      case "get_launcher_release_notes":
      case "check_launcher_update":
        return null;

      case "is_game_running":
        return false;

      case "launch_game":
        console.info("[dev IPC mock] launch_game", payload);
        return null;

      case "minimize_window":
      case "close_window":
        return null;

      // `mockIPC` only intercepts the event plugin when it is constructed with
      // `shouldMockEvents`, which this mock does not pass, so `listen` and
      // `unlisten` reach this switch like any other command.
      case "plugin:event|listen":
        return nextEventId++;
      case "plugin:event|unlisten":
        return null;

      // `check_patch_status` is intentionally NOT stubbed. The command resolves
      // before the `on_patch_status` event carries the result, and
      // `requestPatchStatus` waits on that event, so a stub that returns parks
      // `init()` on its waiter for the whole `PATCH_STATUS_EVENT_TIMEOUT_MS`
      // (15 s), holding `initPromise` non-null and blocking re-entry into
      // `init()` for that long. Rejecting exits through `requestPatchStatus`'
      // own `finally` instead, and the boot chain catches it.
      //
      // Measured by timing the `init()` promise directly: it settles in 6 ms
      // with this rejection, and in 15,006 ms with a resolving stub — the whole
      // `PATCH_STATUS_EVENT_TIMEOUT_MS`. The UI stays responsive either way,
      // because `App.svelte` fires `init()` without awaiting it and no component
      // reads `initialized`, so the cost is the retained `initPromise` rather
      // than a visible stall.
      //
      // The visible trade: the patch panel stays at its default state, because
      // the mock cannot emit events and so cannot reproduce that payload.

      default:
        // Title-bar drag, and anything else the window plugin sends, is a
        // no-op here; the browser has no real window to act on.
        if (cmd.startsWith("plugin:window|")) return null;
        throw new Error(`no dev IPC mock handler for "${cmd}"`);
    }
  });

  console.info(
    `[dev IPC mock] active — game=${gamePath} tiers=${tiers.join(",")}`,
  );
  return true;
}

function parseTiers(raw: string | null): QualityLevel[] {
  const wanted = (raw ?? "")
    .split(",")
    .map((value) => value.trim().toUpperCase())
    .filter((value): value is QualityLevel =>
      (QUALITY_TIERS as readonly string[]).includes(value),
    );
  return wanted.length > 0 ? wanted : ["HD"];
}
