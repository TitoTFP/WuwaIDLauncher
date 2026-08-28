import { appState } from "../../src/lib/launcherState.svelte";
import { isTauriRuntime } from "../../src/lib/runtime";
import {
  calls,
  fixtureReady,
  invoke,
  shutdownFixture,
} from "./launcherStateTauriFixture.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

type InvokeCall = {
  command: string;
  args: Record<string, unknown>;
};

const invokeCalls = calls.invoke as InvokeCall[];
const deliveredEvents = calls.events as Array<{
  event: string;
  payload: Record<string, unknown>;
}>;

function callsFor(command: string) {
  return invokeCalls.filter((call) => call.command === command);
}

export async function runLauncherStateScenario() {
  try {
    const paths = await fixtureReady;
    const initStarted = performance.now();
    await appState.init();
    const initDuration = performance.now() - initStarted;
    assert(isTauriRuntime(), "frontend fixture did not expose Tauri runtime");

    await invoke("fixture_emit_launcher_release_notes", { tag: "v2.10.0" });
    assert(
      appState.firstLaunchLauncherReleaseNotes?.tag === "v2.10.0",
      "release-note event was not routed to LauncherState",
    );
    appState.dismissFirstLaunchLauncherReleaseNotes();
    assert(
      appState.firstLaunchLauncherReleaseNotes === null,
      "release-note dismissal did not close the modal state",
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    const acknowledgementCalls = callsFor("acknowledge_launcher_release_notes");
    assert(
      acknowledgementCalls.length === 1 &&
        acknowledgementCalls[0].args.tag === "v2.10.0",
      `release-note dismissal did not acknowledge the matching tag: ${JSON.stringify(acknowledgementCalls)}`,
    );
    await invoke("fixture_emit_launcher_release_notes", { tag: "v2.10.0" });
    assert(
      appState.firstLaunchLauncherReleaseNotes === null,
      "an acknowledged release-note tag was shown again",
    );
    await invoke("fixture_emit_launcher_release_notes", { tag: "v2.11.0" });
    assert(
      appState.firstLaunchLauncherReleaseNotes?.tag === "v2.11.0",
      "a new release-note tag was not shown",
    );
    appState.dismissFirstLaunchLauncherReleaseNotes();

    assert(
      initDuration < 5_000,
      "initial patch status exceeded the test budget",
    );
    const startupStatusCalls = callsFor("check_patch_status");
    assert(startupStatusCalls.length === 1, "startup status was not requested");
    assert(
      startupStatusCalls[0].args.gamePath === paths.canonicalGamePath &&
        startupStatusCalls[0].args.installMethod === "resource_mount" &&
        startupStatusCalls[0].args.hideUid === false,
      "real bridge sent the wrong startup IPC payload",
    );
    assert(
      appState.patchStatusCheckPending === false,
      "startup status waiter remained pending",
    );
    assert(
      appState.patchState === "not_installed",
      "backend status event was not routed to LauncherState",
    );

    // The backend has already canonicalized persisted settings. Re-select the
    // legacy lexical alias to exercise frontend samePath matching against the
    // canonical path emitted by the real Rust command.
    appState.setGamePath(paths.legacyGamePath);
    await appState.requestPatchStatus(
      paths.legacyGamePath,
      appState.config.installMethod,
    );
    const aliasStatusCalls = callsFor("check_patch_status");
    assert(
      aliasStatusCalls.length === 2 &&
        aliasStatusCalls[1].args.gamePath === paths.legacyGamePath,
      "real bridge did not preserve the legacy path in the IPC request",
    );
    assert(
      appState.patchStatusCheckPending === false,
      "canonical event did not resolve the legacy-path waiter",
    );

    const switchStarted = performance.now();
    await appState.switchInstallMethod("loader");
    const switchDuration = performance.now() - switchStarted;

    assert(switchDuration < 5_000, "method switch exceeded the test budget");
    const switchMethodCalls = callsFor("switch_method");
    assert(
      switchMethodCalls.length === 1,
      "backend method switch was not called",
    );
    assert(
      switchMethodCalls[0].args.gamePath === paths.legacyGamePath &&
        switchMethodCalls[0].args.newMethod === "loader",
      "real bridge sent the wrong method-switch IPC payload",
    );
    const postSwitchStatusCalls = callsFor("check_patch_status");
    assert(
      postSwitchStatusCalls.length === 3,
      "post-switch status was not requested",
    );
    assert(
      postSwitchStatusCalls[2].args.gamePath === paths.legacyGamePath &&
        postSwitchStatusCalls[2].args.installMethod === "loader" &&
        postSwitchStatusCalls[2].args.hideUid === false,
      "post-switch status request did not use the new method",
    );
    const savedSettings = callsFor("save_settings");
    assert(
      savedSettings.some((call) => {
        const settings = JSON.parse(String(call.args.settingsJson)) as {
          installMethod?: string;
        };
        return settings.installMethod === "loader";
      }),
      "new method was not persisted before the backend switch",
    );
    assert(
      appState.config.installMethod === "loader" &&
        appState.patchState === "not_installed",
      "LauncherState did not commit the post-switch state",
    );
    assert(
      appState.patchStatusCheckPending === false,
      "post-switch backend event did not resolve the waiter",
    );
    const patchEvents = deliveredEvents.filter(
      (event) => event.event === "onPatchStatus",
    );
    assert(
      patchEvents.length === 3 &&
        patchEvents.every(
          (event) => event.payload.gamePath === paths.canonicalGamePath,
        ),
      "Rust backend status events were not delivered through Tauri event routing",
    );
    assert(
      paths.canonicalGamePath !== paths.legacyGamePath,
      "fixture must use distinct legacy and canonical path strings",
    );
  } finally {
    appState.dispose();
    await shutdownFixture();
  }
}

(globalThis as any).__launcherStateScenario = runLauncherStateScenario();
