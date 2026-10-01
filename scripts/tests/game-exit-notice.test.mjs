import test from "node:test";
import assert from "node:assert/strict";
import {
  compactText,
  createGameExitDeduper,
  gameExitToast,
  GAME_EXIT_LABELS,
} from "../../src/lib/gameExitNotice.js";

test("duplicate exit events create one accepted notification", () => {
  const accept = createGameExitDeduper();
  const payload = {
    id: "1700000000000:42",
    status: "crashed",
    reason: "exit code -1073741819",
  };

  assert.equal(accept(payload), true);
  assert.equal(accept({ ...payload }), false);
  assert.equal(
    gameExitToast(payload).message,
    "Game berhenti: exit code -1073741819",
  );
});

test("duplicate exit IDs stay suppressed after another exit arrives", () => {
  const accept = createGameExitDeduper();

  assert.equal(
    accept({ id: "first:42", status: "normal", reason: "selesai" }),
    true,
  );
  assert.equal(
    accept({ id: "second:42", status: "force_quit", reason: "dihentikan" }),
    true,
  );
  assert.equal(
    accept({ id: "first:42", status: "normal", reason: "selesai lagi" }),
    false,
  );
});

test("exit deduper evicts the oldest IDs at its memory bound", () => {
  const accept = createGameExitDeduper();
  const payload = (id) => ({ id, status: "normal", reason: "selesai" });

  for (let index = 0; index < 256; index += 1) {
    assert.equal(accept(payload(`exit:${index}`)), true);
  }
  assert.equal(accept(payload("exit:256")), true);
  assert.equal(accept(payload("exit:0")), true);

  assert.equal(accept(payload("exit:255")), false);
});

test("exit reasons are compacted without splitting Unicode characters", () => {
  const reason = "🙂 ".repeat(120);
  const compact = compactText(reason, 10);

  assert.equal(Array.from(compact).length, 10);
  assert.equal(compact.endsWith("…"), true);
});

test("every backend exit status has a label and a correct kind", () => {
  // Mirrors the statuses `complete_launcher_exit` can emit from
  // src-tauri/src/lib.rs. Nothing links the two lists at compile time, so this
  // key-set comparison is the guard against a status added on one side only.
  const backendStatuses = ["normal", "crashed", "force_quit", "not_started"];
  assert.deepEqual(
    Object.keys(GAME_EXIT_LABELS).sort(),
    [...backendStatuses].sort(),
  );

  // A status that reaches the toast without a label degrades to a generic one
  // rather than rendering `undefined: <reason>`.
  assert.equal(
    gameExitToast({ id: "u:1", status: "brand_new", reason: "rincian" }).message,
    "Game berakhir: rincian",
  );

  const kindOf = (status) =>
    gameExitToast({ id: `${status}:1`, status, reason: "rincian" }).kind;
  assert.equal(kindOf("normal"), "info");
  assert.equal(kindOf("force_quit"), "info");
  assert.equal(kindOf("crashed"), "err");
  assert.equal(kindOf("not_started"), "err");

  assert.equal(
    gameExitToast({
      id: "ns:1",
      status: "not_started",
      reason: "Proses game tidak pernah dimulai",
    }).message,
    "Game tidak dimulai: Proses game tidak pernah dimulai",
  );
});
