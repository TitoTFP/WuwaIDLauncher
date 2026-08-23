import assert from "node:assert/strict";
import test from "node:test";

import { createPatchStatusWaiter } from "../../src/lib/patchStatusWait.js";

test("patch status waiter resolves when the backend event arrives", async () => {
  const waiter = createPatchStatusWaiter(100);

  waiter.resolve();

  await waiter.promise;
});

test("patch status waiter rejects when the backend event is missing", async () => {
  const waiter = createPatchStatusWaiter(5);

  await assert.rejects(waiter.promise, /Waktu tunggu status patch habis/);
});
