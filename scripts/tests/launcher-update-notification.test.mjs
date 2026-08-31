import test from "node:test";
import assert from "node:assert/strict";
import { createLauncherUpdateEventHandlers } from "../../src/lib/launcherUpdateRestart.js";

const verificationToast = {
  id: 1,
  message: "Update launcher sudah diverifikasi.",
  kind: "ok",
};

function createState() {
  return {
    launcherUpdateError: "previous error",
    launcherUpdateRestartCountdown: null,
    launcherUpdateStatus: "",
    statusMessage: "",
    diagnosticMessage: "",
    toasts: [],
    toastCalls: [],
    toastSequence: 0,
    setStatus(message) {
      this.statusMessage = message;
      this.diagnosticMessage = message;
      this.showToast(message, "info");
    },
    showToast(message, kind) {
      this.toastCalls.push({ message, kind });
      this.toasts = [
        ...this.toasts,
        { id: ++this.toastSequence, message, kind },
      ];
    },
  };
}

test("real staged and restart events keep one update toast", () => {
  const state = createState();
  let endedOperations = 0;
  const handlers = createLauncherUpdateEventHandlers(state, () => {
    endedOperations += 1;
  });

  handlers.onLauncherUpdateStaged();
  for (
    let remainingSeconds = 12;
    remainingSeconds >= 0;
    remainingSeconds -= 1
  ) {
    handlers.onLauncherUpdateRestarting(remainingSeconds);
  }

  assert.deepEqual(state.toastCalls, [
    { message: verificationToast.message, kind: verificationToast.kind },
  ]);
  assert.deepEqual(state.toasts, [verificationToast]);
  assert.equal(state.launcherUpdateRestartCountdown, 0);
  assert.equal(state.launcherUpdateStatus, "Launcher sedang dimulai ulang...");
  assert.equal(
    state.statusMessage,
    "Launcher akan tertutup otomatis lalu dibuka kembali.",
  );
  assert.equal(endedOperations, 1);
});

test("restart events clamp fractional and negative payloads", () => {
  const state = createState();
  let endedOperations = 0;
  const handlers = createLauncherUpdateEventHandlers(state, () => {
    endedOperations += 1;
  });

  handlers.onLauncherUpdateRestarting(3.9);
  assert.equal(state.launcherUpdateRestartCountdown, 3);
  handlers.onLauncherUpdateRestarting(-1);
  assert.equal(state.launcherUpdateRestartCountdown, 0);
  assert.equal(endedOperations, 1);
});
