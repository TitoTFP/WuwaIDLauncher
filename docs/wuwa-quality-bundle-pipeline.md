# How Wuthering Waves SD / HD / UHD actually works

Date: 2026-10-01
Status: Reverse engineering of `reverse-engineer/WutheringWaves_overseas_setup_3.0.1.0.exe` (Kuro Games, `Guangzhou Kuro Technology Co., Ltd`, `InternalName: launcher.exe`, v3.0.1.0) plus the locally installed Wuthering Waves 3.7 client at `~/.local/share/Steam/steamapps/common/Wuthering Waves`. No launcher code changed.

## Scope and method

Question answered: what is the workflow/pipeline behind the SD, HD and UHD modes.

The installer contains no game binaries — only the official launcher — so the launcher-side pipeline comes from the installer, the live server manifests that launcher consumes at runtime, and finally the locally installed 3.7 client for the game-side consumer. Nothing under `reverse-engineer/` is committed; see `.gitignore`.

Artifacts produced under `reverse-engineer/` (not committed; see `.gitignore`):

| Path | What it is |
| --- | --- |
| `extracted/$PLUGINSDIR/` | NSIS payload (15 files) |
| `krlauncher/3.0.1.0/` | Unpacked launcher (537 files, locale dirs skipped) |
| `decompiled/launcher_main/` | ILSpy decompile of `launcher_main.dll` (319 `.cs`) |
| `decompiled/KRApp.conf.json` | Decrypted launcher config (see below) |
| `decompiled/game_index.json` | Live game server config, fetched from the launcher's own `configUrl` |
| `decompiled/packindex/{common,sd,hd,uhd}.json` | Per-pack file manifests, fetched from the game CDN |
| `feapp/` | `krfeapp.dat` — the WebView2 UI (Vue bundle), unzipped |
| `tools/` | `ilspycmd` 9.1.0.7988 (standalone, runs on the installed .NET 8 runtime) |

Reproduction commands:

```bash
7z x -y -oreverse-engineer/extracted reverse-engineer/WutheringWaves_overseas_setup_3.0.1.0.exe
7z x -y -oreverse-engineer/krlauncher reverse-engineer/extracted/'$PLUGINSDIR'/KRLauncher.zip
dotnet reverse-engineer/tools/tools/net8.0/any/ilspycmd.dll \
  reverse-engineer/krlauncher/3.0.1.0/launcher_main.dll \
  -o reverse-engineer/decompiled/launcher_main -p -r reverse-engineer/krlauncher/3.0.1.0
```

Game-side reproduction (local 3.7 install):

```bash
G=~/.local/share/Steam/steamapps/common/'Wuthering Waves'
S="$G/Client/Binaries/Win64/Client-Win64-Shipping.exe"
strings -el -n 5 "$S" | grep -nE 'krqlv|Use launcher'   # -krqlv= , kuro: Use launcher to start game!
strings -n 4 "$G/Wuthering Waves.exe" | grep -i pdb        # BootstrapPackagedGame-Win64-Shipping.pdb
ls "$G/Client/Content"                                     # HD + Paks only
```

`Assets/KRApp.conf` is not plain JSON. `KRLauncherConfig.Init()` decodes it as `Xor(Base64Decode(text), 0x63)`; if `kr_game_cache/KRConfig.json` exists it wins over the embedded copy, so the shipped file is a fallback for the server-pushed one.

## Installer anatomy

The NSIS script is tiny — it only unpacks and hands off. Its string table (recovered by parsing the NSIS first header at file offset `438272` and inflating the 1108-byte header block):

```text
$PROGRAMFILES\Wuthering Waves            install root
Wuthering Waves Setup                     window title
$PLUGINSDIR\{KRNSISPlugin.dll, DuiLib.dll, libcrypto-3.dll, libcurl.dll,
             libssl-3.dll, sqlite3.dll, zlibwapi.dll, thinkingdata.dll,
             kurodata.dll, msvcp_win.dll, msvcp140.dll, ucrtbase.dll,
             vcruntime140.dll, KRRes.zip, KRLauncher.zip}
```

- `KRRes.zip` (43 MB) is the DuiLib-based installer UI only (`layout/*.xml`, `image/*`).
- `KRLauncher.zip` (184 MB) is the real launcher: `launcher_main.dll` (.NET 8, WPF host + WebView2 UI), `launcher_updater.dll`, `hpatchz.exe`, `KRInstallExternal.exe`, `krfeapp.dat`, `Assets/KRApp.conf`, `filechecklist.json`.

`launcher_main.dll` is fully decompilable — no obfuscation.

## The three modes are "bundles"

There is no separate SD/HD/UHD executable and no separate install directory. The tier is a **bundle**, and a bundle is exactly two things: a set of resource packs, and a launch argument.

From the decrypted `KRApp.conf` (`gameId: G153`, `pkgId: A1730`, `resId: 50004`, `serverCode: official`):

```json
"defaultBundleName": "HD",
"bundleRecommendPriority": ["UHD", "HD", "SD"],
"bundles": [
  { "name": "UHD", "bundleDirName": "Wuthering Waves Game", "launchArgs": ["-krqlv=uhd"] },
  { "name": "HD",  "bundleDirName": "Wuthering Waves Game", "launchArgs": ["-krqlv=hd"]  },
  { "name": "SD",  "bundleDirName": "Wuthering Waves Game", "launchArgs": ["-krqlv=sd"]  }
]
```

All three share `bundleDirName`, so switching tier reuses one install directory. `defaultBundleName` is `HD`, which is exactly the `-krqlv=hd` that Steam passes in the observed launch command ([`game-launch-3.7-steam-proton.md`](game-launch-3.7-steam-proton.md)) — that is not a coincidence, it is the same default.

Code path: `KRLauncherConfig.GetGameConfig(gameId).Bundles.FirstOrDefault(b => b.Name == bundleName).LaunchArgs`, consumed by `KRBundleLaunchArgsResolver.Resolve()`.

## Tier → resource pack

The live server config (`bundles[*].resourcePacks`, `resourcePacks[*]`) at game version `3.7.0`:

| Bundle | Packs | Download size |
| --- | --- | --- |
| SD | `common` + `sd` | 21.97 GB + 40.17 GB |
| HD | `common` + `hd` | 45.71 GB + 40.17 GB |
| UHD | `common` + `uhd` | 66.04 GB + 40.17 GB |

`common` is tier-independent: 406 files under `Client/Binaries/Win64`, the DLSS/XeSS/Streamline/PhysX plugins, and the **non-tiered** paks `Client/Content/Paks/pakchunk{0..57,70,101..113}-WindowsNoEditor.pak` (~33 GiB). The three tier packs are each exactly 50 `.pak` + 50 `.sig`:

```text
Client/Content/SD/pakchunk{N}-SD-WindowsNoEditor.pak
Client/Content/HD/pakchunk{N}-HD-WindowsNoEditor.pak
Client/Content/UHD/pakchunk{N}-UHD-WindowsNoEditor.pak
```

The chunk-number set is **identical** across all three tiers — `[1,4,5,7,8,9,20..23,26..28,31..36,39..42,45..57,70,101..113]` — only the payload differs. So the three tiers are three parallel cooks of the same logical content, mounted from three different roots. Per-chunk size ratios from the manifests:

| chunk | SD | HD | UHD | HD/SD | UHD/SD |
| --- | --- | --- | --- | --- | --- |
| 9 | 658.4 MiB | 661.6 MiB | 668.0 MiB | 1.00 | 1.01 |
| 47 | 835.0 MiB | 1190.8 MiB | 1538.8 MiB | 1.43 | 1.84 |
| 54 | 390.2 MiB | 1114.5 MiB | 1700.7 MiB | 2.86 | 4.36 |
| 70 | 6642.6 MiB | 18582.7 MiB | 28262.8 MiB | 2.80 | 4.25 |
| **total** | **20.46 GiB** | **42.57 GiB** | **61.50 GiB** | 2.08 | 3.01 |

The size spread across chunks is *not* uniform: chunk 9 is 1.00× from SD to UHD while chunk 70 is 4.25×. That pattern is consistent with the tiers shipping the same logical assets at different LODs (geometry/code chunks barely move; texture-heavy chunks multiply), but the manifest does not name asset types, so the geometry-vs-texture attribution is **inference from size ratios, not proven**. Net install footprint: SD ≈ 58 GiB, HD ≈ 80 GiB, UHD ≈ 99 GiB.

## Runtime pipeline

```text
launcher_main.exe
  └─ KRLauncherConfig.Init()          base64 → XOR 0x63 → KRApp.conf   (or kr_game_cache/KRConfig.json)
  └─ fetch LauncherConfigUrl          launcher index.json (server)
  └─ fetch game configUrl             KRGameServerConfig {cdnList, resourcePacks, bundles, config}
        └─ bundles[bundleName].resourcePacks → [common, <tier>]
              └─ for each pack: fetch <cdn>/<indexFile>  (md5-verified → gameResources.json)
                    └─ { resource[] }  dest/md5/size/chunkInfos   → download + apply
  └─ H5 UI (krfeapp.dat)  ──kr_check_update_status / kr_update──▶  flow
  └─ H5 UI  ──kr_set_rhi_info / kr_alter_command──▶  cache JSON
  └─ H5 UI  ──kr_start_game_process──▶  Process.Start("Wuthering Waves.exe", args)
```

Concrete mechanics:

- **Pack → files.** `KRCheckUpdateFlow.RebuildUpdateInfoFromBundle()` takes `bundles[bundleName].ResourcePacks`, resolves each against `resourcePacks`, and builds a `KRPackUpdateInfo` (index URL, md5, baseUrl, version, size, patch/zip config). `resourcePacks[0]` is the *primary* — its `version` is the install version.
- **Index → files.** `indexFile` is fetched from the **game CDN** (`cdnList[].url`, e.g. `hw-pcdownload-qcloud.aki-game.net`), not the config CDN, and md5-checked before being parsed as `KRIndexFile` (`resource[]` / `groupResource[]` / `patchInfos[]` / `zipInfos[]` / `deleteFiles[]`). A tier switch therefore rewrites `Client/Content/<OLD>/` → `Client/Content/<NEW>/` through the normal diff-and-apply path; nothing special-cases tiers.
- **Apply method.** Server experiment `apply.applyMethodFeature = "group"` selects `groupResource` over `resource` in `KRIndexFile.Resource`.
- **Integrity.** Repair's `directoryIntegrityCheckList` is `[{dir: Client/Content/Paks, exts: [pak, sig], recursive: true}]`, and `keyFileCheckList` covers `Wuthering Waves.exe` plus `Client/Binaries/Win64/Client-Win64-Shipping.exe`.

### Tier recommendation

Server-side `bundles[*].config.recommendGpu` is a ~40-entry GPU allowlist per tier. The launcher reports the machine's GPUs via `kr_get_gpu_info` → `DeviceUtils.GetGpuNames()` (`Win32_VideoController`, filtered to physical devices — `PNPDeviceID` must start with `PCI\` and the name must not contain a virtual-display keyword such as `VMware`, `VirtualBox`, `RDP`, `基本显示`). The H5 bundle then lowercases both sides and does an **exact string match**; ties are broken by `bundleRecommendPriority` (`UHD > HD > SD`), falling back to `defaultBundleName`.

Consequence for non-Windows/VM/GPU-rename setups: the match is exact and case-insensitive but not fuzzy, so an unrecognized GPU yields no recommendation and the launcher silently keeps the default `HD`.

### Runtime graphics toggles (a different axis from the tier)

- `RHIOptionList` is the launcher's per-launch graphics switch. It currently carries one visible entry: `-dx11` ("Launch with DirectX 11 (in case of game anomalies)"). An entry with `isShow: 0` and `cmdOption: ""` acts as the hidden default, so nothing is appended unless the user opts in.
- `commandList` carries `{"id": "f56ff5d76e", "cmd": "-slno", "default": 0}` — "Disable DLSS".

Both are stored under the game cache dir (`kr_game_cache/<gameId>/`), as `RHI_setting.json`, `RHI_cache.json`, `Extend_command_cache.json`, `Extend_command_preferences.json`.

## Exact launch command

`KRResUpdateModule.StartGameProcess()` is the whole story:

```csharp
string rHICommand = GetRHICommand();                       // "" or "-dx11"
List<string> list = new();
if (!string.IsNullOrEmpty(rHICommand)) list.Add(rHICommand);
list.AddRange(KRBundleLaunchArgsResolver.Resolve(gameId, bundleName));  // "-krqlv=<tier>"
list.AddRange(ExtractCommandOption());                     // "-slno" + forwarded launcher args
new KRGameProcess(gameDirPath, gameExeName, restartingLockFilePath, list, list2.Count, ...)
```

`ExtractCommandOption()` appends `KRExtendCommandManager.ExtractAvailableExtendCommandSet()` and then `KRLauncherArgUtils.GetLauncherArgs(excludeInnerArgs: true)` — **every launcher CLI argument is forwarded to the game verbatim**, except the pairs `--LauncherRootDir <v>` and `--updateFrom <v>`.

`KRGameProcess.Start()` runs:

```csharp
new ProcessStartInfo(PathUtils.Combine(GameDir, GameExe), Arguments)   // ArgumentList → per-arg quoting
```

`GameExe` is `gameExeName` from `KRApp.conf` = `Wuthering Waves.exe` — the **root** executable, not `Client/Binaries/Win64/Client-Win64-Shipping.exe`. `WorkingDirectory` is never set, so the child inherits the launcher's CWD.

Resulting command lines, per tier, with no optional toggles:

```text
"<install>\Wuthering Waves.exe" -krqlv=sd
"<install>\Wuthering Waves.exe" -krqlv=hd
"<install>\Wuthering Waves.exe" -krqlv=uhd
```

with DX11 and/or DLSS-off enabled the first argument becomes `-dx11` (and/or `-slno` is appended after the tier argument).

## Game-side consumer of `-krqlv`

The installer ships no game binaries, but the local Steam install of Wuthering Waves 3.7 does. `Client/Binaries/Win64/Client-Win64-Shipping.exe` (169 MB) contains the consumer, as one contiguous UTF-16 literal pool at file offsets `0x7b0e710`–`0x7b0f770` in the first data section (`-krqlv=` itself is at file offset `0x7b0e8b8`, VA `0x147b0fab8`):

```text
LogKuroPak
KuroExtMount
InitExtPakPlatform() invoke FKuroPakPlatform::StartupMount().
InitExtPakPlatform() invoke FKuroPakPlatform::StartupMount() finished.
launch not by commandlet, use kuro quality!
-krqlv=
%sHD/
%sSD/
UHD
%sUHD/
kuro: Use launcher to start game!
kuro auto mount dir %s
read mount manifest file %s
PakName is empty or MountOrder(%d) <= 0
Can't find the file: DefaultKuroApp.ini
KRLauncherGameRestarting.lock
PAK file (%s) mounted failed.  /  mouted PAK file (%s).
The Sha1 of file(%s) is %s, Target Sha1 is %s
```

Reading the block as a pipeline:

1. `FKuroPakPlatform::StartupMount()` runs during `InitExtPakPlatform()`, next to `UKuroPakMountStatic` / `UKuroPakKeyLibrary` — i.e. before normal pak mounting.
2. It scans the command line for `-krqlv=` and derives a tier directory from the three `printf` format strings `%sHD/`, `%sSD/`, `%sUHD/` (with a bare `UHD` token alongside them, consistent with a `== TEXT("UHD")` comparison).
3. If it is not running as a commandlet and no `-krqlv` was supplied, it raises the fatal `kuro: Use launcher to start game!` — the exact string seen in the no-argument Proton run.
4. It then auto-mounts the paks under that tier directory (`kuro auto mount dir %s`), driven by a mount manifest (`read mount manifest file %s`, `MountOrder(%d)`), and verifies each pak against a SHA-1 (`KuroPakKey`, `RSAPubKey`).

This closes the loop. `-krqlv=<tier>` is not a hint: it selects `Client/Content/<tier>/`, the directory the launcher installs from `bundles.<tier>.resourcePacks`.

Cross-check on the local install: only `Client/Content/HD/` (50 `.pak` + 50 `.sig`, 43 GB) and `Client/Content/Paks/` (55 `.pak`, 37 GB) exist — exactly the `common` + `hd` pack pair, matching the manifest sizes (42.57 GiB / 37.41 GiB) and the shipped `defaultBundleName: "HD"`. There is no `SD/` or `UHD/` directory, so `-krqlv=sd` / `-krqlv=uhd` would have nothing to mount.

Two further cross-checks against the local install:

- `Client/Config/` holds exactly the 13 files listed in the `common` pack manifest (`AMD.json`, `DGFX_*.json`, `IGFX_*.json`, `NVIDIA.json`, `RTX.json`, `SpatialData/*`, `Kuro/*`, `uninstall.ini`) plus one extra, `Client/Config/Kuro/KuroConfigMonitor.hash`, which the manifest does not list — it is generated at runtime. The manifest therefore describes the shipped state, not the live tree.
- `Client/Saved/SaveGames/KURO_PLAYER_PREFS.sav` is a UE 4.26 GVAS save for `/Script/KuroRenderingRuntimeBPPlugin.KuroSaveGame` whose `StringMap` holds only `LoginDeviceId`. There is **no persisted quality/tier value** anywhere in it: the tier is not a saved graphics setting, it is re-derived from the command line on every launch. That is why the launcher must pass it every time.

Limits of this evidence:

- The **code** sections of `Client-Win64-Shipping.exe` are encrypted at rest (entropy 8.000 bits/byte across sections 0 and 3; the entry point does not disassemble). Only the data sections are plaintext, so the strings above are readable but their *code xrefs are not recoverable statically*. No instruction-level disassembly of `StartupMount()` was performed.
- The mapping from the `-krqlv` value to `%sHD/`/`%sSD/`/`%sUHD/` is reconstructed from string adjacency in one literal pool, plus the filename correspondence between `-krqlv=hd` and `Client/Content/HD/`. It is very strong but not instruction-proven.
- `Client/Saved/Logs/Client.log` is written obfuscated (byte-shifted), so no runtime log line confirms the chosen mount directory.
- A RIP-relative-`lea` cross-reference sweep over every CODE section (indices 0, 11 and 12 of 15) returns zero hits for all six strings. That is expected given the point above: the code is unpacked at runtime, so instruction-level analysis needs runtime dumping, which was not attempted here.

### Constraint this puts on WuwaID's patch

The same pool shows Shipping SHA-1-verifies game files against an RSA-signed key table (`UKuroPakKeyLibrary`, `/Script/KuroPakKey`, `RSAPubKey is empty!`, `File(%s) Sha1: %s is not match %s, modify time: %s`), with tunable worker counts via `kuro.Sha1MultiThread` etc. On the launcher side, Kuro's server config whitelists exactly which files may legitimately differ from the manifest: `experiment.res_check.fileCheckWhiteListConfig = Client/Config/RTX.json : Engine/Plugins/Runtime/Nvidia/Streamline/Binaries/ThirdParty/Win64/sl.pcl.dll : Engine/Extras/Redist/en-us/UE4PrereqSetup_x64.exe`. Nothing under `Client/Content/` is whitelisted. **Observed constraint, not a verified failure**: an in-place edit to a `.pak` or `.sig` under `Client/Content/Paks` or `Client/Content/<TIER>` is not covered by that whitelist, so it is a candidate for tripping Kuro's own res-check or SHA-1 verification. Whether WuwaID's patched asset file actually lands in the SHA-1-verified set is unproven — verifying that needs the runtime manifest the game reads, which lives in the encrypted-at-rest code path.

## What is established, and what is not

Established by this RE:

- The three modes are launcher **bundles**; each bundle is a resource-pack list plus one `launchArgs` entry.
- `bundles.<tier>.resourcePacks` = `common` + `<tier>`, materialised as `Client/Content/{SD,HD,UHD}/pakchunk<N>-<TIER>-WindowsNoEditor.pak` with identical chunk numbering across tiers.
- The official launch command is `"<install>\Wuthering Waves.exe" -krqlv=<tier>` (root exe, not Shipping).
- `Client-Win64-Shipping.exe` builds the pak mount root from the `-krqlv` value — the literal `-krqlv=` is immediately followed by `%sHD/`, `%sSD/`, `%sUHD/` and then the `kuro: Use launcher to start game!` fatal, inside the `FKuroPakPlatform::StartupMount()` string block. The mount target is `Client/Content/<tier>/`; a missing tier value has nowhere to mount and aborts.
- Tier and the `-dx11` / `-slno` toggles are independent mechanisms.

Not established:

- The exact instruction-level parse of `-krqlv` (code sections are encrypted at rest).
- Whether `FKuroPakPlatform` mounts from `FPaths::ProjectContentDir() + <tier>/` or from an absolute path — the `%s` prefix is unproven.
- The behaviour for an unrecognised `-krqlv` value, and whether `HD` is an explicit fallback or simply the first branch tested.
- The internal reason the guard exists (launcher attestation vs. anti-tamper). The most that can be said is that the string sits in the same block as the SHA-1 pak verification used by `KuroPakKey`.

## Launch contract, read out of the root bootstrap

`Wuthering Waves.exe` is not packed, so it disassembles. Its strings and the code around them show:

- It resolves Shipping from **its own module path**, not the working directory: `GetModuleFileNameW` → `PathRemoveFileSpecW` → `PathCombineW` with `Client`, then `Binaries\Win64`, then `Client-Win64-Shipping.exe`, followed by `PathFileExistsW` on `<exedir>\Client`. The same base is used for `Saved\KRLauncherGameRestarting.lock`, i.e. `<exedir>\Client\Saved\...` — which matches Kuro's `gameRestartingLockFilePath: "Client/Saved/KRLauncherGameRestarting.lock"`.
- The child command line uses the format `"%s\%s" %s %s` (a quoted `<dir>\<name>` plus two arguments). The last argument is the accumulated `argv[1..]` tail, built by a loop that concatenates every argument after `argv[0]` with spaces, so every argument WuwaID passes — `-dx11` and `-krqlv` — reaches Shipping. The binary imports `CreateProcessW`.
- Its manifest requests `requireAdministrator`, so a direct spawn fails with `ERROR_ELEVATION_REQUIRED` (740); WuwaID already classifies that and retries through `runas`. `Client-Win64-Shipping.exe` carries the same `requestedExecutionLevel level="requireAdministrator"`, so the UAC prompt is pre-existing and not a consequence of switching to the root bootstrap — `launch_game_elevated*` was already on the launch path.

Consequences: the working directory is not part of the contract, and every argument survives the handoff.

## What this branch implements

1. `engine/path.rs` keeps `GAME_EXE_RELATIVE` (Shipping) for install validation, process detection and force-quit ownership, and adds `LAUNCH_EXE_RELATIVE` (`Wuthering Waves.exe`) used only by the launch path.
2. `build_launch_command` starts the root bootstrap with `working_directory` set to the game root and appends `-krqlv=<tier>`. `wait_for_launcher_process_tree` already tolerates the root exiting first and still watches for Shipping, so the monitor needed no change. The command is built and the tier resolved **once** per launch: `launch_prebuilt` spawns that exact instance, and the log line reads the tier back off `command.quality_level()` so what is logged cannot differ from what is passed.
3. The tier comes from `installed_quality_levels()`, which reports only tiers that have paks under `Client/Content/<TIER>/`. The preference is a new `qualityLevel` setting (`auto` | `SD` | `HD` | `UHD`): an explicit tier wins when it is installed, otherwise the most complete installed tier wins, otherwise Kuro's `defaultBundleName` (`HD`). Ties on total pack bytes resolve to the *higher-preferred* tier — `max_by_key` yields the last maximum, so the iterator is reversed deliberately.
4. The argument is lowercased (`-krqlv=hd`) even though the directory it selects is uppercase (`Client/Content/HD/`), matching what Kuro's launcher and Steam send.
5. The settings dialog offers only the detected tiers, so a flag/directory mismatch cannot be configured. The OTOMATIS card is always rendered alongside them, because with a single installed tier it is otherwise unreachable and the user could never return to the default. A stored tier that is no longer installed is coerced back to `auto`. `detect_quality_levels` is a new Tauri command backing that list.
6. `-dx11` is the only remaining optional toggle and stays independent of the tier; it continues to map onto Kuro's `RHI_setting.json` option.
7. WuwaID does not replicate Kuro's Authenticode thumbprint check (`ProcessUtils.VerifyFileCertThumbsPrint`, `KRResUpdateModule.cs:613` against `abd0851f…`, `78db074a…`). It is Kuro's own launcher-attestation step, not part of the game-side contract.
8. `detect_quality_levels` had to be added to `commands.allow` in `src-tauri/permissions/app-commands.toml`. Tauri v2 rejects any `invoke` for a command missing from that list, and the UI swallows the rejection, so a missing entry silently renders an empty tier list rather than an error.
9. Verification status: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test` (242 tests), the frontend gate (`281 FILES 0 ERRORS`), the production build, `npm run test:lifecycle` (25), `npm run test:patch-status` (5) and `npm run test:version` (1) all pass. The `#![cfg(windows)]` paths — `launch_game_elevated*`, `tests/game_lifecycle_windows_tests.rs` and the `wut_game_lifecycle_fixture` bin — are **executed by CI on `windows-latest`**. PR #15's Windows job ran `direct_launch_without_dx11_handoffs_and_force_quits_verified_tree`, `direct_launch_with_dx11_handoffs_and_force_quits_verified_tree`, `elevated_uac_launch_handoffs_and_force_quits_with_retained_handle`, `external_instance_is_not_claimed_or_killed_by_unrelated_launcher_tree` and `launcher_child_handoff_stays_owned_and_force_quit_cleans_the_tree` isolated, all green, with no failing test in the job. That is evidence against the test fixture binary, not against a real 3.7 installation. Before that they were only compile-verified locally, via `cargo xwin check --target x86_64-pc-windows-msvc --all-targets` plus `cargo xwin clippy` for that target; plain `cargo check --target x86_64-pc-windows-msvc` cannot work in the authoring environment because `lib.exe` is absent. `cargo-xwin` is the same tool the repo's own `npm run launcher-build:msvc` script uses. Launch-command, working-directory, tier-argument, crash-classification and evidence-serialisation behaviour all have platform-neutral coverage in `engine/runtime.rs`.
10. Because the root is the bootstrap, the game is only ever observed as a *separate* process that appears and then disappears, so `handoff_observed` carries the entire exit decision and the bootstrap's exit code carries none of it. `classify_game_exit` therefore returns one of two outcomes: `NotStarted` when Shipping never appeared — reported as `SpawnFailureKind::ProcessNotDetected` with the status `not_started`, never as a normal finish — and `Finished` when it did appear. `ProcessNotDetected` existed in the enum but was constructed nowhere; this is its first use. `handoff_observed` is also threaded onto `LaunchEvidence` (with `#[serde(default)]`, so evidence written before the field still loads) so the persisted diagnostics file can tell "the game ran and exited" from "the bootstrap left without starting it". The residual loss is stated plainly: after a handoff the launcher cannot distinguish a crashed game from a clean one, because WuwaID never owned the Shipping process and so has no handle from which to read its exit status. Recovering that needs runtime exit capture, which is out of scope here.
11. Of the three Windows-only acceptance runners, only `run-windows-fixture-performance.ps1` actually drives a launch — it writes `settings.json` with `gamePath`, starts `WuwaIDLauncher.exe` and clicks launch, so `New-FixtureGame` had to grow the root bootstrap and a `Client/Content/HD/pakchunk1-HD-WindowsNoEditor.pak` or every scenario would fail with `executable_missing`. `windows-release-gate.ps1` never starts the app; it only builds and snapshots the fixture tree, so its change is consistency (its `fixture-layout.json` evidence list would otherwise describe a tree that no longer matches a real install). `run-windows-real-acceptance.ps1` runs against a real installation, which already has the bootstrap, so its change is a precondition guard. All three still keep the `Client-Win64-Shipping.exe` reference, because process monitoring and force-quit correctly continue to key on Shipping.
