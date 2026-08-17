use std::fs;
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_HEARTBEAT_ENDPOINT: &str = "https://wuwa-active.titofp.workers.dev";

pub fn get_or_create_client_id(appdata_dir: &Path) -> String {
    let client_id_path = appdata_dir.join("active-client-id.txt");
    if let Ok(existing) = fs::read_to_string(&client_id_path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() && trimmed.len() <= 128 {
            return trimmed.to_string();
        }
    }

    let new_id = hex::encode(md5_hash(&uuid_v4_like()));
    let _ = fs::create_dir_all(appdata_dir);
    let _ = fs::write(&client_id_path, &new_id);
    new_id
}

fn uuid_v4_like() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    format!("{}-{:x}", pid, now)
}

fn md5_hash(data: &str) -> [u8; 16] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let res = hasher.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&res[..16]);
    out
}

pub fn build_heartbeat_payload(
    client_id: &str,
    launcher_version: &str,
    install_method: &str,
    event: &str,
) -> serde_json::Value {
    serde_json::json!({
        "client_id": client_id,
        "launcher_version": launcher_version,
        "install_method": install_method,
        "event": if event.is_empty() { "heartbeat" } else { event }
    })
}

pub async fn send_heartbeat(
    client_id: &str,
    launcher_version: &str,
    install_method: &str,
    event: &str,
) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let payload = build_heartbeat_payload(client_id, launcher_version, install_method, event);

    let response = client
        .post(DEFAULT_HEARTBEAT_ENDPOINT)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to send heartbeat: {}", e))?;

    Ok(response.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_client_id_persistence() {
        let tmp = tempdir().unwrap();
        let id1 = get_or_create_client_id(tmp.path());
        assert!(!id1.is_empty());

        let id2 = get_or_create_client_id(tmp.path());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_build_heartbeat_payload() {
        let payload = build_heartbeat_payload("test-client", "2.6.1", "method3", "launch");
        assert_eq!(payload["client_id"], "test-client");
        assert_eq!(payload["launcher_version"], "2.6.1");
        assert_eq!(payload["install_method"], "method3");
        assert_eq!(payload["event"], "launch");
    }
}
