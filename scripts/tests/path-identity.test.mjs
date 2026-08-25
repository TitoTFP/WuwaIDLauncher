import assert from "node:assert/strict";
import test from "node:test";
import { samePath } from "../../src/lib/pathIdentity.js";

test("matches legacy and canonical Windows path identities", () => {
  assert.equal(
    samePath(String.raw`C:\Games\Wuwa`, String.raw`\\?\C:\Games\Wuwa`),
    true,
  );
  assert.equal(
    samePath(
      String.raw`\\?\UNC\server\share\Wuwa`,
      String.raw`\\server\share\Wuwa`,
    ),
    true,
  );
});

test("keeps distinct game paths distinct", () => {
  assert.equal(
    samePath(String.raw`C:\Games\Wuwa`, String.raw`C:\Games\Other`),
    false,
  );
});
