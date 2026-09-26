//! Signature-verified remote theming.
//!
//! The launcher ships a general theme. A remote theme is a plain data payload —
//! CSS custom properties plus an optional stylesheet fragment — declared in the
//! asset manifest. It is honoured only when the manifest carries a valid
//! Ed25519 signature from a key compiled into this binary; every failure path
//! (missing signature, unknown key, bad signature, oversized payload, hash
//! mismatch, network failure) leaves the bundled general theme untouched.
//!
//! Trust rotation: `TRUSTED_SIGNING_KEYS` is a keyring, not a single key, so a
//! rotation can be staged in the standby slot ahead of time and then performed
//! without shipping a launcher build.

use crate::engine::downloader::{
    download_file, read_response_body_limited, replace_file_atomically, verify_sha256,
};
use crate::engine::media::{validate_asset_url, AssetEntry};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SIGNATURE_HEADER: &str = "wuwaid-manifest-v1";
pub const SIGNATURE_FILE_SUFFIX: &str = ".sig";
pub const MAX_SIGNATURE_BYTES: u64 = 4096;

pub const GENERAL_THEME_ID: &str = "general";
pub const GENERAL_THEME_NAME: &str = "Tema Umum";
pub const THEME_CSS_FILE: &str = "theme.css";
pub const THEME_BACKGROUND_FILE: &str = "bg.jpg";
const THEME_CACHE_FILE: &str = "theme-cache.json";

pub const MAX_THEME_TOKENS: usize = 240;
pub const MAX_TOKEN_VALUE_LENGTH: usize = 160;
pub const MAX_THEME_ID_LENGTH: usize = 48;
pub const MAX_THEME_NAME_LENGTH: usize = 64;
pub const MAX_THEME_CSS_BYTES: usize = 128 * 1024;

const TOKEN_NAME_MAX_LENGTH: usize = 48;
const FORBIDDEN_IN_TOKEN_VALUE: &[char] = &[';', '{', '}', '<', '>', '\\'];
const FORBIDDEN_IN_CSS: &[&str] = &["@import", "expression(", "javascript:", "</style"];

/// Keys allowed to sign the asset manifest.
///
/// The second entry is the rotation slot: stage the incoming public key there
/// and sign with it, and the rotation needs no launcher release. Removing a key
/// does require a release, so retire keys by demoting them to the standby slot
/// rather than deleting them immediately.
pub const TRUSTED_SIGNING_KEYS: &[(&str, &str)] = &[
    (
        "wuwa-web-2026-01",
        "e4bf5c507ba36d2e5b51540318ec2743bb24b86c85ede7ce503bdcb2ee1cbd40",
    ),
    // ("<key-id-next>", "<64 hex chars>"),
];

/// The theme block as authored in `assets.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    /// The manifest author flips this when a game version ships a new look.
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
    #[serde(default)]
    pub theme_css: Option<AssetEntry>,
    #[serde(default)]
    pub background: Option<AssetEntry>,
}

/// The verified theme as cached on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedTheme {
    pub id: String,
    pub name: String,
    pub key_id: String,
    pub tokens: BTreeMap<String, String>,
    pub has_background: bool,
}

/// Everything the webview needs to paint a theme, or nothing at all when the
/// bundled general theme should stay active.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemePayload {
    pub id: String,
    pub name: String,
    pub key_id: String,
    pub tokens: BTreeMap<String, String>,
    pub css: String,
    pub background_file: String,
    pub status: String,
}

pub fn signature_url(manifest_url: &str) -> String {
    format!("{manifest_url}{SIGNATURE_FILE_SUFFIX}")
}

pub fn general_payload(status: &str) -> ThemePayload {
    ThemePayload {
        id: GENERAL_THEME_ID.to_string(),
        name: GENERAL_THEME_NAME.to_string(),
        key_id: String::new(),
        tokens: BTreeMap::new(),
        css: String::new(),
        background_file: String::new(),
        status: status.to_string(),
    }
}

pub async fn fetch_signature(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "WuwaIDLauncher-Tauri")
        .send()
        .await
        .map_err(|error| format!("Gagal mengambil tanda tangan manifest: {error}"))?;
    if !resp.status().is_success() {
        return Err(format!("Server response error: {}", resp.status()));
    }
    if resp
        .content_length()
        .is_some_and(|length| length > MAX_SIGNATURE_BYTES)
    {
        return Err("Tanda tangan manifest terlalu besar.".to_string());
    }
    let body = read_response_body_limited(resp, MAX_SIGNATURE_BYTES)
        .await
        .map_err(|error| format!("Gagal membaca tanda tangan manifest: {error}"))?;
    String::from_utf8(body.to_vec())
        .map_err(|_| "Tanda tangan manifest bukan UTF-8 valid.".to_string())
}

/// Verifies the detached signature over the exact manifest bytes that were
/// fetched. Returns the key id that signed the manifest.
pub fn verify_manifest_signature(
    manifest_bytes: &[u8],
    signature_text: &str,
) -> Result<String, String> {
    verify_with_keyring(manifest_bytes, signature_text, TRUSTED_SIGNING_KEYS)
}

/// Verification against an explicit keyring. Split out so tests can prove both
/// acceptance and rejection without mutating process-wide state.
fn verify_with_keyring(
    manifest_bytes: &[u8],
    signature_text: &str,
    keyring: &[(&str, &str)],
) -> Result<String, String> {
    let mut fields = signature_text.split_whitespace();
    let header = fields.next().unwrap_or_default();
    let key_id = fields.next().unwrap_or_default();
    let signature_hex = fields.next().unwrap_or_default();
    if header != SIGNATURE_HEADER || key_id.is_empty() || signature_hex.is_empty() {
        return Err("Format tanda tangan manifest tidak dikenal.".to_string());
    }
    if fields.next().is_some() {
        return Err("Tanda tangan manifest memiliki field berlebih.".to_string());
    }

    let public_key_hex = keyring
        .iter()
        .find(|(id, _)| *id == key_id)
        .map(|(_, public_key_hex)| *public_key_hex)
        .ok_or_else(|| format!("Key penanda tangan tidak dikenal: {key_id}"))?;

    let key_bytes =
        hex::decode(public_key_hex).map_err(|_| format!("Public key {key_id} tidak valid."))?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| format!("Public key {key_id} bukan 32 byte."))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|_| format!("Public key {key_id} ditolak."))?;
    let signature_bytes = hex::decode(signature_hex)
        .map_err(|_| "Signature manifest bukan hex valid.".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "Signature manifest tidak memiliki panjang yang valid.".to_string())?;

    verifying_key
        .verify(manifest_bytes, &signature)
        .map_err(|_| "Tanda tangan manifest tidak valid.".to_string())?;
    Ok(key_id.to_string())
}

pub fn validate_theme_definition(theme: &ThemeDefinition) -> Result<(), String> {
    if theme.id.is_empty() || theme.id.len() > MAX_THEME_ID_LENGTH {
        return Err("ID tema tidak valid.".to_string());
    }
    if !theme
        .id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("ID tema hanya boleh berisi huruf kecil, angka, dan strip.".to_string());
    }
    if theme.name.trim().is_empty() || theme.name.chars().count() > MAX_THEME_NAME_LENGTH {
        return Err("Nama tema tidak valid.".to_string());
    }
    if theme.name.chars().any(|c| c.is_control()) {
        return Err("Nama tema mengandung karakter kontrol.".to_string());
    }
    if theme.tokens.len() > MAX_THEME_TOKENS {
        return Err(format!(
            "Tema melebihi batas {MAX_THEME_TOKENS} token warna."
        ));
    }
    for (name, value) in &theme.tokens {
        if !is_valid_token_name(name) {
            return Err(format!("Nama token tema tidak valid: {name}"));
        }
        if value.is_empty() || value.len() > MAX_TOKEN_VALUE_LENGTH {
            return Err(format!("Nilai token tema tidak valid: {name}"));
        }
        if value.contains(FORBIDDEN_IN_TOKEN_VALUE) {
            return Err(format!(
                "Nilai token tema memuat karakter terlarang: {name}"
            ));
        }
        let lowered = value.to_ascii_lowercase();
        if lowered.contains("@import")
            || lowered.contains("expression(")
            || lowered.contains("javascript:")
            || lowered.contains("url(")
        {
            return Err(format!(
                "Nilai token tema memuat css yang tidak diizinkan: {name}"
            ));
        }
    }
    if let Some(entry) = &theme.theme_css {
        validate_theme_asset(entry, THEME_CSS_FILE)?;
    }
    if let Some(entry) = &theme.background {
        validate_theme_asset(entry, THEME_BACKGROUND_FILE)?;
    }
    Ok(())
}

fn is_valid_token_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("--") else {
        return false;
    };
    !rest.is_empty()
        && rest.len() <= TOKEN_NAME_MAX_LENGTH
        && rest.starts_with(|c: char| c.is_ascii_lowercase())
        && rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn validate_theme_asset(entry: &AssetEntry, expected_name: &str) -> Result<(), String> {
    if entry.name != expected_name {
        return Err(format!(
            "Aset tema harus bernama {expected_name}, bukan {}.",
            entry.name
        ));
    }
    if entry.sha256.trim().is_empty() {
        return Err(format!("SHA-256 wajib dicantumkan untuk {expected_name}."));
    }
    validate_asset_url(entry, (expected_name, expected_name))
}

fn validate_fragment(css: &str) -> Result<(), String> {
    if css.len() > MAX_THEME_CSS_BYTES {
        return Err("Fragmen CSS tema melebihi batas ukuran.".to_string());
    }
    let lowered = css.to_ascii_lowercase();
    if let Some(forbidden) = FORBIDDEN_IN_CSS
        .iter()
        .find(|needle| lowered.contains(**needle))
    {
        return Err(format!("Fragmen CSS tema memuat {forbidden}."));
    }
    Ok(())
}

pub fn theme_cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(THEME_CACHE_FILE)
}

pub fn read_cached_theme(cache_dir: &Path) -> Result<Option<CachedTheme>, String> {
    let path = theme_cache_path(cache_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .map_err(|error| format!("Gagal membaca cache tema: {error}"))?;
    let cached: CachedTheme =
        serde_json::from_str(&data).map_err(|error| format!("Cache tema tidak valid: {error}"))?;
    // A fragment that went missing must not resurrect a partial theme.
    if cached_theme_css(cache_dir)?.is_none() {
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }
    if cached.has_background && !cache_dir.join(THEME_BACKGROUND_FILE).is_file() {
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }
    Ok(Some(cached))
}

fn cached_theme_css(cache_dir: &Path) -> Result<Option<String>, String> {
    let path = cache_dir.join(THEME_CSS_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let css = std::fs::read_to_string(&path)
        .map_err(|error| format!("Gagal membaca fragmen CSS tema: {error}"))?;
    if css.trim().is_empty() {
        return Ok(None);
    }
    validate_fragment(&css)?;
    Ok(Some(css))
}

fn write_cached_theme(cache_dir: &Path, cached: &CachedTheme) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|error| format!("Gagal membuat folder cache tema: {error}"))?;
    let path = theme_cache_path(cache_dir);
    let temp = path.with_extension("tmp");
    let data = serde_json::to_vec(cached)
        .map_err(|error| format!("Gagal menyusun cache tema: {error}"))?;
    std::fs::write(&temp, data).map_err(|error| format!("Gagal menulis cache tema: {error}"))?;
    replace_file_atomically(&temp, &path)
        .map_err(|error| format!("Gagal mengaktifkan cache tema: {error}"))
}

/// Downloads and verifies the theme payload, then activates it atomically.
/// Only called for a manifest whose signature has already been verified.
pub async fn sync_theme(cache_dir: &Path, theme: &ThemeDefinition) -> Result<(), String> {
    validate_theme_definition(theme)?;
    std::fs::create_dir_all(cache_dir)
        .map_err(|error| format!("Gagal membuat folder cache tema: {error}"))?;

    match &theme.theme_css {
        Some(entry) => {
            let css = fetch_theme_asset(cache_dir, entry, THEME_CSS_FILE).await?;
            let text = std::fs::read_to_string(&css)
                .map_err(|error| format!("Gagal membaca fragmen CSS tema: {error}"))?;
            validate_fragment(&text)?;
        }
        // A theme without a fragment must not inherit the previous theme's one.
        None => remove_theme_file(cache_dir, THEME_CSS_FILE)?,
    }

    match &theme.background {
        Some(entry) => {
            fetch_theme_asset(cache_dir, entry, THEME_BACKGROUND_FILE).await?;
        }
        None => remove_theme_file(cache_dir, THEME_BACKGROUND_FILE)?,
    }

    write_cached_theme(
        cache_dir,
        &CachedTheme {
            id: theme.id.clone(),
            name: theme.name.clone(),
            key_id: String::new(),
            tokens: theme.tokens.clone(),
            has_background: theme.background.is_some(),
        },
    )
}

async fn fetch_theme_asset(
    cache_dir: &Path,
    entry: &AssetEntry,
    destination_name: &str,
) -> Result<PathBuf, String> {
    let destination = cache_dir.join(destination_name);
    if destination.is_file() && verify_sha256(&destination, &entry.sha256).unwrap_or(false) {
        return Ok(destination);
    }
    let candidate = cache_dir.join(format!(".{destination_name}.candidate"));
    let _ = std::fs::remove_file(&candidate);
    download_file(&entry.url, &candidate, |_| {}).await?;
    if !verify_sha256(&candidate, &entry.sha256).unwrap_or(false) {
        let _ = std::fs::remove_file(&candidate);
        return Err(format!(
            "Integritas hash SHA-256 untuk {destination_name} tidak valid. File dibersihkan."
        ));
    }
    if destination.exists() && !destination.is_file() {
        return Err(format!("Target tema bukan file: {destination:?}"));
    }
    replace_file_atomically(&candidate, &destination)
        .map_err(|error| format!("Gagal mengaktifkan {destination_name}: {error}"))?;
    Ok(destination)
}

fn remove_theme_file(cache_dir: &Path, name: &str) -> Result<(), String> {
    let path = cache_dir.join(name);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("Gagal menghapus {name} lama: {error}"))?;
    }
    Ok(())
}

/// Builds the payload the webview paints. `key_id` is filled in by the caller
/// because only it knows which key verified the manifest.
pub fn build_payload(cache_dir: &Path, key_id: &str) -> Result<ThemePayload, String> {
    let Some(mut cached) = read_cached_theme(cache_dir)? else {
        return Ok(general_payload("general"));
    };
    cached.key_id = key_id.to_string();
    let background_file = if cached.has_background {
        THEME_BACKGROUND_FILE.to_string()
    } else {
        String::new()
    };
    Ok(ThemePayload {
        id: cached.id,
        name: cached.name,
        key_id: cached.key_id,
        tokens: cached.tokens,
        css: cached_theme_css(cache_dir)?.unwrap_or_default(),
        background_file,
        status: "signed".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const TEST_KEY_ID: &str = "test-key";

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(bytes: &[u8]) -> String {
        let signature = test_signing_key().sign(bytes);
        format!(
            "{SIGNATURE_HEADER} {TEST_KEY_ID} {}",
            hex::encode(signature.to_bytes())
        )
    }

    /// Public halves for the two directions verification must be proven in.
    /// The caller keeps the `String` alive while the borrowed slice is used.
    fn trusted_public_hex() -> String {
        hex::encode(test_signing_key().verifying_key().to_bytes())
    }

    fn untrusted_public_hex() -> String {
        hex::encode(
            SigningKey::from_bytes(&[9u8; 32])
                .verifying_key()
                .to_bytes(),
        )
    }

    fn definition() -> ThemeDefinition {
        ThemeDefinition {
            id: "wuwa-2-4".to_string(),
            name: "Wuthering Waves 2.4".to_string(),
            active: true,
            tokens: BTreeMap::from([("--gold-rgb".to_string(), "255 0 0".to_string())]),
            theme_css: None,
            background: None,
        }
    }

    #[test]
    fn a_trusted_signature_verifies() {
        let public_hex = trusted_public_hex();
        let keyring = [(TEST_KEY_ID, public_hex.as_str())];
        let payload = b"{\"assets\":[]}";
        assert_eq!(
            verify_with_keyring(payload, &sign(payload), &keyring).unwrap(),
            TEST_KEY_ID
        );
    }

    #[test]
    fn signature_from_an_untrusted_key_is_rejected() {
        let public_hex = untrusted_public_hex();
        let keyring = [("someone-else", public_hex.as_str())];
        let payload = b"{\"assets\":[]}";
        let error = verify_with_keyring(payload, &sign(payload), &keyring).unwrap_err();
        assert!(error.contains("tidak dikenal"), "unexpected: {error}");
    }

    #[test]
    fn tampered_manifest_is_rejected() {
        let public_hex = trusted_public_hex();
        let keyring = [(TEST_KEY_ID, public_hex.as_str())];
        let signature = sign(b"{\"assets\":[]}");
        let error = verify_with_keyring(b"{\"assets\":[1]}", &signature, &keyring).unwrap_err();
        assert!(error.contains("tidak valid"), "unexpected: {error}");
    }

    #[test]
    fn unsigned_manifest_is_rejected() {
        let public_hex = trusted_public_hex();
        let keyring = [(TEST_KEY_ID, public_hex.as_str())];
        let error = verify_with_keyring(b"{\"assets\":[]}", "", &keyring).unwrap_err();
        assert!(error.contains("tidak dikenal"), "unexpected: {error}");
    }

    #[test]
    fn signature_rejects_unknown_format() {
        for bad in [
            "",
            "garbage",
            "wuwaid-manifest-v2 k aa",
            "wuwaid-manifest-v1 k",
        ] {
            assert!(
                verify_manifest_signature(b"x", bad).is_err(),
                "accepted {bad}"
            );
        }
    }

    #[test]
    fn theme_definition_validation_rejects_dangerous_tokens() {
        let mut theme = definition();
        theme
            .tokens
            .insert("--gold-rgb".into(), "255 0 0; } body {".into());
        assert!(validate_theme_definition(&theme).is_err());

        let mut theme = definition();
        theme.tokens.insert("gold-rgb".into(), "255 0 0".into());
        assert!(validate_theme_definition(&theme).is_err());

        let mut theme = definition();
        theme.tokens.insert(
            "--gold-rgb".into(),
            "url(https://evil.example/x.png)".into(),
        );
        assert!(validate_theme_definition(&theme).is_err());
    }

    #[test]
    fn theme_definition_validation_accepts_a_palette() {
        let mut theme = definition();
        theme.tokens.insert(
            "--mist-grad".into(),
            "linear-gradient(135deg, #fff 0%, #000 100%)".into(),
        );
        assert!(validate_theme_definition(&theme).is_ok());
    }

    #[test]
    fn theme_asset_url_must_be_official() {
        let mut theme = definition();
        theme.theme_css = Some(AssetEntry {
            name: THEME_CSS_FILE.to_string(),
            url: "https://evil.example/theme.css".to_string(),
            sha256: "a".repeat(64),
        });
        assert!(validate_theme_definition(&theme).is_err());
    }

    #[test]
    fn cached_theme_requires_a_present_fragment() {
        let temp = tempfile::tempdir().unwrap();
        let cached = CachedTheme {
            id: "wuwa-2-4".to_string(),
            name: "Wuthering Waves 2.4".to_string(),
            key_id: TEST_KEY_ID.to_string(),
            tokens: BTreeMap::new(),
            has_background: false,
        };
        write_cached_theme(temp.path(), &cached).unwrap();
        // Metadata without the fragment is not a usable theme, and the stale
        // metadata is dropped so the next sync re-downloads the pair.
        assert!(read_cached_theme(temp.path()).unwrap().is_none());
        assert!(!theme_cache_path(temp.path()).exists());

        // A complete cache activates again.
        std::fs::write(temp.path().join(THEME_CSS_FILE), "body{color:red}").unwrap();
        write_cached_theme(temp.path(), &cached).unwrap();
        assert_eq!(
            read_cached_theme(temp.path()).unwrap().unwrap().id,
            "wuwa-2-4"
        );
    }

    #[test]
    fn cached_theme_without_its_background_is_dropped() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), "body{color:red}").unwrap();
        write_cached_theme(
            temp.path(),
            &CachedTheme {
                id: "wuwa-2-4".to_string(),
                name: "Wuthering Waves 2.4".to_string(),
                key_id: TEST_KEY_ID.to_string(),
                tokens: BTreeMap::new(),
                has_background: true,
            },
        )
        .unwrap();
        assert!(read_cached_theme(temp.path()).unwrap().is_none());
    }

    #[test]
    fn empty_fragment_is_treated_as_no_theme() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), "   \n").unwrap();
        assert!(cached_theme_css(temp.path()).unwrap().is_none());
    }

    #[test]
    fn build_payload_falls_back_to_the_general_theme() {
        let temp = tempfile::tempdir().unwrap();
        let payload = build_payload(temp.path(), "any-key").unwrap();
        assert_eq!(payload.id, GENERAL_THEME_ID);
        assert!(payload.css.is_empty());
        assert!(payload.background_file.is_empty());
    }

    #[test]
    fn build_payload_exposes_cached_tokens_and_fragment() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), "body{color:red}").unwrap();
        std::fs::write(temp.path().join(THEME_BACKGROUND_FILE), b"jpeg").unwrap();
        write_cached_theme(
            temp.path(),
            &CachedTheme {
                id: "wuwa-2-4".to_string(),
                name: "Wuthering Waves 2.4".to_string(),
                key_id: String::new(),
                tokens: BTreeMap::from([("--gold-rgb".to_string(), "1 2 3".to_string())]),
                has_background: true,
            },
        )
        .unwrap();
        let payload = build_payload(temp.path(), TEST_KEY_ID).unwrap();
        assert_eq!(payload.id, "wuwa-2-4");
        assert_eq!(payload.key_id, TEST_KEY_ID);
        assert_eq!(payload.css, "body{color:red}");
        assert_eq!(payload.background_file, THEME_BACKGROUND_FILE);
        assert_eq!(payload.tokens.get("--gold-rgb").unwrap(), "1 2 3");
    }

    #[test]
    fn fragment_validation_blocks_imports() {
        assert!(validate_fragment("@import url(https://evil.example/x.css);").is_err());
        assert!(validate_fragment("body{color:red}").is_ok());
    }

    #[test]
    fn signature_url_sits_next_to_the_manifest() {
        assert_eq!(
            signature_url("https://example.test/Web/assets.json"),
            "https://example.test/Web/assets.json.sig"
        );
    }
}
