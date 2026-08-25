/**
 * @typedef {"ok" | "err" | "info"} ToastKind
 * @typedef {{ id: string, status: "normal" | "crashed" | "force_quit", reason: string }} GameExitPayload
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
 * @param {GameExitPayload} payload
 * @returns {GameExitToast}
 */
export function gameExitToast(payload) {
 const labels = {
  normal: "Game ditutup",
  crashed: "Game berhenti",
  force_quit: "Game dipaksa tutup",
 };
 const reason = compactText(payload.reason) || "Tidak ada detail tambahan.";
 return {
  message: `${labels[payload.status]}: ${reason}`,
  kind: payload.status === "crashed" ? "err" : "info",
 };
}
