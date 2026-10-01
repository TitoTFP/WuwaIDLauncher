/**
 * @typedef {"ok" | "err" | "info"} ToastKind
 * @typedef {{ id: string, status: "normal" | "crashed" | "force_quit" | "not_started", reason: string }} GameExitPayload
 * @typedef {{ message: string, kind: ToastKind }} GameExitToast
 */

/**
 * Keep exit events one-shot per launcher session. The backend can emit the
 * same lifecycle notification again while a tray/window transition settles;
 * only the first event for an exit id should reach the toast queue.
 *
 * @returns {(payload: GameExitPayload) => boolean}
 */
export function createGameExitDeduper() {
 const maxSeenIds = 256;
 const seenIds = new Set();
 return (payload) => {
  if (!payload.id || seenIds.has(payload.id)) return false;
  if (seenIds.size >= maxSeenIds) {
   const oldestId = seenIds.values().next().value;
   if (oldestId !== undefined) seenIds.delete(oldestId);
  }
  seenIds.add(payload.id);
  return true;
 };
}

/**
 * @param {string} value
 * @param {number} [maxLength]
 */
export function compactText(value, maxLength = 180) {
 const compact = value.replace(/\s+/g, " ").trim();
 const characters = Array.from(compact);
 return characters.length > maxLength
  ? `${characters.slice(0, maxLength - 1).join("")}…`
  : compact;
}

/**
 * Labels for the statuses the backend can emit from `complete_launcher_exit`.
 * That function takes a bare `&'static str`, so no type checker compares this
 * map with the Rust call sites. `gameExitToast` falls back to a generic label
 * so a status added in `lib.rs` and missed here costs a vague toast rather than
 * a broken `undefined: <reason>` one, and `game-exit-notice.test.mjs` pins the
 * key set so the two lists cannot drift apart silently.
 */
export const GAME_EXIT_LABELS = {
 normal: "Game ditutup",
 crashed: "Game berhenti",
 force_quit: "Game dipaksa tutup",
 not_started: "Game tidak dimulai",
};

/** Statuses that must read as a failure rather than as plain information. */
const FAILURE_STATUSES = new Set(["crashed", "not_started"]);

/**
 * @param {GameExitPayload} payload
 * @returns {GameExitToast}
 */
export function gameExitToast(payload) {
 const reason = compactText(payload.reason) || "Tidak ada detail tambahan.";
 return {
  message: `${GAME_EXIT_LABELS[payload.status] ?? "Game berakhir"}: ${reason}`,
  kind: FAILURE_STATUSES.has(payload.status) ? "err" : "info",
 };
}
