import assert from "node:assert/strict";
import {
  createPrivateKey,
  createPublicKey,
  verify as verifyBytes,
} from "node:crypto";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const signerPath = path.join(repoRoot, "scripts/sign-manifest.mjs");

test("generated key id is reused to sign the manifest without overwriting keys", (t) => {
  const directory = mkdtempSync(path.join(tmpdir(), "wuwaid-sign-manifest-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));

  const keyPath = path.join(directory, "wuwa-web-2026-02.key.pem");
  const keyIdPath = path.join(directory, "wuwa-web-2026-02.key-id");
  const publicKeyPath = path.join(directory, "wuwa-web-2026-02.pub.hex");
  const manifestPath = path.join(directory, "assets.json");
  const manifestBytes = Buffer.from('{"assets":[]}\n');
  const keyId = "wuwa-web-2026-02";
  writeFileSync(manifestPath, manifestBytes);

  const generated = runSigner([
    "--generate",
    "--key",
    keyPath,
    "--key-id",
    keyId,
  ]);
  assert.equal(generated.status, 0, generated.stderr);
  assert.equal(readFileSync(keyIdPath, "utf8").trim(), keyId);
  assert.match(readFileSync(publicKeyPath, "utf8").trim(), /^[a-f0-9]{64}$/);
  if (process.platform !== "win32") {
    assert.equal(statSync(keyPath).mode & 0o777, 0o600);
  }

  const privateKeyBytes = readFileSync(keyPath);
  const signed = runSigner(["--in", manifestPath, "--key", keyPath]);
  assert.equal(signed.status, 0, signed.stderr);
  const [header, signedKeyId, signatureHex] = readFileSync(
    `${manifestPath}.sig`,
    "utf8",
  )
    .trim()
    .split(/\s+/);
  assert.equal(header, "wuwaid-manifest-v1");
  assert.equal(signedKeyId, keyId);
  assert.equal(
    verifyBytes(
      null,
      manifestBytes,
      createPublicKey(createPrivateKey(privateKeyBytes)),
      Buffer.from(signatureHex, "hex"),
    ),
    true,
  );

  const duplicateGeneration = runSigner([
    "--generate",
    "--key",
    keyPath,
    "--key-id",
    "wuwa-web-2026-03",
  ]);
  assert.notEqual(duplicateGeneration.status, 0);
  assert.match(duplicateGeneration.stderr, /refusing to overwrite/);
  assert.deepEqual(readFileSync(keyPath), privateKeyBytes);
});

function runSigner(args) {
  const result = spawnSync(process.execPath, [signerPath, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.ifError(result.error);
  return result;
}
