import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  beginVolumePointer,
  cancelVolume,
  commitVolume,
  createVolumeState,
  previewVolume,
  volumeView,
} from "../../src/lib/volumeControl.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (file) => readFileSync(join(root, file), "utf8");

test("volume updates live and persists only on release/change", () => {
  const source = read("src/components/AudioPlayer.svelte");

  assert.match(source, /onpointerdown=\{handleVolumePointerDown\}/);
  assert.match(source, /oninput=\{handleVolumeInput\}/);
  assert.match(source, /onpointerup=\{handleVolumeChange\}/);
  assert.match(source, /onchange=\{handleVolumeChange\}/);
  assert.match(source, /onpointercancel=\{handleVolumeCancel\}/);
  assert.match(source, /aria-label="Volume musik"/);

  const input = source.match(
    /function handleVolumeInput\([\s\S]*?\n {2}}/,
  )?.[0];
  assert.ok(input, "handleVolumeInput must remain the realtime path");
  assert.match(input, /applyVolumePreview\(value\)/);
  assert.doesNotMatch(input, /commitVolumeState|persistAudioConfig/);

  const apply = source.match(/function applyVolumeState\([\s\S]*?\n {2}}/)?.[0];
  assert.ok(apply, "applyVolumeState must update the live UI/audio state");
  assert.match(apply, /volumePercent = view\.percent/);
  assert.match(apply, /isMuted = view\.muted/);
  assert.match(apply, /syncAudioVolume\(\)/);

  const sync = source.match(/function syncAudioVolume\([\s\S]*?\n {2}}/)?.[0];
  assert.ok(sync, "syncAudioVolume must remain the audio application path");
  assert.match(sync, /audioElement\.volume = view\.audioVolume/);
  assert.match(sync, /audioElement\.muted = view\.muted/);

  const commit = source.match(
    /function handleVolumeChange\([\s\S]*?\n {2}}/,
  )?.[0];
  assert.ok(commit, "handleVolumeChange must remain the commit path");
  assert.match(commit, /commitVolumeState\(volumeState, value/);
  assert.match(commit, /appState\.config\.bgmVolume/);
  assert.match(commit, /persistAudioConfig\(\)/);

  let state = beginVolumePointer(createVolumeState(35));
  state = previewVolume(state, 80);
  assert.equal(state.savedValue, 35);
  assert.deepEqual(volumeView(state), {
    percent: 80,
    muted: false,
    audioVolume: 0.8,
  });

  const persisted = [];
  const pointerUp = commitVolume(state, 80, (volume) => persisted.push(volume));
  const change = commitVolume(pointerUp.state, 80, (volume) =>
    persisted.push(volume),
  );
  assert.equal(pointerUp.changed, true);
  assert.equal(change.changed, false);
  assert.deepEqual(persisted, [0.8]);
});

test("volume pointer cancellation restores the saved state", () => {
  let state = beginVolumePointer(createVolumeState(40, true));
  state = previewVolume(state, 75);
  assert.deepEqual(volumeView(state), {
    percent: 75,
    muted: false,
    audioVolume: 0.75,
  });

  const cancelled = cancelVolume(state);
  assert.equal(cancelled.value, 40);
  assert.equal(cancelled.savedValue, 40);
  assert.equal(cancelled.muted, true);
  assert.deepEqual(volumeView(cancelled), {
    percent: 40,
    muted: true,
    audioVolume: 0,
  });
});

test("settings close control uses a symmetric SVG X", () => {
  const source = read("src/components/SettingsOverlay.svelte");
  const closeButton = source.match(
    /<button class="settings-close"[\s\S]*?<\/button>/,
  )?.[0];

  assert.ok(closeButton, "settings close button must exist");
  assert.match(closeButton, /aria-label="Tutup pengaturan"/);
  assert.match(closeButton, /M6 6l12 12M18 6L6 18/);
  assert.match(closeButton, /stroke="currentColor"/);
  assert.doesNotMatch(closeButton, /d="m18\.3 5\.7/);
});

test("icon-only controls expose accessible names", () => {
  const topBar = read("src/components/TopBar.svelte");
  const audio = read("src/components/AudioPlayer.svelte");
  const sidePanel = read("src/components/SidePanel.svelte");
  const rightPanel = read("src/components/RightPanel.svelte");

  assert.match(topBar, /id="btnMinimize"[^>]*aria-label="Minimalkan"/);
  assert.match(topBar, /id="btnClose"[^>]*aria-label="Tutup launcher"/);
  assert.match(
    audio,
    /aria-label=\{isPlaying \? 'Jeda musik' : 'Putar musik'\}/,
  );
  assert.match(
    audio,
    /aria-label=\{isMuted \? 'Aktifkan suara' : 'Bisukan musik'\}/,
  );
  assert.match(
    sidePanel,
    /id="rnToggle"[\s\S]*aria-label=\{collapsed \? 'Buka pengumuman' : 'Tutup pengumuman'\}/,
  );
  assert.match(
    rightPanel,
    /id="btnMenu"[\s\S]*aria-label="Buka menu"[\s\S]*aria-controls="rpDropdown"/,
  );
});

test("Tauri titlebar dragging does not create a native drag cursor region", () => {
  const topBar = read("src/components/TopBar.svelte");
  const baseStyles = read("src/styles/styles-base.css");
  const settings = read("src/components/SettingsOverlay.svelte");
  const capabilities = read("src-tauri/capabilities/default.json");

  assert.match(topBar, /getCurrentWindow\(\)\.startDragging\(\)/);
  assert.match(
    topBar,
    /topBar\.addEventListener\('mousedown', handleWindowDrag\)/,
  );
  assert.doesNotMatch(topBar, /data-tauri-drag-region/);
  assert.match(
    baseStyles,
    /\.top-bar\s*\{[\s\S]*cursor: var\(--cursor-custom\);/,
  );
  assert.match(baseStyles, /\.top-bar\s*\{[\s\S]*-webkit-app-region: no-drag;/);
  assert.doesNotMatch(baseStyles, /-webkit-app-region:\s*drag;/);
  assert.doesNotMatch(settings, /cursor:\s*default;/);
  for (const selector of ["uid-mode-card", "method-card"]) {
    const disabled = settings.match(
      new RegExp(`\\.${selector}:disabled\\s*\\{[\\s\\S]*?\\}`),
    )?.[0];
    assert.ok(disabled, `${selector} disabled rule must exist`);
    assert.match(disabled, /cursor:\s*var\(--cursor-select\);/);
  }
  assert.match(capabilities, /"core:window:allow-start-dragging"/);
});

test("Node wrappers do not enable shell execution", () => {
  for (const path of [
    "scripts/run-frontend-gate.mjs",
    "scripts/run-tauri.mjs",
  ]) {
    const source = read(path);
    assert.match(source, /shell: false/);
    assert.doesNotMatch(source, /shell:\s*true/);
  }
});
