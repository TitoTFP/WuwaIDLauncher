# Window Controls and Main UI Parity Design

## Goal

Make the Tauri rebuild follow the `main` branch launcher shell exactly for the
window controls, navigation, update modal, home layout, and performance page,
while keeping the existing Tauri command/event boundary and release safety
work intact.

## Source of truth

The authoritative visual and interaction source is:

- `main:Resources/Web/index.html`
- `main:Resources/Web/styles-base.css`
- `main:Resources/Web/styles-panel.css`
- `main:Resources/Web/styles-effects.css`
- `main:Resources/Web/script-home.js`
- `main:Resources/Web/script-misc.js`
- `main:Resources/Web/script-nav.js`
- `main:Resources/Web/script-perf.js`

The active branch already carries the same CSS files in `src/styles/`; the
parity work therefore focuses on Svelte markup, state transitions, and Tauri
command adapters instead of introducing a second visual system.

## Current discrepancies

1. `TopBar.svelte` exposes `SETTINGS`, `LOGS`, and `ABOUT`, while `main` exposes
   `HOME`, `PERFORMA`, and `METODE`.
2. The active update modal renders `Versi {version}` and keeps update failures
   in a persistent right-panel status card. `main` renders the release value
   directly and reports a failed check through a transient toast.
3. The active app has no `PerformancePanel.svelte`, toast host, or admin
   permission modal corresponding to the `main` DOM.
4. The Tauri branch has no adapter for the performance page's three bridge
   operations, so copying only its controls would create new broken actions.

## Design

### Component topology

`App.svelte` renders the same visible regions as `main`:

```
BackgroundFx
TopBar (HOME / PERFORMA / METODE / minimize / close)
home: SidePanel + AudioPlayer + RightPanel
performance: PerformancePanel
UpdateModal
ToastHost
ConfirmModal
AdminModal
```

Settings, logs, and about remain available only through existing supported
actions where they are still useful, but they are removed from the top
navigation so the visible launcher matches `main`.

### State and event flow

- `LauncherState.page` becomes `home | performance`.
- `onLauncherUpdateError` clears update-modal state and emits a toast; it does
  not populate the persistent home status card for an automatic check.
- Explicit user actions may still emit a toast with the backend diagnostic.
- Existing launch/install diagnostics continue to be written to the current
  log/evidence paths; UI presentation uses the `main` toast pattern.
- The update modal keeps the existing secure ZIP/checksum command contract and
  only changes presentation and lifecycle state.

### Performance page

Port the exact `main` panel structure and labels into a focused
`PerformancePanel.svelte`. Launcher visual mode uses the existing
`launcherVisualMode` setting. The fourteen game optimization toggles use a
typed `PerformanceConfig` object and three new Tauri commands:

- `get_performance_config_active(game_path) -> bool`
- `apply_performance_config(game_path, settings_json) -> String`
- `clear_performance_config(game_path) -> String`

The Rust implementation preserves the `main` behavior: backup
`Client/Saved/Config/WindowsNoEditor/Engine.ini` once, remove/rewrite only the
managed keys, preserve unrelated INI content, and restore/delete the backup on
clear. Calls reject while the game is running.

### Window controls

- `close_window` retains the existing signature-restore guard, hides the
  launcher while the game is active, and exits otherwise.
- `minimize_window` hides the launcher while the game is active and minimizes
  it otherwise, matching `MainWindow.RequestMinimizeWindow`.
- Both command names are added to the application-owned ACL only; no broad
  process, shell, or filesystem permission is added.

## Verification

- A contract test must fail before the ACL change when either window command is
  missing from the source and generated manifest.
- Rust unit tests cover INI managed-key replacement, unrelated-key
  preservation, backup/restore, and active-game rejection.
- `npm run check` validates the Svelte component and typed bridge contracts.
- Existing Rust integration/contracts and `git diff --check` remain green.
- A release build regenerates the ACL manifest and is smoke-tested by starting
  the release binary, clicking minimize/close, opening the update modal, and
  visiting the Performa page.

## Non-goals

- No checkout, reset, or commit.
- No visual redesign beyond the `main` source of truth.
- No change to the existing secure self-update verification policy.
