//! Signature-verified remote theming.
//!
//! The launcher ships a general theme. A remote theme is a plain data payload —
//! CSS custom properties plus an optional stylesheet fragment — declared in the
//! asset manifest. It is honoured only when the manifest carries a valid
//! Ed25519 signature from a key compiled into this binary. A failure never
//! activates anything: the last theme this launcher verified stays in place and
//! is reported as such, and the bundled general theme is what ships when
//! nothing verified is cached.
//!
//! Trust rotation: `TRUSTED_SIGNING_KEYS` is a keyring, not a single key, so a
//! rotation can be staged in the standby slot ahead of time and then performed
//! without shipping a launcher build.

use crate::engine::downloader::{
    download_file_with_expected_size_limited_policy, read_response_body_limited,
    replace_file_atomically, verify_sha256, DownloadRedirectPolicy,
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
/// Ceiling for a theme image, well under the downloader's global limit.
const MAX_THEME_ASSET_BYTES: u64 = 32 * 1024 * 1024;

const TOKEN_NAME_MAX_LENGTH: usize = 48;
/// Rejection set for a token value. Must stay identical to
/// `themeRuntime.svelte.ts` and `public/theme-boot.js`: three layers that
/// disagree are three different answers to "is this theme allowed?".
const FORBIDDEN_IN_TOKEN_VALUE: &[&str] = &[
    ";",
    "{",
    "}",
    "<",
    ">",
    "\\",
    "/*",
    "url(",
    "@import",
    "expression(",
    "javascript:",
];
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
#[serde(rename_all = "camelCase")]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    /// Whether this theme is the published one. Tri-state on purpose: an
    /// omitted `active` is a draft the author forgot to publish, not a
    /// withdrawal, and must not wipe every launcher's theme.
    #[serde(default)]
    pub active: Option<bool>,
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
    /// Digests of `theme.css` and, when present, of `bg.jpg` at cache time.
    /// A theme ships one asset or the other, never a separate "has background"
    /// flag: two fields that must agree, with nothing checking that they do.
    /// `None` means the theme ships that file not at all, and the cache is
    /// rejected if the file turns up anyway.
    pub css_sha256: Option<String>,
    pub background_sha256: Option<String>,
}

/// What a signed manifest asks the launcher to do about theming.
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeAction {
    /// Download and activate this theme.
    Activate,
    /// The author withdrew the theme. This is the only kill switch a shipped
    /// look has, so it removes the cache rather than merely hiding it.
    Withdraw,
    /// The manifest did not say which theme is published — no theme block at
    /// all, or a block that never set `active`. Absence is not a withdrawal,
    /// so the last verified theme stays: treating silence as consent to delete
    /// would let one careless manifest wipe every launcher's look, and the
    /// cost of being wrong is a theme that fails to appear rather than one
    /// that silently disappears for everyone.
    Keep,
}

/// Decides what a signed manifest means for the active theme.
pub fn resolve_theme_action(theme: Option<&ThemeDefinition>) -> ThemeAction {
    match theme.and_then(|theme| theme.active) {
        Some(true) => ThemeAction::Activate,
        Some(false) => ThemeAction::Withdraw,
        // No block, or a block that omitted `active`.
        None => ThemeAction::Keep,
    }
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
        let lowered = value.to_ascii_lowercase();
        if let Some(forbidden) = FORBIDDEN_IN_TOKEN_VALUE
            .iter()
            .find(|needle| lowered.contains(**needle))
        {
            return Err(format!(
                "Nilai token tema memuat css yang tidak diizinkan ({forbidden}): {name}"
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

/// Rejects a cache that is incomplete, torn, or signed by a key we no longer
/// trust. The keyring is a parameter rather than a direct read of the
/// production const, so tests can prove both accept and reject without ever
/// shipping a key inside `TRUSTED_SIGNING_KEYS`.
pub fn read_cached_theme(
    cache_dir: &Path,
    keyring: &[(&str, &str)],
) -> Result<Option<CachedTheme>, String> {
    let path = theme_cache_path(cache_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path)
        .map_err(|error| format!("Gagal membaca cache tema: {error}"))?;
    let cached: CachedTheme =
        serde_json::from_str(&data).map_err(|error| format!("Cache tema tidak valid: {error}"))?;

    let reject = |reason: &str| -> Option<CachedTheme> {
        log::warn!("Cache tema ditolak: {reason}");
        let _ = std::fs::remove_file(&path);
        None
    };

    // Removing a key must revoke the themes it signed, on launchers that never
    // reach the network again. The cache is the only state that outlives a
    // release, so it is the only place revocation can take hold.
    if !keyring.iter().any(|(trusted, _)| *trusted == cached.key_id) {
        return Ok(reject(&format!("key `{}` tidak dipercaya", cached.key_id)));
    }
    // `sync_theme` promotes assets one at a time and writes metadata last, so an
    // interrupted sync can leave this theme's stylesheet beside the previous
    // theme's tokens. The recorded digests are what make that state detectable
    // instead of serving a mix that looks coherent.
    let css_path = cache_dir.join(THEME_CSS_FILE);
    match &cached.css_sha256 {
        Some(expected) => {
            if !verify_sha256(&css_path, expected).unwrap_or(false) {
                return Ok(reject("fragmen CSS tidak cocok dengan metadata"));
            }
            if cached_theme_css(cache_dir)?.is_none() {
                return Ok(reject("fragmen CSS kosong"));
            }
        }
        // A tokens-only theme must not inherit a fragment left by another one.
        None if css_path.exists() => return Ok(reject("fragmen CSS tak terduga")),
        None => {}
    }
    let background_path = cache_dir.join(THEME_BACKGROUND_FILE);
    match &cached.background_sha256 {
        Some(expected) => {
            if !verify_sha256(&background_path, expected).unwrap_or(false) {
                return Ok(reject("latar tidak cocok dengan metadata"));
            }
        }
        None if background_path.exists() => return Ok(reject("latar tak terduga")),
        None => {}
    }
    Ok(Some(cached))
}

/// Removes every trace of a cached theme. Used when the manifest author turns
/// a theme off, which is the only way a shipped theme can be withdrawn without
/// releasing the launcher.
pub fn clear_cached_theme(cache_dir: &Path) -> Result<(), String> {
    for name in [THEME_CACHE_FILE, THEME_CSS_FILE, THEME_BACKGROUND_FILE] {
        let path = cache_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| format!("Gagal menghapus {name}: {error}"))?;
        }
    }
    Ok(())
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
/// Only called for a manifest whose signature has already been verified, and
/// the key that verified it is recorded so removal of that key revokes the
/// cache on launchers that never sync again.
pub async fn sync_theme(
    cache_dir: &Path,
    theme: &ThemeDefinition,
    key_id: &str,
) -> Result<(), String> {
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
            key_id: key_id.to_string(),
            tokens: theme.tokens.clone(),
            css_sha256: theme.theme_css.as_ref().map(|entry| entry.sha256.clone()),
            background_sha256: theme.background.as_ref().map(|entry| entry.sha256.clone()),
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
    // A stylesheet has a hard size ceiling, so refuse it mid-transfer rather
    // than downloading first and measuring afterwards.
    let max_bytes = if destination_name == THEME_CSS_FILE {
        MAX_THEME_CSS_BYTES as u64
    } else {
        MAX_THEME_ASSET_BYTES
    };
    download_file_with_expected_size_limited_policy(
        &entry.url,
        &candidate,
        None,
        max_bytes,
        DownloadRedirectPolicy::AnyHttps,
        |_| {},
    )
    .await?;
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

/// Builds the payload the webview paints, from the last theme this launcher
/// verified. Returns the general theme when nothing verified is cached.
pub fn build_payload(cache_dir: &Path, keyring: &[(&str, &str)]) -> Result<ThemePayload, String> {
    let Some(cached) = read_cached_theme(cache_dir, keyring)? else {
        return Ok(general_payload("general"));
    };
    let background_file = if cached.background_sha256.is_some() {
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
    use crate::engine::media::AssetManifest;
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::Digest;

    const TEST_KEY_ID: &str = "test-key";
    const TEST_SEED: [u8; 32] = [7u8; 32];

    /// Deterministic public halves for the fixed seeds, so the test keyrings are
    /// `const` and no owned value has to be kept alive.
    ///
    /// The production `TRUSTED_SIGNING_KEYS` is never touched from a test: a
    /// publicly known key in that const would let anyone restyle every launcher.
    const TRUSTED_PUBLIC_HEX: &str =
        "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
    const OTHER_PUBLIC_HEX: &str =
        "fd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618";

    const TRUSTED_KEYRING: &[(&str, &str)] = &[(TEST_KEY_ID, TRUSTED_PUBLIC_HEX)];
    const OTHER_KEYRING: &[(&str, &str)] = &[("someone-else", OTHER_PUBLIC_HEX)];

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&TEST_SEED)
    }

    fn sign(bytes: &[u8]) -> String {
        let signature = test_signing_key().sign(bytes);
        format!(
            "{SIGNATURE_HEADER} {TEST_KEY_ID} {}",
            hex::encode(signature.to_bytes())
        )
    }

    fn definition() -> ThemeDefinition {
        ThemeDefinition {
            id: "wuwa-2-4".to_string(),
            name: "Wuthering Waves 2.4".to_string(),
            active: Some(true),
            tokens: BTreeMap::from([("--gold-rgb".to_string(), "255 0 0".to_string())]),
            theme_css: None,
            background: None,
        }
    }

    fn digest(bytes: &[u8]) -> String {
        hex::encode(sha2::Sha256::digest(bytes))
    }

    /// A cache record signed by `key_id`, with digests matching whatever the
    /// caller writes into the cache dir.
    fn cached(key_id: &str, css: Option<&[u8]>, background: Option<&[u8]>) -> CachedTheme {
        CachedTheme {
            id: "wuwa-2-4".to_string(),
            name: "Wuthering Waves 2.4".to_string(),
            key_id: key_id.to_string(),
            tokens: BTreeMap::from([("--gold-rgb".to_string(), "1 2 3".to_string())]),
            css_sha256: css.map(digest),
            background_sha256: background.map(digest),
        }
    }

    /// Writes a complete, trusted cache into `dir`.
    fn primed_cache(dir: &Path) -> CachedTheme {
        let css = b"body{color:red}";
        let background = b"jpeg-bytes";
        std::fs::write(dir.join(THEME_CSS_FILE), css).unwrap();
        std::fs::write(dir.join(THEME_BACKGROUND_FILE), background).unwrap();
        let record = cached(TEST_KEY_ID, Some(css), Some(background));
        write_cached_theme(dir, &record).unwrap();
        record
    }

    // --- signature verification -------------------------------------------

    #[test]
    fn a_trusted_signature_verifies() {
        let payload = b"{\"assets\":[]}";
        assert_eq!(
            verify_with_keyring(payload, &sign(payload), TRUSTED_KEYRING).unwrap(),
            TEST_KEY_ID
        );
    }

    #[test]
    fn signature_from_an_untrusted_key_is_rejected() {
        let payload = b"{\"assets\":[]}";
        let error = verify_with_keyring(payload, &sign(payload), OTHER_KEYRING).unwrap_err();
        assert!(error.contains("tidak dikenal"), "unexpected: {error}");
    }

    #[test]
    fn tampered_manifest_is_rejected() {
        let signature = sign(b"{\"assets\":[]}");
        let error =
            verify_with_keyring(b"{\"assets\":[1]}", &signature, TRUSTED_KEYRING).unwrap_err();
        assert!(error.contains("tidak valid"), "unexpected: {error}");
    }

    #[test]
    fn unsigned_manifest_is_rejected() {
        let error = verify_with_keyring(b"{\"assets\":[]}", "", TRUSTED_KEYRING).unwrap_err();
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
    fn signature_url_sits_next_to_the_manifest() {
        assert_eq!(
            signature_url("https://example.test/Web/assets.json"),
            "https://example.test/Web/assets.json.sig"
        );
    }

    // --- wire format -------------------------------------------------------

    /// The documented authoring example must deserialise into the field the
    /// engine actually reads. A silent `None` loses the whole fragment.
    #[test]
    fn documented_theme_block_parses_into_theme_css() {
        let json = r#"{
            "id": "wuwa-2-4",
            "name": "Wuthering Waves 2.4",
            "active": true,
            "tokens": { "--gold-rgb": "255 210 90" },
            "themeCss": {
                "name": "theme.css",
                "url": "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/Theme/wuwa-2-4/theme.css",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            }
        }"#;
        let parsed: ThemeDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed.theme_css.as_ref().map(|e| e.name.as_str()),
            Some("theme.css")
        );
        assert_eq!(parsed.active, Some(true));
    }

    #[test]
    fn theme_definition_serialises_back_to_camel_case() {
        let encoded = serde_json::to_string(&definition()).unwrap();
        assert!(encoded.contains("\"active\""), "unexpected: {encoded}");
        assert!(!encoded.contains("theme_css"));
    }

    /// A cache written before a field existed must still load, and must be
    /// treated as "no digest recorded" rather than a hard error, or every
    /// already-installed launcher breaks on upgrade.
    #[test]
    fn a_legacy_cache_without_digest_fields_still_loads() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = r#"{"id":"wuwa-2-4","name":"Wuthering Waves 2.4","key_id":"test-key","tokens":{"--gold-rgb":"1 2 3"},"has_background":false}"#;
        std::fs::write(temp.path().join(THEME_CACHE_FILE), legacy).unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), "body{color:red}").unwrap();
        // Deserialisation must succeed; the missing digests become None.
        let parsed: CachedTheme =
            serde_json::from_str(legacy).expect("legacy cache must not fail to parse");
        assert!(parsed.css_sha256.is_none());
        assert!(parsed.background_sha256.is_none());
        // And it is rejected through the ordinary path, not an Err.
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
    }

    // --- validation --------------------------------------------------------

    #[test]
    fn theme_definition_validation_rejects_dangerous_tokens() {
        for bad in [
            "255 0 0; } body {",
            "url(https://evil.example/x.png)",
            "@import 'x'",
            "/* comment */",
            "",
        ] {
            let mut theme = definition();
            theme
                .tokens
                .insert("--gold-rgb".to_string(), bad.to_string());
            assert!(
                validate_theme_definition(&theme).is_err(),
                "accepted token value: {bad:?}"
            );
        }
    }

    #[test]
    fn theme_definition_validation_rejects_bad_token_names() {
        for bad in ["gold-rgb", "--Gold-Rgb", "--gold rgb", "-gold"] {
            let mut theme = definition();
            theme.tokens.insert(bad.to_string(), "1 2 3".to_string());
            assert!(
                validate_theme_definition(&theme).is_err(),
                "accepted token name: {bad:?}"
            );
        }
    }

    #[test]
    fn theme_definition_validation_rejects_bad_ids() {
        for bad in ["", "Wuwa-2-4", &"x".repeat(MAX_THEME_ID_LENGTH + 1)] {
            let mut theme = definition();
            theme.id = bad.to_string();
            assert!(
                validate_theme_definition(&theme).is_err(),
                "accepted id: {bad:?}"
            );
        }
    }

    #[test]
    fn theme_definition_validation_accepts_a_palette() {
        let mut theme = definition();
        theme.tokens.insert(
            "--mist-grad".to_string(),
            "linear-gradient(135deg, #fff 0%, #000 100%)".to_string(),
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
    fn theme_asset_name_must_match() {
        let mut theme = definition();
        theme.background = Some(AssetEntry {
            name: "something-else.jpg".to_string(),
            url: "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/Theme/x/something-else.jpg".to_string(),
            sha256: "a".repeat(64),
        });
        assert!(validate_theme_definition(&theme).is_err());
    }

    #[test]
    fn fragment_validation_blocks_imports() {
        assert!(validate_fragment("@import url(https://evil.example/x.css);").is_err());
        assert!(validate_fragment("body{color:red}").is_ok());
        let oversized = "a".repeat(MAX_THEME_CSS_BYTES + 1);
        assert!(validate_fragment(&oversized).is_err());
    }

    // --- cache lifecycle ---------------------------------------------------

    #[test]
    fn cached_theme_requires_a_present_fragment() {
        let temp = tempfile::tempdir().unwrap();
        let record = cached(TEST_KEY_ID, Some(b"body{color:red}"), None);
        write_cached_theme(temp.path(), &record).unwrap();
        // Metadata without the fragment is not a usable theme, and the stale
        // metadata is dropped so the next sync re-downloads the pair.
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
        assert!(!theme_cache_path(temp.path()).exists());
    }

    #[test]
    fn cached_theme_without_its_background_is_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let record = cached(TEST_KEY_ID, Some(b"body{color:red}"), Some(b"jpeg"));
        write_cached_theme(temp.path(), &record).unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), b"body{color:red}").unwrap();
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
    }

    /// Removing a key must revoke the themes it signed, on launchers that never
    /// reach the network again.
    #[test]
    fn a_cache_signed_by_a_removed_key_is_revoked() {
        let temp = tempfile::tempdir().unwrap();
        primed_cache(temp.path());
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_some());
        assert_eq!(
            build_payload(temp.path(), TRUSTED_KEYRING).unwrap().status,
            "signed"
        );

        std::fs::write(temp.path().join(THEME_CSS_FILE), b"body{color:red}").unwrap();
        write_cached_theme(
            temp.path(),
            &cached("some-retired-key", Some(b"body{color:red}"), None),
        )
        .unwrap();
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
        assert_ne!(
            build_payload(temp.path(), TRUSTED_KEYRING).unwrap().status,
            "signed"
        );
    }

    #[test]
    fn a_cache_with_no_key_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), b"body{color:red}").unwrap();
        write_cached_theme(temp.path(), &cached("", Some(b"body{color:red}"), None)).unwrap();
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
    }

    /// An interrupted sync promotes `theme.css` before it can write metadata, so
    /// the cache can hold this theme's stylesheet beside the previous theme's
    /// tokens. Serving that would look coherent and be wrong.
    #[test]
    fn a_torn_cache_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        primed_cache(temp.path());
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_some());

        // Metadata still describes the old fragment; the file on disk is new.
        std::fs::write(temp.path().join(THEME_CSS_FILE), b"body{color:hotpink}").unwrap();
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
    }

    /// A tokens-only theme is legitimate, and must not inherit a fragment left
    /// behind by a previous one.
    #[test]
    fn a_tokens_only_theme_does_not_inherit_a_fragment() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), b"body{color:red}").unwrap();
        write_cached_theme(temp.path(), &cached(TEST_KEY_ID, None, None)).unwrap();
        assert!(read_cached_theme(temp.path(), TRUSTED_KEYRING)
            .unwrap()
            .is_none());
    }

    #[test]
    fn empty_fragment_is_treated_as_no_theme() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(THEME_CSS_FILE), "   \n").unwrap();
        assert!(cached_theme_css(temp.path()).unwrap().is_none());
    }

    // --- payload -----------------------------------------------------------

    #[test]
    fn build_payload_falls_back_to_the_general_theme() {
        let temp = tempfile::tempdir().unwrap();
        let payload = build_payload(temp.path(), TRUSTED_KEYRING).unwrap();
        assert_eq!(payload.id, GENERAL_THEME_ID);
        assert!(payload.css.is_empty());
        assert!(payload.background_file.is_empty());
    }

    #[test]
    fn build_payload_exposes_cached_tokens_and_fragment() {
        let temp = tempfile::tempdir().unwrap();
        let record = primed_cache(temp.path());
        let payload = build_payload(temp.path(), TRUSTED_KEYRING).unwrap();
        assert_eq!(payload.id, "wuwa-2-4");
        // The key recorded at sync time is the one reported, not the caller's.
        assert_eq!(payload.key_id, record.key_id);
        assert_eq!(payload.css, "body{color:red}");
        assert_eq!(payload.background_file, THEME_BACKGROUND_FILE);
        assert_eq!(payload.tokens.get("--gold-rgb").unwrap(), "1 2 3");
    }

    /// The kill switch: a manifest that deactivates its theme must leave nothing
    /// behind, so the general theme returns without a launcher release.
    #[test]
    fn clearing_the_cache_returns_the_general_theme() {
        let temp = tempfile::tempdir().unwrap();
        primed_cache(temp.path());
        assert_eq!(
            build_payload(temp.path(), TRUSTED_KEYRING).unwrap().id,
            "wuwa-2-4"
        );

        clear_cached_theme(temp.path()).unwrap();
        assert_eq!(
            build_payload(temp.path(), TRUSTED_KEYRING).unwrap().id,
            GENERAL_THEME_ID
        );
        for name in [THEME_CACHE_FILE, THEME_CSS_FILE, THEME_BACKGROUND_FILE] {
            assert!(
                !temp.path().join(name).exists(),
                "{name} survived the clear"
            );
        }
    }

    // --- manifest intent ---------------------------------------------------

    /// The decision that decides whether a launcher's theme survives lives here,
    /// not in the command layer, so it is testable without a Tauri app.
    #[test]
    fn an_active_theme_activates() {
        let theme = definition();
        assert_eq!(resolve_theme_action(Some(&theme)), ThemeAction::Activate);
    }

    /// The kill switch: an explicit `active: false` withdraws the theme.
    #[test]
    fn an_inactive_theme_withdraws() {
        let mut theme = definition();
        theme.active = Some(false);
        assert_eq!(resolve_theme_action(Some(&theme)), ThemeAction::Withdraw);
    }

    /// A manifest with no theme block is silence, not a withdrawal. Treating it
    /// as one would let any manifest lacking the block delete the fleet's theme.
    #[test]
    fn a_manifest_without_a_theme_block_keeps_the_cache() {
        assert_eq!(resolve_theme_action(None), ThemeAction::Keep);
    }

    /// A draft that never said `"active": true` is a theme that fails to
    /// appear, not one that silently disappears for every user.
    #[test]
    fn a_theme_block_omitting_active_keeps_the_cache() {
        let theme: ThemeDefinition =
            serde_json::from_str(r#"{"id":"wuwa-2-4","name":"Wuthering Waves 2.4","tokens":{}}"#)
                .unwrap();
        assert_eq!(theme.active, None, "an omitted active must stay absent");
        assert_eq!(resolve_theme_action(Some(&theme)), ThemeAction::Keep);
    }

    /// The real pre-feature manifest parses and keeps whatever was verified.
    #[test]
    fn a_manifest_without_theming_keeps_the_cached_theme() {
        let manifest: AssetManifest =
            serde_json::from_str(r#"{"assets":[],"theme":null}"#).unwrap();
        assert_eq!(
            resolve_theme_action(manifest.theme.as_ref()),
            ThemeAction::Keep
        );
        let no_key: AssetManifest = serde_json::from_str(r#"{"assets":[]}"#).unwrap();
        assert_eq!(
            resolve_theme_action(no_key.theme.as_ref()),
            ThemeAction::Keep
        );
    }

    #[test]
    fn clearing_an_empty_cache_is_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        assert!(clear_cached_theme(temp.path()).is_ok());
    }
}
