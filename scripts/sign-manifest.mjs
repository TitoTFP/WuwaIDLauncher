#!/usr/bin/env node
// Signs the asset manifest with the web content Ed25519 key.
//
//   node scripts/sign-manifest.mjs --generate
//   node scripts/sign-manifest.mjs --in Web/assets.json
//
// The signature covers the manifest file's exact bytes and is written to
// `<manifest>.sig`. The launcher verifies it against the public keys compiled
// into the binary (src-tauri/src/engine/theme.rs) before it will honour any
// theme the manifest declares.

import { createPrivateKey, createPublicKey, generateKeyPairSync, sign as signBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const KEY_DIR = join(REPO_ROOT, "scripts", "keys");
const DEFAULT_KEY = join(KEY_DIR, "web-manifest-2026-02.key.pem");
const DEFAULT_KEY_ID_FILE = join(KEY_DIR, "web-manifest-2026-02.key-id");
const DEFAULT_MANIFEST = join(REPO_ROOT, "Web", "assets.json");
const SIGNATURE_HEADER = "wuwaid-manifest-v1";
const FALLBACK_KEY_ID = "wuwa-web-2026-02";

function parseArgs(argv) {
  const args = { key: DEFAULT_KEY, in: DEFAULT_MANIFEST, out: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--generate") {
      args.generate = true;
    } else if (arg === "--help" || arg === "-h") {
      args.help = true;
    } else if (arg.startsWith("--")) {
      const value = argv[i + 1];
      if (value === undefined) throw new Error(`Missing value for ${arg}`);
      args[arg.slice(2)] = value;
      i += 1;
    } else {
      throw new Error(`Unexpected argument: ${arg}`);
    }
  }
  return args;
}

function usage() {
  process.stdout.write(
    [
      "Usage:",
      "  node scripts/sign-manifest.mjs --generate [--key <pem>] [--key-id <id>]",
      "  node scripts/sign-manifest.mjs [--in Web/assets.json] [--key <pem>] [--key-id <id>]",
      "",
    ].join("\n"),
  );
}

function publicKeyHex(privateKeyPath) {
  const privateKey = createPrivateKey(readFileSync(privateKeyPath));
  return rawPublicKeyHex(createPublicKey(privateKey));
}

function rawPublicKeyHex(publicKey) {
  const spki = publicKey.export({ format: "der", type: "spki" });
  return Buffer.from(spki).subarray(-32).toString("hex");
}

function keySidecarPath(privateKeyPath, suffix) {
  if (suffix === "key-id" && resolve(privateKeyPath) === DEFAULT_KEY) {
    return DEFAULT_KEY_ID_FILE;
  }
  const filename = basename(privateKeyPath);
  const stem = filename.endsWith(".key.pem") ? filename.slice(0, -8) : filename;
  return join(dirname(privateKeyPath), `${stem}.${suffix}`);
}

function validateKeyId(keyId) {
  if (keyId.length > 64 || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(keyId)) {
    throw new Error(`Invalid key id: ${keyId}`);
  }
}

function generateKeyPair(args) {
  const keyPath = resolve(args.key);
  const keyId = args["key-id"] || FALLBACK_KEY_ID;
  validateKeyId(keyId);

  const keyIdPath = keySidecarPath(keyPath, "key-id");
  const publicKeyPath = keySidecarPath(keyPath, "pub.hex");
  for (const artifactPath of [keyPath, keyIdPath, publicKeyPath]) {
    if (existsSync(artifactPath)) {
      throw new Error(`Key artifact already exists at ${artifactPath}; refusing to overwrite.`);
    }
  }

  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  mkdirSync(dirname(keyPath), { recursive: true, mode: 0o700 });
  writeFileSync(keyPath, privateKey.export({ format: "pem", type: "pkcs8" }), {
    flag: "wx",
    mode: 0o600,
  });
  const hex = rawPublicKeyHex(publicKey);
  writeFileSync(keyIdPath, `${keyId}\n`, { flag: "wx", mode: 0o600 });
  writeFileSync(publicKeyPath, `${hex}\n`, { flag: "wx", mode: 0o600 });

  process.stdout.write(
    [
      `Private key: ${keyPath} (git-ignored, keep it offline)`,
      `Key id file: ${keyIdPath}`,
      `Public key:  ${hex}`,
      `Public file: ${publicKeyPath}`,
      "",
      "Add the public key to TRUSTED_SIGNING_KEYS in src-tauri/src/engine/theme.rs",
      "before shipping a launcher build that should trust it.",
      "",
    ].join("\n"),
  );
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    usage();
    return;
  }
  if (args.generate) {
    generateKeyPair(args);
    return;
  }

  const keyPath = resolve(args.key);
  const manifestPath = resolve(args.in);
  if (!existsSync(keyPath)) {
    throw new Error(`Signing key not found: ${keyPath}. Run with --generate first.`);
  }
  if (!existsSync(manifestPath)) {
    throw new Error(`Manifest not found: ${manifestPath}`);
  }

  const keyId = args["key-id"] || readKeyId(keyPath);
  validateKeyId(keyId);
  // The signature covers the bytes GitHub serves, which are the committed blob.
  // A Windows checkout with core.autocrlf=true rewrites them to CRLF, so sign
  // the normalised form — otherwise verification fails with no visible error.
  const onDisk = readFileSync(manifestPath);
  const manifestBytes = Buffer.from(onDisk.toString("utf8").replace(/\r\n/g, "\n"), "utf8");
  if (onDisk.length !== manifestBytes.length) {
    process.stderr.write(
      "Warning: CRLF line endings were normalised to LF before signing. " +
        "Add `*.json text eol=lf` to .gitattributes in the asset repository.\n",
    );
  }
  const privateKey = createPrivateKey(readFileSync(keyPath));
  const signature = signBytes(null, manifestBytes, privateKey).toString("hex");
  const signaturePath = resolve(args.out || `${manifestPath}.sig`);
  writeFileSync(signaturePath, `${SIGNATURE_HEADER} ${keyId} ${signature}\n`);

  process.stdout.write(
    [
      `Signed ${basename(manifestPath)} (${manifestBytes.length} bytes)`,
      `  key id:  ${keyId}`,
      `  output:  ${signaturePath}`,
      `  public:  ${publicKeyHex(keyPath)}`,
      "",
    ].join("\n"),
  );
}

function readKeyId(privateKeyPath) {
  const keyIdPath = keySidecarPath(privateKeyPath, "key-id");
  if (!existsSync(keyIdPath)) return FALLBACK_KEY_ID;
  const id = readFileSync(keyIdPath, "utf8").trim();
  return id || FALLBACK_KEY_ID;
}

try {
  main();
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
}
