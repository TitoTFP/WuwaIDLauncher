//! One measured scenario per launcher state, on the counting allocator in
//! `super::perf_probe`.
//!
//! The states are the ones the work item names: idle, startup, update check,
//! download, and install/extract. Each scenario builds a real fixture, measures
//! the code path a launch would actually take, prints a `WUL1|` line, and
//! asserts on allocations and bytes only — see the policy note in
//! `super::perf_probe` for why a timing is never an assertion.
//!
//! Run them single-threaded and with output captured:
//!
//! ```text
//! cargo test --locked --manifest-path src-tauri/Cargo.toml --lib perf_scenarios -- --test-threads=1 --nocapture
//! ```
//!
//! `--test-threads=1` is not optional: the probe's arm flag is process-wide, so
//! two scenarios at once would count each other's allocations.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::perf_probe::{measure, report};
use crate::engine::downloader::{compute_sha256, hash_downloaded_prefix};
use crate::engine::{installer, media, metadata, repak, runtime, settings, theme, updater};
use crate::RuntimeCoordinator;
use sha2::{Digest, Sha256};

// -----------------------------------------------------------------------------
// Fixtures
// -----------------------------------------------------------------------------

/// Points the launcher at a private AppData folder for the length of one
/// scenario and puts the previous value back on drop, so a failing assertion
/// cannot leak the override into the next test.
struct Appdata {
    previous: Option<std::ffi::OsString>,
}

impl Appdata {
    fn enter(path: &Path) -> Self {
        let previous = std::env::var_os("WUWAID_E2E_APPDATA");
        std::env::set_var("WUWAID_E2E_APPDATA", path);
        Self { previous }
    }
}

impl Drop for Appdata {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("WUWAID_E2E_APPDATA", value),
            None => std::env::remove_var("WUWAID_E2E_APPDATA"),
        }
    }
}

/// A game directory the path validators accept, with nothing else in it: the
/// executable they look for, and that is the whole contract.
fn fake_game(root: &Path) -> PathBuf {
    let executable = root.join(crate::engine::path::GAME_EXE_RELATIVE);
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"mock game executable").unwrap();
    root.to_path_buf()
}

/// Deterministic filler, so building a fixture costs one allocation and one
/// write and never leaks whatever a previous run left behind.
fn filler(size: usize) -> Vec<u8> {
    vec![0x5Au8; size]
}

/// A stylesheet fragment of roughly `kib` kibibytes, well under the 128 KiB
/// ceiling `validate_fragment` enforces. A real theme fragment is a few
/// kilobytes of rules, and it is the fragment that gets copied two or three
/// times per payload, so its size is what the theme budget is about.
fn css_fragment(kib: usize) -> String {
    let mut css = String::new();
    while css.len() < kib * 1024 {
        css.push_str(".wuwaid-rule { --wg-color: rgb(12, 34, 56); }\n");
    }
    css
}

/// A loopback HTTP origin that answers `requests` requests with one body and
/// then stops. Loopback is the only scheme a manifest fetch accepts in a test,
/// and the count matters: the probe warms once before it arms.
fn serve_bytes(body: Vec<u8>, requests: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for _ in 0..requests {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    format!("http://{address}/Web/assets.json")
}

/// Writes a settings file pointing at `game`, the way a configured launcher
/// has it on disk.
fn write_settings(appdata: &Path, game: &Path) {
    let configured = settings::LauncherSettings {
        game_path: game.to_string_lossy().to_string(),
        ..settings::LauncherSettings::default()
    };
    std::fs::write(
        appdata.join("settings.json"),
        serde_json::to_vec(&configured).unwrap(),
    )
    .unwrap();
}

/// A keyring of a key that exists nowhere but here. The production
/// `TRUSTED_SIGNING_KEYS` must never grow a published key, and the probe has no
/// private half of a real one to sign a fixture with.
const TEST_KEY_ID: &str = "perf-probe-key";
const TEST_SEED: [u8; 32] = [23u8; 32];

fn test_keyring() -> Vec<(&'static str, String)> {
    let public = ed25519_dalek::SigningKey::from_bytes(&TEST_SEED).verifying_key();
    vec![(TEST_KEY_ID, hex::encode(public.to_bytes()))]
}

fn as_keyring<'a>(owned: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    owned
        .iter()
        .map(|(id, public)| (*id, public.as_str()))
        .collect()
}

// -----------------------------------------------------------------------------
// idle
// -----------------------------------------------------------------------------

/// The 2-second monitor tick is the launcher's floor: it runs forever, on
/// every launcher, whether or not a game is running. This is the tick body
/// without the two Tauri calls around it — the window handle and the event
/// emit — neither of which allocates.
///
/// The coordinator lives as long as the app does, exactly as it does in the
/// monitor loop, so the resolution of the configured game is paid once instead
/// of once per tick: what is left per tick is the cached read, a path clone
/// and the process reconciliation.
#[test]
fn idle_monitor_tick() {
    const ITERATIONS: u64 = 64;
    // Reading a resolved path out of the coordinator costs one lock, one clone
    // and one join. Anything that grows with the tick — a settings read, a
    // parse, a canonicalize — belongs in the first tick, not in the thirty per
    // minute that follow it.
    const ALLOCS_PER_RUN: u64 = 8;
    const BYTES_PER_RUN: u64 = 1_024;

    let appdata = tempfile::tempdir().unwrap();
    let game = tempfile::tempdir().unwrap();
    let game = fake_game(game.path());
    let canonical_game = std::fs::canonicalize(&game).unwrap();
    let _appdata = Appdata::enter(appdata.path());
    write_settings(appdata.path(), &game);

    let coordinator = RuntimeCoordinator::default();
    // One cache for the whole run, exactly as the monitor loop keeps between
    // ticks: a cache rebuilt per tick would hide its own cost.
    let mut cache = runtime::ProcessSnapshotCache::default();
    let metrics = measure(ITERATIONS, || {
        let expected_executable = coordinator.configured_game_executable();
        let inspection = runtime::inspect_runtime_processes_with_cache(
            None,
            None,
            None,
            None,
            expected_executable.as_deref(),
            false,
            &mut cache,
        );
        runtime::reconcile_runtime_state(None, inspection.detected_pid);
    });
    report("idle.monitor_tick", ITERATIONS, metrics);
    assert_eq!(
        coordinator.configured_game_executable(),
        Some(canonical_game.join(crate::engine::path::GAME_EXE_RELATIVE)),
        "the cached path must still be the configured game the tick watches"
    );
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "idle.monitor_tick allocated {} times per tick, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "idle.monitor_tick handed out {} bytes per tick, budget {BYTES_PER_RUN}",
        metrics.bytes_per_run(ITERATIONS)
    );
}

/// A settings read and parse: what the UI asks for on every panel open, and
/// what the monitor tick does before anything else.
#[test]
fn idle_settings_read_parse() {
    const ITERATIONS: u64 = 64;
    // A nine-field settings object is a handful of small allocations and
    // nothing else. This is the floor of the launcher's per-request work, so
    // the budget is tight on purpose: an extra copy of the parsed settings, or
    // a diagnostic list built eagerly, breaches it immediately.
    const ALLOCS_PER_RUN: u64 = 32;
    // The same read in bytes: the object is under a kilobyte, and nothing on
    // this path is entitled to a second copy of it.
    const BYTES_PER_RUN: u64 = 1_200;

    let appdata = tempfile::tempdir().unwrap();
    let game = tempfile::tempdir().unwrap();
    let game = fake_game(game.path());
    let _appdata = Appdata::enter(appdata.path());
    write_settings(appdata.path(), &game);
    let raw = std::fs::read_to_string(appdata.path().join("settings.json")).unwrap();

    let metrics = measure(ITERATIONS, || {
        let loaded = settings::normalize_settings_json(&raw);
        std::hint::black_box(&loaded);
    });
    report("idle.settings_read_parse", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "idle.settings_read_parse allocated {} times per read, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "idle.settings_read_parse handed out {} bytes per read, budget {BYTES_PER_RUN}",
        metrics.bytes_per_run(ITERATIONS)
    );
}

// -----------------------------------------------------------------------------
// startup
// -----------------------------------------------------------------------------

/// Everything a launch spends on the cached media before it can promise a BGM:
/// the digest check that decides whether the cache is usable, and the media sync
/// that follows it. On a warm cache with an unchanged manifest the sync has
/// nothing to download and never awaits, so `block_on` drives it to completion
/// in one go.
#[test]
fn startup_cached_media_validate() {
    const ITERATIONS: u64 = 4;
    // The two media files are hashed once between the two calls, never twice:
    // a second pass over the same digests is read I/O and CPU that proves
    // nothing. 15.6 MB of media hashed twice is exactly the 15.6 MB of extra
    // startup reads this budget exists to keep gone, and it halves the hashing
    // time of the scenario, which the `us/run` column shows. Hashing streams
    // through a stack buffer, so the allocation counter only sees the digest
    // strings: this budget is deliberately close, and the I/O saving is what
    // the Windows matrix's read-I/O row measures.
    const ALLOCS_PER_RUN: u64 = 36;
    const BYTES_PER_RUN: u64 = 4_800;

    let appdata = tempfile::tempdir().unwrap();
    let _appdata = Appdata::enter(appdata.path());
    let cache = appdata.path().join("Cache");
    std::fs::create_dir_all(&cache).unwrap();
    for name in media::MEDIA_ASSET_NAMES {
        std::fs::write(cache.join(name), filler(4 * 1024 * 1024)).unwrap();
    }
    let manifest = media::AssetManifest {
        update_date: None,
        theme: None,
        assets: media::MEDIA_ASSET_NAMES
            .iter()
            .map(|name| media::AssetEntry {
                name: (*name).to_string(),
                url: format!("http://127.0.0.1/{name}"),
                sha256: compute_sha256(&cache.join(name)).unwrap(),
            })
            .collect(),
    };

    let metrics = measure(ITERATIONS, || {
        let verified = media::validate_cached_media(&cache, &manifest).unwrap();
        let payload = tauri::async_runtime::block_on(media::sync_media(
            &cache,
            &manifest,
            &verified,
            |_, _| {},
        ))
        .expect("a warm cache must sync without downloading anything");
        std::hint::black_box((verified, payload));
    });
    report("startup.cached_media_validate", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "startup.cached_media_validate allocated {} times per launch, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "startup.cached_media_validate handed out {} bytes per launch, budget {BYTES_PER_RUN}",
        metrics.bytes_per_run(ITERATIONS)
    );
}

/// The theme cache read behind `get_active_theme` and `emit_theme_payload`,
/// which is twice per launch. It is the launcher's only path that reads and
/// re-validates a file it has already read.
#[test]
fn startup_theme_cache_read() {
    const ITERATIONS: u64 = 64;
    // A signed theme with a 16 KiB fragment: the metadata record, one read of
    // the fragment, and a digest over the bytes already in hand. The fragment
    // must not be copied to be scanned for forbidden tokens, and it must not be
    // read a second time to fill the payload — either one costs whole copies of
    // it, which this budget cannot absorb.
    const ALLOCS_PER_RUN: u64 = 28;
    const BYTES_PER_RUN: u64 = 24 * 1024;

    let temp = tempfile::tempdir().unwrap();
    let css = css_fragment(16);
    let background = b"jpeg-bytes";
    std::fs::write(temp.path().join(theme::THEME_CSS_FILE), &css).unwrap();
    std::fs::write(temp.path().join(theme::THEME_BACKGROUND_FILE), background).unwrap();
    let record = theme::CachedTheme {
        id: "wuwa-2-4".to_string(),
        name: "Wuthering Waves 2.4".to_string(),
        key_id: TEST_KEY_ID.to_string(),
        tokens: std::collections::BTreeMap::from([("--gold-rgb".to_string(), "1 2 3".to_string())]),
        css_sha256: Some(compute_sha256(&temp.path().join(theme::THEME_CSS_FILE)).unwrap()),
        background_sha256: Some(
            compute_sha256(&temp.path().join(theme::THEME_BACKGROUND_FILE)).unwrap(),
        ),
    };
    std::fs::write(
        temp.path().join("theme-cache.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    let owned = test_keyring();
    let keyring = as_keyring(&owned);

    let metrics = measure(ITERATIONS, || {
        let payload = theme::build_payload(temp.path(), &keyring).unwrap();
        std::hint::black_box(&payload);
    });
    report("startup.theme_cache_read", ITERATIONS, metrics);
    assert_eq!(
        theme::build_payload(temp.path(), &keyring).unwrap().css,
        css,
        "the payload must still carry the verified fragment"
    );
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "startup.theme_cache_read allocated {} times per read, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "startup.theme_cache_read handed out {} bytes per read, budget {BYTES_PER_RUN}: the fragment is being copied again",
        metrics.bytes_per_run(ITERATIONS)
    );
}

/// The manifest fetch that opens every launch. A response body is already an
/// owned buffer by the time it reaches the decoder, so decoding it is a move.
/// The fixture is a manifest at the size cap `fetch_manifest_bytes` enforces,
/// so the copy this guards against is one whole body and cannot hide in the
/// noise a socket adds.
#[test]
fn startup_manifest_fetch_body() {
    const ITERATIONS: u64 = 16;
    // A body of `MAX_MANIFEST_BYTES` carrying a real manifest plus a padding
    // field the parser skips, so the transfer dominates everything else. The
    // headroom is well under one body: a duplicated body of memory does not
    // fit inside it.
    const BYTES_PER_RUN: u64 = 6_000_000;
    // Measured: 5,419,148 bytes per fetch with the body moved rather than
    // copied, and 6,500,470 with the copy. The budget sits between the two.

    let padding = "a".repeat(1024 * 1024 - 256);
    let body = format!(r#"{{"update_date":null,"assets":[],"padding":"{padding}"}}"#).into_bytes();
    assert!(
        media::parse_manifest(std::str::from_utf8(&body).unwrap())
            .map(|manifest| manifest.assets.is_empty())
            == Ok(true),
        "the fixture must be a manifest the parser actually reads"
    );
    let url = serve_bytes(body, ITERATIONS as usize + 1);
    let client = reqwest::Client::new();

    let metrics = measure(ITERATIONS, || {
        let fetched = tauri::async_runtime::block_on(media::fetch_manifest_bytes(&client, &url));
        std::hint::black_box(&fetched);
    });
    report("startup.manifest_fetch_body", ITERATIONS, metrics);
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "startup.manifest_fetch_body handed out {} bytes per fetch, budget {BYTES_PER_RUN}",
        metrics.bytes_per_run(ITERATIONS)
    );
}

// -----------------------------------------------------------------------------
// update check
// -----------------------------------------------------------------------------

/// The one field the patch-status check reads out of `versions.json`. The file
/// is a small tree, and reading a string out of it must not clone the tree to
/// do it.
#[test]
fn update_check_read_game_field_only() {
    const ITERATIONS: u64 = 256;
    // One parse of a three-field entry and one clone of the leaf. The old shape
    // cloned the whole document per field, and the status check asks for three
    // of them, so the budget is one leaf's worth of work, not one document's.
    // Measured: 16 allocations and 2,176 bytes now, 44 and 6,224 before.
    const ALLOCS_PER_RUN: u64 = 24;
    const BYTES_PER_RUN: u64 = 3_000;

    let temp = tempfile::tempdir().unwrap();
    let game = fake_game(temp.path());
    let versions = temp.path().join("versions.json");
    let document = serde_json::json!({
        "_schemaVersion": 3,
        "games": {
            metadata::game_key(&game).unwrap(): {
                "_vhVersion": "3.6.1-id.2",
                "_installMethod": "resource_mount",
                "_patchVariant": "hide_uid",
            }
        }
    });
    std::fs::write(&versions, serde_json::to_vec(&document).unwrap()).unwrap();

    let metrics = measure(ITERATIONS, || {
        let field = metadata::read_game_field(&versions, &game, "_vhVersion");
        std::hint::black_box(&field);
    });
    report("update_check.read_game_field_only", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "update_check.read_game_field_only allocated {} times per read, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "update_check.read_game_field_only handed out {} bytes per read, budget {BYTES_PER_RUN}: the document is being cloned again",
        metrics.bytes_per_run(ITERATIONS)
    );
}

// -----------------------------------------------------------------------------
// download
// -----------------------------------------------------------------------------

/// The whole-file read behind a download. One buffer, grown once; the cost is
/// the file, and the budget is that file and nothing beside it.
#[test]
fn download_fs_read_32mib() {
    const ITERATIONS: u64 = 4;
    // A 32 MiB read allocates one buffer, and the allocator doubles it on the
    // way up, so the honest ceiling is two payload copies in flight. More than
    // that is a second copy of the download.
    const BYTES_PER_RUN: u64 = 40 * 1024 * 1024;

    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("patch.zip");
    std::fs::write(&archive, filler(32 * 1024 * 1024)).unwrap();

    let metrics = measure(ITERATIONS, || {
        let data = std::fs::read(&archive).unwrap();
        std::hint::black_box(&data);
    });
    report("download.fs_read_32mib", ITERATIONS, metrics);
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "download.fs_read_32mib handed out {} bytes per read, budget {BYTES_PER_RUN}",
        metrics.bytes_per_run(ITERATIONS)
    );
}

/// The SHA-256 over a downloaded patch, streamed through a fixed stack buffer.
#[test]
fn download_compute_sha256_32mib() {
    const ITERATIONS: u64 = 4;
    // The digest of a file is a hex string and nothing else. The read buffer
    // is a fixed array on the stack, so the whole operation costs a couple of
    // small allocations however large the file is.
    const ALLOCS_PER_RUN: u64 = 4;

    let temp = tempfile::tempdir().unwrap();
    let patch = temp.path().join("pakchunk0-ID.pak");
    std::fs::write(&patch, filler(32 * 1024 * 1024)).unwrap();

    let metrics = measure(ITERATIONS, || {
        let digest = compute_sha256(&patch).unwrap();
        std::hint::black_box(&digest);
    });
    report("download.compute_sha256_32mib", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "download.compute_sha256_32mib allocated {} times per hash, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
}

/// The bytes a resumed download does not fetch. The digest a download reports
/// has to cover them, or a checksum would vouch for a file the launcher never
/// read end to end — so they are hashed from the partial file through the same
/// fixed stack buffer, and the cost stays a buffer and a hex string whatever
/// the file weighs.
#[test]
fn download_resume_prefix_digest() {
    const ITERATIONS: u64 = 4;
    // A digest and nothing else. Reading the skipped prefix into memory to hash
    // it would cost a whole copy of it, which no allocation budget this small
    // can absorb.
    const ALLOCS_PER_RUN: u64 = 4;
    const BYTES_PER_RUN: u64 = 1_024;

    const SKIPPED_BYTES: u64 = 12 * 1024 * 1024;
    let temp = tempfile::tempdir().unwrap();
    let partial = temp.path().join("update.zip.part");
    let bytes = filler(16 * 1024 * 1024);
    std::fs::write(&partial, &bytes).unwrap();
    let expected = hex::encode(Sha256::digest(&bytes[..SKIPPED_BYTES as usize])).to_lowercase();

    let metrics = measure(ITERATIONS, || {
        let mut hasher = Sha256::new();
        hash_downloaded_prefix(&partial, SKIPPED_BYTES, &mut hasher).unwrap();
        std::hint::black_box(hex::encode(hasher.finalize()));
    });
    report("download.resume_prefix_digest", ITERATIONS, metrics);

    let mut hasher = Sha256::new();
    hash_downloaded_prefix(&partial, SKIPPED_BYTES, &mut hasher).unwrap();
    assert_eq!(
        hex::encode(hasher.finalize()).to_lowercase(),
        expected,
        "a resumed download must hash the bytes it skipped over"
    );
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "download.resume_prefix_digest allocated {} times, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
    assert!(
        metrics.bytes_per_run(ITERATIONS) <= BYTES_PER_RUN,
        "download.resume_prefix_digest handed out {} bytes, budget {BYTES_PER_RUN}: the skipped prefix is being buffered",
        metrics.bytes_per_run(ITERATIONS)
    );
}

/// The archive inspection that runs on a downloaded update before anything is
/// unpacked. It is pure metadata work: a bounded walk of the central directory.
#[test]
fn download_validate_archive() {
    const ITERATIONS: u64 = 16;
    // One walk of the archive, measured at 19 allocations. The update path used
    // to pay this twice — once in `perform_launcher_update` and again inside
    // `extract_zip_update`, which validates as its first statement and rejects
    // with the same strings — and the duplicate call is gone, so the budget is
    // one walk plus a small platform margin, not two walks' worth of room.
    // The duplicate-entry set and the path handling are what make this worth
    // bounding, because both grow with entry count.
    const ALLOCS_PER_RUN: u64 = 21;

    let archive = update_archive_fixture();
    let metrics = measure(ITERATIONS, || {
        let checked = updater::validate_update_archive(&archive, updater::RELEASE_EXECUTABLE_NAME);
        std::hint::black_box(&checked);
    });
    report("download.validate_archive", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "download.validate_archive allocated {} times per validation, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
}

/// The smallest archive the updater will accept: the release executable and
/// nothing else, which is also the shape a real release ZIP has at its head.
fn update_archive_fixture() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        zip.start_file(
            updater::RELEASE_EXECUTABLE_NAME,
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"launcher").unwrap();
        zip.finish().unwrap();
    }
    buffer.into_inner()
}

// -----------------------------------------------------------------------------
// install / extract
// -----------------------------------------------------------------------------

/// The SHA-256 over the patch payload the install path verifies before it
/// touches the game.
#[test]
fn install_sha256_16mib_once() {
    const ITERATIONS: u64 = 4;
    // The same shape as the download digest and the same budget: one hex
    // string, streamed through a stack buffer. An install is where a second
    // copy of the payload would cost most, because it happens while the game
    // directory is already half written.
    const ALLOCS_PER_RUN: u64 = 4;

    let temp = tempfile::tempdir().unwrap();
    let payload = temp.path().join("pakchunk0-ID-WindowsNoEditor_1000_P.pak");
    std::fs::write(&payload, filler(16 * 1024 * 1024)).unwrap();

    let metrics = measure(ITERATIONS, || {
        let digest = compute_sha256(&payload).unwrap();
        std::hint::black_box(&digest);
    });
    report("install.sha256_16mib_once", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "install.sha256_16mib_once allocated {} times per hash, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
}

/// Extract a v12 pak and build it back, the way a loader-method install
/// rewrites the patch archive, plus the SHA-1 index check that decides whether
/// the result is usable at all.
///
/// This is the body the install transaction runs, and it is now run on a
/// blocking thread rather than on a runtime worker: the archive rewrite is
/// unchanged, so the allocations are unchanged, and what the budget now bounds
/// is the work parked on the thread the install blocks on instead of the work
/// stalling the runtime.
#[test]
fn install_repak_round_trip() {
    const ITERATIONS: u64 = 4;
    // Three small entries in, three out, measured at 133 allocations. The cost
    // is the directory walk and the per-entry buffers, so the budget is
    // proportional to the entries and not to the bytes: an extract that starts
    // holding a whole archive in memory breaches it. Re-cut around the measured
    // value once the work moved off the runtime, with headroom for the same
    // three entries on another platform.
    const ALLOCS_PER_RUN: u64 = 160;

    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    for index in 0..3 {
        let entry = source_dir
            .join("Client/Content")
            .join(format!("entry-{index}.txt"));
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, filler(4096)).unwrap();
    }
    let source_pak = temp.path().join("source.pak");
    repak::pack_v12(&source_dir, &source_pak).unwrap();
    let unpacked = temp.path().join("unpacked");
    let rebuilt = temp.path().join("rebuilt.pak");

    let metrics = measure(ITERATIONS, || {
        let _ = std::fs::remove_dir_all(&unpacked);
        repak::unpack_v12(&source_pak, &unpacked).unwrap();
        repak::pack_v12(&unpacked, &rebuilt).unwrap();
        let valid = installer::validate_pak_file(&rebuilt).unwrap();
        std::hint::black_box(valid);
    });
    report("install.repak_round_trip", ITERATIONS, metrics);
    assert!(
        metrics.allocs_per_run(ITERATIONS) <= ALLOCS_PER_RUN,
        "install.repak_round_trip allocated {} times per round trip, budget {ALLOCS_PER_RUN}",
        metrics.allocs_per_run(ITERATIONS)
    );
}
