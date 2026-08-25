import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  countdownExpired,
  shouldRunCountdown,
} from "../../src/lib/countdown.js";

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), "../..");
const readSource = (relativePath) =>
  readFileSync(join(projectRoot, relativePath), "utf8");

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
  assert.match(source, /LauncherReadBytesPerSecond/);
  assert.match(source, /WebViewWriteBytesPerSecond/);
  assert.match(source, /\$minimumRequired/);
  assert.match(source, /\$nextDeadline = \$previous\.Timestamp\.AddSeconds/);
  assert.match(source, /\$nextDeadline = \$current\.Timestamp\.AddSeconds/);
  assert.match(source, /\$launcherCpuMax -gt \$MaxLauncherCpuPercent/);
  assert.match(source, /\$webviewCpuMax -gt \$MaxWebViewCpuPercent/);
  assert.match(source, /\$maxCadenceJitter -gt \$MaxCadenceJitterMilliseconds/);
});
