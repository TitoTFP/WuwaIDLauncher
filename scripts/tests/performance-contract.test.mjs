import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const readSource = (relativePath) =>
  readFileSync(join(root, relativePath), "utf8");

const performanceRunner = () =>
  readSource("scripts/acceptance/run-windows-fixture-performance.ps1");
const resourceSampler = () =>
  readSource("scripts/acceptance/wut-launcher-resource.tests.ps1");

test("fixture performance runner covers visible, tray, and restore evidence", () => {
  const source = performanceRunner();

  assert.match(source, /VisibleDurationSeconds/);
  assert.match(source, /TrayDurationSeconds/);
  assert.match(source, /Invoke-ResourceSample/);
  assert.match(source, /Scenario "visible"/);
  assert.match(source, /Scenario "tray"/);
  assert.match(source, /visibleToTray/);
  assert.match(source, /trayToVisibleRestore/);
  assert.match(source, /resource-visible\.csv/);
  assert.match(source, /resource-tray\.csv/);
  assert.match(source, /summary\.json/);
  assert.match(source, /Write-MinimalValidPak/);
  assert.match(source, /installMethod = "loader"/);
  assert.match(source, /_loaderSha256/);
  assert.match(source, /MaxLauncherPrivateMemoryMB/);
  assert.match(source, /MaxLauncherWorkingSetMB/);
  assert.match(source, /MaxLauncherReadBytesPerSecond/);
  assert.match(source, /MaxWebViewWriteBytesPerSecond/);
  assert.doesNotMatch(source, /run-windows-real-acceptance\.ps1/);
});

test("resource sampler exposes visible-state and WebView working-set evidence", () => {
  const source = resourceSampler();

  assert.match(source, /RequireVisibleWindow/);
  assert.match(source, /WebViewWorkingSetBytes/);
  assert.match(source, /WebViewWorkingSetMB/);
  assert.match(source, /LauncherReadBytesPerSecond/);
  assert.match(source, /WebViewWriteBytesPerSecond/);
  assert.match(source, /MaxLauncherMemoryGrowthMB/);
  assert.match(source, /maxCadenceJitter/);
});

test("fixture lifetime is controllable without a real game", () => {
  const source = readSource(
    "src-tauri/tests/support/wut-game-lifecycle-fixture.rs",
  );

  assert.match(source, /WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS/);
});

test("CI runs the fixture matrix on a hosted Windows runner and uploads evidence", () => {
  const workflow = readSource(".github/workflows/ci.yml");

  assert.match(workflow, /runs-on:\s*windows-latest/);
  assert.match(workflow, /run-windows-fixture-performance\.ps1/);
  assert.match(
    workflow,
    /windows-fixture-performance-\$\{\{ github\.run_number \}\}/,
  );
  assert.match(workflow, /path:\s*performance-evidence/);
  assert.doesNotMatch(workflow, /self-hosted/i);
});

test("performance documentation states the matrix boundary", () => {
  const path = join(root, "docs/launcher-performance.md");
  assert.ok(existsSync(path), "performance documentation must exist");
  const docs = readFileSync(path, "utf8");

  for (const marker of [
    "Visible foreground",
    "System tray",
    "Thresholds",
    "windows-latest",
    "real game",
    "UAC",
    "self-update",
  ]) {
    assert.match(
      docs,
      new RegExp(marker, "i"),
      `missing documentation marker: ${marker}`,
    );
  }
});
