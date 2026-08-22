use super::method::InstallMethod;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

pub const ACTIVE_HEARTBEAT_ENDPOINT: &str = "https://logs.titotfp.my.id/api/active/heartbeat";

static CLIENT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn active_player_method(install_method: InstallMethod) -> &'static str {
    match install_method {
        InstallMethod::ResourceMount => "method3",
        InstallMethod::Loader => "method2",
    }
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
    if let Err(error) =
        std::fs::create_dir_all(appdata_dir).and_then(|_| std::fs::write(&path, &id))
    {
        log::warn!("Active player client ID tidak dapat disimpan: {error}");
    }
    id
}

fn generate_client_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = CLIENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = format!("{timestamp}:{}:{counter}", std::process::id());
    hex::encode(Sha256::digest(seed.as_bytes()))
}

#[derive(Clone)]
pub struct ActivePlayerService {
    inner: Arc<ActivePlayerInner>,
}

struct ActivePlayerInner {
    client: Client,
    client_id: String,
    endpoint: String,
    lifecycle: Mutex<ActivePlayerLifecycle>,
}

struct ActivePlayerLifecycle {
    method: InstallMethod,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl ActivePlayerService {
    pub fn new(appdata_dir: PathBuf) -> Self {
        Self::with_endpoint(appdata_dir, ACTIVE_HEARTBEAT_ENDPOINT.to_string())
    }

    fn with_endpoint(appdata_dir: PathBuf, endpoint: String) -> Self {
        Self {
            inner: Arc::new(ActivePlayerInner {
                client: Client::new(),
                client_id: load_or_create_client_id(&appdata_dir),
                endpoint,
                lifecycle: Mutex::new(ActivePlayerLifecycle {
                    method: InstallMethod::ResourceMount,
                    task: None,
                }),
            }),
        }
    }

    pub fn start(&self, method: InstallMethod) -> bool {
        let mut lifecycle = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
                service
                    .send_event("heartbeat", service.current_method())
                    .await;
            }
        }));
        true
    }

    pub fn send_launch(&self, method: InstallMethod) {
        self.start(method);
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            service.send_event("launch", method).await;
        });
    }

    pub fn stop(&self) {
        let task = self
            .inner
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .task
            .take();
        if let Some(task) = task {
            task.abort();
        }
    }

    async fn send_event(&self, event: &'static str, method: InstallMethod) {
        let payload = build_heartbeat_payload(&self.inner.client_id, method, event);
        match self
            .inner
            .client
            .post(&self.inner.endpoint)
            .timeout(Duration::from_secs(8))
            .json(&payload)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                log::debug!("Active player heartbeat sent: event={event}");
            }
            Ok(response) => {
                log::debug!(
                    "Active player heartbeat rejected: event={event}, status={}",
                    response.status()
                );
            }
            Err(error) => {
                log::debug!("Active player heartbeat failed: event={event}, error={error}");
            }
        }
    }

    fn current_method(&self) -> InstallMethod {
        self.inner
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .method
    }

    #[cfg(test)]
    fn with_endpoint_for_test(appdata_dir: PathBuf, endpoint: &str) -> Self {
        Self::with_endpoint(appdata_dir, endpoint.to_string())
    }

    #[cfg(test)]
    fn is_running_for_test(&self) -> bool {
        self.inner
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .task
            .is_some()
    }

    #[cfg(test)]
    fn method_for_test(&self) -> InstallMethod {
        self.current_method()
    }
}

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
        assert_eq!(
            build_heartbeat_payload("id", InstallMethod::ResourceMount, "open")["install_method"],
            "method3"
        );
        assert_eq!(
            build_heartbeat_payload("id", InstallMethod::Loader, "open")["install_method"],
            "method2"
        );
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

    #[tokio::test]
    async fn service_start_is_idempotent_and_stop_aborts_the_timer() {
        let appdata = tempfile::tempdir().unwrap();
        let service = ActivePlayerService::with_endpoint_for_test(
            appdata.path().to_path_buf(),
            "http://127.0.0.1:9/active",
        );

        assert!(service.start(InstallMethod::Loader));
        assert!(!service.start(InstallMethod::ResourceMount));
        assert!(service.is_running_for_test());
        assert_eq!(service.method_for_test(), InstallMethod::ResourceMount);

        service.stop();
        assert!(!service.is_running_for_test());
    }
}
