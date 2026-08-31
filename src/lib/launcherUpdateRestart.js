/** @typedef {"ok" | "err" | "info"} ToastKind */

const RESTART_STATUS = "Launcher akan tertutup otomatis lalu dibuka kembali.";

/**
 * @param {{
 *   launcherUpdateError: string;
 *   launcherUpdateRestartCountdown: number | null;
 *   launcherUpdateStatus: string;
 *   statusMessage: string;
 *   diagnosticMessage: string;
 * }} state
 * @param {number} remainingSeconds
 * @returns {number} The normalized countdown value.
 */
function applyLauncherUpdateRestarting(state, remainingSeconds) {
 const countdown = Math.max(0, Math.floor(remainingSeconds));
 state.launcherUpdateError = "";
 state.launcherUpdateRestartCountdown = countdown;
 state.launcherUpdateStatus =
  countdown === 0
   ? "Launcher sedang dimulai ulang..."
   : `Update selesai diunduh. Launcher akan tertutup otomatis dan dibuka kembali dalam ${countdown} detik.`;
 if (countdown > 0) {
  state.statusMessage = RESTART_STATUS;
  state.diagnosticMessage = RESTART_STATUS;
 }
 return countdown;
}

/**
 * Keep the launcher-update lifecycle callbacks together so the event bridge
 * and regression tests exercise the same staged/countdown behavior.
 *
 * @param {{
 *   launcherUpdateError: string;
 *   launcherUpdateRestartCountdown: number | null;
 *   launcherUpdateStatus: string;
 *   statusMessage: string;
 *   diagnosticMessage: string;
 *   showToast: (message: string, kind: ToastKind) => void;
 * }} state
 * @param {() => void} endLauncherUpdate
 * @returns {{
 *   onLauncherUpdateRestarting: (remainingSeconds: number) => void;
 *   onLauncherUpdateStaged: () => void;
 * }}
 */
export function createLauncherUpdateEventHandlers(state, endLauncherUpdate) {
 return {
  onLauncherUpdateRestarting: (remainingSeconds) => {
   const countdown = applyLauncherUpdateRestarting(state, remainingSeconds);
   if (countdown === 0) endLauncherUpdate();
  },
  onLauncherUpdateStaged: () => {
   state.launcherUpdateStatus = "Update terverifikasi dan siap diterapkan.";
   state.showToast("Update launcher sudah diverifikasi.", "ok");
  },
 };
}
