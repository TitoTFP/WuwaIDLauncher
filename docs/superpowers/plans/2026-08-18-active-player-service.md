# Active Player Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `main` branch's anonymous active-player heartbeat lifecycle to the Tauri launcher without restoring remote diagnostics upload or blocking game launch.

**Architecture:** Add a focused `engine::active_player` service managed by Tauri. It owns one persisted opaque client ID, a fixed production endpoint, a cancellable five-minute heartbeat task, and best-effort asynchronous POSTs. The frontend passes its already-normalized install method at the UI-ready milestone; the launch command sends the `launch` event after the game process is spawned.

**Tech Stack:** Rust, Tauri 2, Tokio, reqwest, serde_json, existing `sha2`/`hex` dependencies, Svelte TypeScript bridge.

---

### Task 1: Add failing payload and client-ID tests

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/active_player.rs`

- [ ] **Step 1: Register the empty module and write the tests first**

Add `pub mod active_player;` to `src-tauri/src/engine/mod.rs`. Create
`active_player.rs` with only the test module below; do not add the production
helpers yet.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::method::InstallMethod;

    #[test]
    fn heartbeat_payload_contains_exactly_the_allowed_fields() {
        let payload = build_heartbeat_payload("client-123", InstallMethod::Loader, "launch");
        let object = payload.as_object().unwrap();

        assert_eq!(object.len(), 4);
        assert_eq!(object["client_id"], "client-123");
        assert_eq!(object["launcher_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(object["install_method"], "method2");
        assert_eq!(object["event"], "launch");
        assert!(!object.contains_key("game_path"));
        assert!(!object.contains_key("windows_user"));
        assert!(!object.contains_key("logs"));
    }

    #[test]
    fn heartbeat_payload_uses_main_install_method_ids() {
        assert_eq!(build_heartbeat_payload("id", InstallMethod::ResourceMount, "open")["install_method"], "method3");
        assert_eq!(build_heartbeat_payload("id", InstallMethod::Loader, "open")["install_method"], "method2");
        assert_eq!(build_heartbeat_payload("id", InstallMethod::SignatureBypass, "open")["install_method"], "method1");
    }

    #[test]
    fn client_id_reuses_valid_content_and_replaces_invalid_content() {
        let appdata = tempfile::tempdir().unwrap();
        let path = appdata.path().join("active-client-id.txt");

        std::fs::write(&path, "existing-client\n").unwrap();
        assert_eq!(load_or_create_client_id(appdata.path()), "existing-client");

        std::fs::write(&path, "x".repeat(129)).unwrap();
        let replacement = load_or_create_client_id(appdata.path());
        assert_ne!(replacement, "x".repeat(129));
        assert!(!replacement.trim().is_empty());
        assert!(replacement.chars().count() <= 128);
        assert_eq!(std::fs::read_to_string(path).unwrap(), replacement);
    }
}
```

- [ ] **Step 2: Run the focused test to verify the expected RED failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine::active_player::tests -- --nocapture
```

Expected: compilation fails because `build_heartbeat_payload` and
`load_or_create_client_id` do not exist yet. This confirms the tests exercise
new behavior rather than existing code.

### Task 2: Implement the pure payload and client-ID helpers

**Files:**
- Modify: `src-tauri/src/engine/active_player.rs`

- [ ] **Step 1: Add the minimum production helpers**

Implement these exact contracts above the test module:

```rust
pub const ACTIVE_HEARTBEAT_ENDPOINT: &str =
    "https://logs.titotfp.my.id/api/active/heartbeat";

pub fn build_heartbeat_payload(
    client_id: &str,
    install_method: InstallMethod,
    event: &str,
) -> serde_json::Value {
    serde_json::json!({
        "client_id": client_id,
        "launcher_version": env!("CARGO_PKG_VERSION"),
        "install_method": active_player_method(install_method),
        "event": event,
    })
}

fn load_or_create_client_id(appdata_dir: &Path) -> String {
    let path = appdata_dir.join("active-client-id.txt");
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() && existing.chars().count() <= 128 {
            return existing.to_string();
        }
    }

    let id = generate_client_id();
    if let Err(error) = std::fs::create_dir_all(appdata_dir)
        .and_then(|_| std::fs::write(&path, &id))
    {
        log::warn!("Active player client ID tidak dapat disimpan: {error}");
    }
    id
}
```

Map `InstallMethod::ResourceMount` to `method3`, `InstallMethod::Loader` to
`method2`, and `InstallMethod::SignatureBypass` to `method1` before
serialization. Generate the opaque ID with a SHA-256 digest of the current time, process ID,
and a static atomic counter, using the already-direct `sha2` and `hex`
dependencies. Do not add a new crate solely for UUID generation.

- [ ] **Step 2: Run the focused tests to verify GREEN**

Run the same `cargo test` command from Task 1. Expected: all three active-player
helper tests pass.

### Task 3: Add a failing lifecycle test

**Files:**
- Modify: `src-tauri/src/engine/active_player.rs`

- [ ] **Step 1: Add the lifecycle test before the service implementation**

Add this test to the existing test module:

```rust
#[tokio::test]
async fn service_start_is_idempotent_and_stop_aborts_the_timer() {
    let appdata = tempfile::tempdir().unwrap();
    let service = ActivePlayerService::with_endpoint_for_test(
        appdata.path().to_path_buf(),
        "http://127.0.0.1:9/active",
    );

    assert!(service.start(InstallMethod::Loader));
    assert!(!service.start(InstallMethod::SignatureBypass));
    assert!(service.is_running_for_test());
    assert_eq!(service.method_for_test(), InstallMethod::SignatureBypass);

    service.stop();
    assert!(!service.is_running_for_test());
}
```

- [ ] **Step 2: Run the focused test to verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine::active_player::tests::service_start_is_idempotent_and_stop_aborts_the_timer -- --nocapture
```

Expected: compilation fails because `ActivePlayerService` and its lifecycle
methods do not exist yet.

### Task 4: Implement the cancellable best-effort service

**Files:**
- Modify: `src-tauri/src/engine/active_player.rs`

- [ ] **Step 1: Add the service state and constructors**

Use a cloneable `Arc` around a `reqwest::Client`, app-data path, endpoint, and
`std::sync::Mutex` lifecycle state. The lifecycle state stores the current
`InstallMethod` and one `tauri::async_runtime::JoinHandle<()>`.

Production construction must use the fixed `ACTIVE_HEARTBEAT_ENDPOINT`; the
test-only constructor may replace the endpoint with the loopback URL used by
Task 3. The client ID is loaded once by `load_or_create_client_id`.

- [ ] **Step 2: Implement `start` and `stop` with one timer**

`start(method)` updates the method and returns `true` only when it creates a
new task. The task sends `open` immediately, consumes Tokio's immediate first
interval tick, then sends `heartbeat` every five minutes. `stop()` takes and
aborts the stored handle. Use `tauri::async_runtime::spawn`, not a new runtime.

```rust
pub fn start(&self, method: InstallMethod) -> bool {
    let mut lifecycle = self.inner.lifecycle.lock().unwrap();
    lifecycle.method = method;
    if lifecycle.task.is_some() {
        return false;
    }

    let service = self.clone();
    lifecycle.task = Some(tauri::async_runtime::spawn(async move {
        service.send_event("open", method).await;
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.tick().await;
        loop {
            interval.tick().await;
            service.send_event("heartbeat", service.current_method()).await;
        }
    }));
    true
}

pub fn stop(&self) {
    if let Some(task) = self.inner.lifecycle.lock().unwrap().task.take() {
        task.abort();
    }
}
```

The test-only inspection helpers return the task-present flag and current
method without changing production behavior.

- [ ] **Step 3: Implement asynchronous event delivery**

`send_launch(method)` calls `start(method)` to handle direct command use, then
spawns one `launch` request. `send_event` builds the four-field payload, uses
the existing reqwest client with an eight-second per-request timeout, treats
only 2xx responses as success, and logs status/transport failures without
returning an error or emitting a UI event. Never log the JSON body or a game
path.

- [ ] **Step 4: Run the focused lifecycle test and the full Rust unit suite**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine::active_player::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: active-player tests and the existing Rust library tests pass with no
new warnings caused by the service.

### Task 5: Wire startup, launch, and shutdown into Tauri

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Register the managed service**

Add `.manage(engine::active_player::ActivePlayerService::new(get_appdata_dir()))`
next to `RuntimeCoordinator::default()` in `run()`.

- [ ] **Step 2: Start the service at the existing UI-ready command**

Change `notify_ui_interactive` to accept `install_method: String`, parse it
with `InstallMethod::parse`, fall back to `ResourceMount` after a local warning,
and call `service.start(method)` through `app.try_state`. The command remains
best-effort and returns `()` so a heartbeat setup problem cannot fail UI
initialization.

- [ ] **Step 3: Send the launch event after process spawn**

In the `Ok(mut process)` arm of `launch_game`, after
`engine::runtime::launch_game(&p, dx11)` succeeds, call
`service.send_launch(method)` before waiting for process detection. This keeps
the event tied to a real spawn while not delaying UAC/game startup.

- [ ] **Step 4: Stop the service on process exit**

In the existing `tauri::RunEvent::Exit` handler, call `stop()` on the managed
service before `restore_tracked_signature(app)`. Leave local runtime monitoring
and signature restoration unchanged.

- [ ] **Step 5: Run the Rust suite after wiring**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: exit code 0 for both commands.

### Task 6: Pass the normalized install method from the frontend

**Files:**
- Modify: `src/lib/bridge.ts`
- Modify: `src/lib/launcherState.svelte.ts`

- [ ] **Step 1: Extend the bridge signature**

Change the bridge method to:

```ts
notifyUiInteractive: (installMethod: InstallMethod): Promise<void> =>
  invoke("notify_ui_interactive", { installMethod }),
```

- [ ] **Step 2: Pass the loaded normalized method during initialization**

Change the existing call in `LauncherState.init()` to:

```ts
await bridge.notifyUiInteractive(this.config.installMethod);
```

No visible UI, setting, opt-out, or heartbeat error state is added.

- [ ] **Step 3: Run frontend checks**

Run:

```powershell
npm run check
npm run build
```

Expected: both commands exit 0.

### Task 7: Align the operational documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-launcher-operations-design.md`

- [ ] **Step 1: Clarify diagnostics versus active presence**

Update the diagnostics goal and non-goal wording so it says remote diagnostics
and log upload remain removed, while the explicitly requested anonymous
active-player heartbeat is the only presence request. Do not claim the branch
has no outbound traffic at all.

- [ ] **Step 2: Verify documentation consistency**

Run:

```powershell
rg -n "no outbound|no .*telemetry|active-player|heartbeat|diagnostics" docs/superpowers/specs/2026-08-18-launcher-operations-design.md docs/superpowers/specs/2026-08-18-active-player-service-design.md
git diff --check
```

Expected: the documents distinguish anonymous presence from diagnostics and
`git diff --check` reports no whitespace errors.

### Task 8: Final verification and handoff

**Files:**
- Verify all modified files above; do not commit or push unless separately requested.

- [ ] **Step 1: Run the complete project verification**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check
npm run build
```

Expected: all four commands exit 0.

- [ ] **Step 2: Review the diff against the spec**

Confirm the diff has one service, one timer, exactly four payload fields, no
game path/user/log data, no frontend error surface, and shutdown cancellation.
Report any failed command or remaining manual limitation instead of claiming
completion.
