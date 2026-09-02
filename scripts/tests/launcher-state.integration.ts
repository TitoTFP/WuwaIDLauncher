import { appState } from "../../src/lib/launcherState.svelte";
import { isTauriRuntime } from "../../src/lib/runtime";
import { isEffectivelyEmptyUidText, isValidUidText } from "../../src/lib/types";
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
        startupStatusCalls[0].args.uidMode === "default" &&
        startupStatusCalls[0].args.uidText === "",
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
    assert(
      isValidUidText("x".repeat(64)) && !isValidUidText("x".repeat(65)),
      "UID UI validator did not enforce the 64-character limit",
    );
    assert(
      isEffectivelyEmptyUidText("\uFEFF") &&
        isEffectivelyEmptyUidText(" \uFEFF ") &&
        !isEffectivelyEmptyUidText("Halo\uFEFF"),
      "UID UI empty-state handling did not match the backend rule",
    );
    for (const separator of [
      "\n",
      "\r",
      "\u0000",
      "\u0085",
      "\u2028",
      "\u2029",
    ]) {
      assert(
        !isValidUidText(`Halo${separator}Nozomi`),
        `UID UI validator accepted line/control separator U+${separator.codePointAt(0)!.toString(16).toUpperCase()}`,
      );
    }
    let invalidUidRejected = false;
    try {
      await appState.updateUidSelection("custom", "Halo\u2028Nozomi");
    } catch {
      invalidUidRejected = true;
    }
    assert(
      invalidUidRejected,
      "LauncherState accepted a Unicode line separator",
    );

    const guardToken = appState.beginOperation("install");
    assert(guardToken, "could not start the UID operation-guard test");
    let guardedUidRejected = false;
    try {
      await appState.updateUidSelection("custom", "blocked");
    } catch {
      guardedUidRejected = true;
    } finally {
      appState.endOperation(guardToken);
    }
    assert(
      guardedUidRejected,
      "UID selection changed during an active operation",
    );

    await appState.updateUidSelection("custom", "Halo Nozomi ✦ 2026!");
    const uidStatusCalls = callsFor("check_patch_status");
    assert(
      uidStatusCalls.length === 2 &&
        uidStatusCalls[1].args.uidMode === "custom" &&
        uidStatusCalls[1].args.uidText === "Halo Nozomi ✦ 2026!",
      "custom UID selection was not sent to the backend",
    );
    assert(
      appState.config.uidMode === "custom" &&
        appState.config.uidText === "Halo Nozomi ✦ 2026!",
      "custom UID selection was not persisted in LauncherState",
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
      aliasStatusCalls.length === 3 &&
        aliasStatusCalls[2].args.gamePath === paths.legacyGamePath &&
        aliasStatusCalls[2].args.uidMode === "custom" &&
        aliasStatusCalls[2].args.uidText === "Halo Nozomi ✦ 2026!",
      "real bridge did not preserve the legacy path or UID selection in the IPC request",
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
      postSwitchStatusCalls.length === 4,
      "post-switch status was not requested",
    );
    assert(
      postSwitchStatusCalls[3].args.gamePath === paths.legacyGamePath &&
        postSwitchStatusCalls[3].args.installMethod === "loader" &&
        postSwitchStatusCalls[3].args.uidMode === "custom" &&
        postSwitchStatusCalls[3].args.uidText === "Halo Nozomi ✦ 2026!",
      "post-switch status request did not use the new method and UID selection",
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
    await invoke("fixture_emit_patch_status", {
      status: "ready",
      gamePath: paths.canonicalGamePath,
      installMethod: "loader",
      uidMode: "custom",
      uidText: "stale value",
    });
    assert(
      appState.patchState === "not_installed",
      "stale UID status event overwrote the active patch state",
    );
    const patchEvents = deliveredEvents.filter(
      (event) => event.event === "onPatchStatus",
    );
    assert(
      patchEvents.length === 5 &&
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
