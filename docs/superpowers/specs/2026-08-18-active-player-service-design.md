# Active Player Service for the Tauri Launcher

**Date:** 2026-08-18

## Goal

Bring the anonymous active-player heartbeat behavior from the `main` branch to
the Tauri launcher without reintroducing remote diagnostics or blocking the
launcher UI. The service reports launcher presence and launch events so the
existing active-player endpoint can count recent clients.

## Scope and constraints

- The endpoint is
  `https://logs.titotfp.my.id/api/active/heartbeat`.
- The service sends JSON only for `open`, `launch`, and periodic `heartbeat`
  events.
- The payload contains only `client_id`, `launcher_version`,
  `install_method`, and `event`. It must not contain a game path, Windows user
  name, account identifier, file contents, or diagnostic logs.
- `client_id` is generated once and persisted in the launcher app-data
  directory as `active-client-id.txt`; existing non-empty values up to 128
  characters are reused.
- The service starts after the existing `notify_ui_interactive` startup
  milestone, sends `open` immediately, and sends `heartbeat` every five
  minutes while the launcher is running.
- A successful `launch_game` request sends one `launch` event. It uses the
  normalized install method selected for that launch.
- Requests are best-effort: an eight-second timeout, cancellation, transport
  errors, HTTP errors, and malformed local identifier state are logged locally
  and never produce a UI error or prevent game launch.
- The service stops when the launcher exits. Existing local game-process
  monitoring remains the source of truth for active runtime state; this
  service does not inspect or upload game telemetry.

## Design

### Service boundary

Add a focused Rust module, `engine::active_player`, with a small stateful
service managed by Tauri. It owns the HTTP client, persisted anonymous client
identifier, current normalized install method, and the cancellable periodic
task. The module also exposes pure payload and identifier helpers so the
privacy contract can be tested without a network.

The service uses the existing `reqwest`, `tokio`, `serde_json`, and `log`
dependencies. No new dependency or frontend panel is needed. The existing
`get_appdata_dir()` helper supplies the storage directory.

### Event flow

```text
UI ready
  -> notify_ui_interactive
  -> ActivePlayerService.start(method from persisted launcher state)
  -> POST { event: "open" }
  -> periodic POST every 5 minutes { event: "heartbeat" }

successful launch request
  -> ActivePlayerService.send_launch(normalized method)
  -> POST { event: "launch" }

launcher exit
  -> ActivePlayerService.stop()
  -> cancel periodic task
```

Startup and launch sends run asynchronously. There is no await on the
heartbeat path in the frontend event loop or in the game process spawn path.
Starting the service more than once is idempotent; a later method value updates
the method used by subsequent heartbeats without creating another timer.

### Install-method representation

Reuse `engine::method::InstallMethod` parsing and normalization. The payload
maps the current enum to the identifiers used by `main`: `ResourceMount` to
`method3`, `Loader` to `method2`, and `SignatureBypass` to `method1`. This keeps
the existing active-player endpoint compatible while the Tauri frontend/backend
continue using the typed enum internally. The persisted method is read from
the existing launcher settings at startup, with `resource_mount` as the safe
default when no method is stored.

### Failure handling and privacy

The endpoint is fixed in code for parity with `main`; no user-editable URL is
introduced. The service logs only event outcome and transport/status failures,
never the request body or local path. A request failure is not surfaced as a
toast, modal, or launcher operation error. A missing or invalid client-id file
is replaced with a newly generated opaque identifier before sending.

The active-player heartbeat is intentionally separate from the removed
diagnostics/log-upload feature. This change does not add log upload, a logs UI,
or game/account telemetry.

## Testing

Write tests before the implementation for the pure helpers and then implement
the minimum service needed to make them pass:

- payload serialization includes exactly the four allowed keys and preserves
  the event/method values;
- payload serialization excludes `game_path`, `windows_user`, and diagnostic
  fields;
- install-method normalization maps all three current enum variants to the
  `main` branch IDs (`method3`, `method2`, and `method1`);
- client-id persistence reuses a valid stored value and replaces empty or
  overlong content;
- starting the service is idempotent and stopping it cancels the periodic task
  without panicking;
- the existing Rust suite, frontend check, and release build remain passing.

Network integration tests are deliberately omitted from the default suite:
the production endpoint is external and should not make local or CI tests
flaky. Request timeout and error paths are covered by the service's
best-effort control flow and logs.

## Acceptance criteria

1. A normal launcher start produces at most one `open` request and then one
   heartbeat request per five-minute interval.
2. A successful game launch produces one `launch` request with the selected
   normalized method.
3. The request body contains no path, username, account, or log data.
4. Offline endpoint/network conditions do not show launcher errors and do not
   prevent game launching or shutdown.
5. Closing the launcher cancels the timer and does not leave a background
   heartbeat task running.
6. Existing local runtime detection and the previously removed diagnostics
   upload path are unchanged.

## Non-goals

- No player account/login tracking.
- No game-process telemetry beyond the local runtime state already present.
- No remote logs, crash uploads, or new server API.
- No frontend settings control for enabling/disabling the heartbeat.
