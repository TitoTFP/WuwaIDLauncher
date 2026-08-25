import { appState } from "../../src/lib/launcherState.svelte";
import {
  calls,
  fixtureReady,
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
