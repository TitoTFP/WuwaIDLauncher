import test from "node:test";
import assert from "node:assert/strict";
import {
  compactText,
  createGameExitDeduper,
  gameExitToast,
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
