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
const DEFAULT_KEY = join(KEY_DIR, "web-manifest.key.pem");
const DEFAULT_KEY_ID_FILE = join(KEY_DIR, "web-manifest.key-id");
const DEFAULT_MANIFEST = join(REPO_ROOT, "Web", "assets.json");
const SIGNATURE_HEADER = "wuwaid-manifest-v1";
const FALLBACK_KEY_ID = "wuwa-web-2026-01";

function parseArgs(argv) {
  const args = { key: DEFAULT_KEY, in: DEFAULT_MANIFEST, out: null, keyId: null };
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
      "  node scripts/sign-manifest.mjs --generate",
      "  node scripts/sign-manifest.mjs [--in Web/assets.json] [--key <pem>] [--key-id <id>]",
      "",
    ].join("\n"),
  );
}

function publicKeyHex(privateKeyPath) {
  const privateKey = createPrivateKey(readFileSync(privateKeyPath));
  const spki = createPublicKey(privateKey).export({ format: "der", type: "spki" });
  return Buffer.from(spki).subarray(-32).toString("hex");
}

function generateKeyPair() {
  if (existsSync(DEFAULT_KEY)) {
    process.stderr.write(`Key already exists at ${DEFAULT_KEY}; refusing to overwrite.\n`);
    process.exitCode = 1;
    return;
  }
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  mkdirSync(KEY_DIR, { recursive: true });
  writeFileSync(DEFAULT_KEY, privateKey.export({ format: "pem", type: "pkcs8" }), {
    mode: 0o600,
  });
  const hex = Buffer.from(publicKey.export({ format: "der", type: "spki" }))
    .subarray(-32)
    .toString("hex");
  writeFileSync(DEFAULT_KEY_ID_FILE, `${FALLBACK_KEY_ID}\n`);
  process.stdout.write(
    [
      `Private key: ${DEFAULT_KEY} (git-ignored, keep it offline)`,
      `Public key:  ${hex}`,
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
    generateKeyPair();
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

  const keyId = args["key-id"] || readKeyId();
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

function readKeyId() {
  if (!existsSync(DEFAULT_KEY_ID_FILE)) return FALLBACK_KEY_ID;
  const id = readFileSync(DEFAULT_KEY_ID_FILE, "utf8").trim();
  return id || FALLBACK_KEY_ID;
}

try {
  main();
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
}
