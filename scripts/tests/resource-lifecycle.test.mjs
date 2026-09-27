import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import {
  countdownExpired,
  shouldRunCountdown,
} from "../../src/lib/countdown.js";

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), "../..");
const readSource = (relativePath) =>
  readFileSync(join(projectRoot, relativePath), "utf8");

// Everything under `public/` is copied verbatim into `dist/`, and Tauri
// compresses `dist/` into the launcher executable. The shipped ZIP contains
// nothing else, so this tree is a direct, per-user cost: every byte here is a
// byte in every download.
const PUBLIC_DIR = join(projectRoot, "public");
// The same icon is already embedded twice by the Windows build: once as the
// Win32 resource and once as the default window/tray icon.
const BINARY_ICON = join(projectRoot, "src-tauri", "icons", "icon.ico");
const MAX_PUBLIC_TREE_BYTES = 96 * 1024;
const MAX_PUBLIC_FILE_BYTES = 64 * 1024;

const listFiles = (root, current = root) =>
  readdirSync(current, { withFileTypes: true }).flatMap((entry) => {
    const path = join(current, entry.name);
    return entry.isDirectory() ? listFiles(root, path) : [path];
  });

const digest = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

test("BackgroundFx stops animation and removes its resize listener on cleanup", () => {
  const source = readSource("src/components/BackgroundFx.svelte");

  assert.match(source, /if \(appState\.launcherInTray\)/);
  assert.match(source, /cancelAnimationFrame\(animFrameId\)/);
  assert.match(
    source,
    /window\.removeEventListener\(['"]resize['"], handleResize\)/,
  );
  assert.match(source, /particleAnimationControl = null/);
});

test("AudioPlayer pauses and unloads media when entering tray", () => {
  const source = readSource("src/components/AudioPlayer.svelte");

  assert.match(source, /const runtimeBlocked = appState\.launcherInTray/);
  assert.match(source, /audioElement\.pause\(\)/);
  assert.match(source, /audioElement\.removeAttribute\(['"]src['"]\)/);
  assert.match(source, /audioElement\.load\(\)/);
  assert.match(source, /appState\.bgmPlaying = false/);
});

test("expired countdown stops scheduling and tray disables it", () => {
  const source = readSource("src/components/RightPanel.svelte");
  const now = Date.now();

  assert.equal(shouldRunCountdown(now + 60_000, false), true);
  assert.equal(shouldRunCountdown(now + 60_000, true), false);
  assert.equal(countdownExpired(now - 1, now), true);
  assert.equal(countdownExpired(now + 60_000, now), false);
  assert.match(
    source,
    /if \(countdownExpired\(targetDateMs, current\)\) clearInterval\(iv\)/,
  );
  assert.match(source, /return \(\) => clearInterval\(iv\)/);
});

test("resource sampler records I/O and enforces every-sample CPU and cadence limits", () => {
  const source = readSource(
    "scripts/acceptance/wut-launcher-resource.tests.ps1",
  );

  assert.match(source, /GetProcessIoCounters/);
  assert.match(source, /RequireVisibleWindow/);
  assert.match(source, /WebViewWorkingSetMB/);
  assert.match(source, /LauncherReadBytesPerSecond/);
  assert.match(source, /WebViewWriteBytesPerSecond/);
  assert.match(source, /\$minimumRequired/);
  assert.match(source, /\$nextDeadline = \$previous\.Timestamp\.AddSeconds/);
  assert.match(source, /\$nextDeadline = \$current\.Timestamp\.AddSeconds/);
  assert.match(source, /\$launcherCpuMax -gt \$MaxLauncherCpuPercent/);
  assert.match(source, /\$webviewCpuMax -gt \$MaxWebViewCpuPercent/);
  assert.match(source, /\$maxCadenceJitter -gt \$MaxCadenceJitterMilliseconds/);
});

test("the frontend payload stays small enough to ship inside the executable", () => {
  const files = listFiles(PUBLIC_DIR);
  assert.ok(files.length > 0, "public/ must not be empty");

  const sizes = files.map((path) => ({
    path: relative(projectRoot, path),
    bytes: statSync(path).size,
  }));
  for (const { path, bytes } of sizes) {
    assert.ok(
      bytes <= MAX_PUBLIC_FILE_BYTES,
      `${path} is ${bytes} bytes, over the ${MAX_PUBLIC_FILE_BYTES}-byte per-file budget`,
    );
  }
  const total = sizes.reduce((sum, { bytes }) => sum + bytes, 0);
  assert.ok(
    total <= MAX_PUBLIC_TREE_BYTES,
    `public/ is ${total} bytes, over the ${MAX_PUBLIC_TREE_BYTES}-byte budget`,
  );
});

test("the frontend never ships a second copy of the icon the binary already embeds", () => {
  const binaryIcon = digest(BINARY_ICON);
  const duplicates = listFiles(PUBLIC_DIR)
    .filter((path) => digest(path) === binaryIcon)
    .map((path) => relative(projectRoot, path));

  assert.deepEqual(
    duplicates,
    [],
    "these files duplicate src-tauri/icons/icon.ico, which tauri-build already " +
      "embeds as a Win32 resource and as the default window icon",
  );
});
