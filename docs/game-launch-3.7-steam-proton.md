# Wuthering Waves 3.7 launch-path investigation

Date: 2026-10-01  
Status: Investigation recorded; no launcher code changed.

## Scope

Record the observed Steam launch behavior for the local Wuthering Waves 3.7 installation and compare it with WuwaID Launcher's current executable selection and process monitoring. This is evidence for a proposed launch-path change, not a claim that the fatal error's exact internal guard has been reverse-engineered.

## Local Steam/Proton evidence

The local Steam installation identifies Wuthering Waves as AppID `3513350`; its Proton prefix is under `steamapps/compatdata/3513350/pfx`. Steam's `gameprocess_log.txt` records multiple launches on 2026-09-30 and 2026-10-01 with this command shape:

```text
.../proton-cachyos-slr/proton waitforexitandrun \
  "<game-root>/Wuthering Waves.exe" -krqlv=hd
```

The inspected game root contains the x86-64 Windows executable `Wuthering Waves.exe`. It also contains `Client/Binaries/Win64/Client-Win64-Shipping.exe`. The exact file `Client/Binaries/Win64/BootstrapPackagedGame-Win64-Shipping.exe` was not present. Local resource files include `Client/Resources/3.7.0`.

This establishes that Steam selected the root `Wuthering Waves.exe` and passed `-krqlv=hd` while running it through the configured Proton tool. It does **not** establish that the resource-tier argument is mandatory, that it alone fixes the reported fatal error, or what current working directory Steam used. Proton is the compatibility runtime in this observed command; the log does not identify Proton as the cause of the error.

## Current WuwaID launch path

- `src-tauri/src/engine/path.rs` defines `GAME_EXE_RELATIVE` as `Client/Binaries/Win64/Client-Win64-Shipping.exe` (with platform-specific separators). Path validation uses this executable to recognize a game installation.
- `src-tauri/src/engine/runtime.rs` builds the launch command from `LAUNCH_EXE_RELATIVE`, the root bootstrap, and appends `-krqlv=<tier>`; `-dx11` is the only remaining optional toggle. `spawn_direct()` invokes that executable directly with `Command::new`.
- `src-tauri/src/lib.rs` passes the Shipping path as the expected game executable to the launch monitor.

The reported `kuro: Use launcher to start game!` error is consistent with WuwaID starting Shipping directly instead of using the entry point observed in the Steam log. The local Steam evidence supports that mismatch, but does not prove the internal reason the game rejects the direct launch.

## Process-monitor implications

The existing Windows monitor keeps the game executable distinct in purpose from the launched process: after the root launched process exits, `wait_for_launcher_process_tree()` searches its process tree for the expected Shipping executable. This can support a root-entry-point-to-Shipping handoff if `Wuthering Waves.exe` actually creates Shipping as a descendant and exits. That process relationship and lifecycle still need verification on Windows before treating the handoff as proven.

Therefore, a Windows launch-path change should use a separate launch-executable path for the root `Wuthering Waves.exe`; it should not globally change `GAME_EXE_RELATIVE`, which remains needed for installation validation, process detection, and force-quit ownership. The observed Steam resource argument is `-krqlv=hd`; whether WuwaID should hard-code it or expose a tier setting is a separate product decision. The Steam log does not provide evidence for changing the working directory.

## Linux/Proton boundary

WuwaID's current `Command::new` path does not itself invoke Steam or configure Proton. In addition, runtime process inspection and process-tree ownership are implemented under Windows-specific code paths; the non-Windows process inspection functions return no detected process. Thus, replacing Shipping with the root `.exe` alone would not provide complete native-Linux/Proton support. A Steam/AppID launch route or a fully configured Proton invocation would also need compatible process detection and lifecycle handling.

## Matched Proton launch comparison

The no-argument run manually launched the root `Wuthering Waves.exe` through the installed Proton CachyOS SLR, reusing `compatdata/3513350` and the Steam client/AppID environment but omitting `-krqlv=hd`. It displayed the fatal dialog `Fatal error: [File:Unknown] [Line: 54] kuro: Use launcher to start game!`, as shown in the user's screenshot.

Repeating the same Steam Linux Runtime/Proton command with the same prefix and environment, changing only by adding `-krqlv=hd`, reached the game's login screen. The user's screenshot showed the 3.7 build with `* HD` and `Login Status: 0`. Paired with the no-argument failure, this strongly indicates the flag matters for this local direct-Proton launch path.

The comparison did not use Steam's client-managed AppID launch and stopped at the login screen; it does not establish further gameplay behavior. Proton output after the flagged launch also contained an unimplemented `ntoskrnl.exe.PsGetProcessExitStatus` message. At the user's request, the launch test was stopped once the login screen appeared; no game process remained. Steam's saved LaunchOptions were unchanged. The no-argument failure left `Client/Saved/Logs/Client.log` at zero bytes; the existing backup log was left untouched.

## Launcher-side and game-side reverse engineering

Reverse engineering the official installer (`reverse-engineer/WutheringWaves_overseas_setup_3.0.1.0.exe`) and the locally installed 3.7 client settles the launch contract this investigation had been inferring. Full findings: [`wuwa-quality-bundle-pipeline.md`](wuwa-quality-bundle-pipeline.md).

The launcher's `KRResUpdateModule.StartGameProcess()` starts `Wuthering Waves.exe` in the install root — never `Client-Win64-Shipping.exe` — and appends, in order: an optional `-dx11`, the bundle argument `-krqlv=<sd|hd|uhd>` from the decrypted `Assets/KRApp.conf`, then `-slno` if DLSS was disabled, then every unrecognized launcher CLI argument forwarded verbatim. That forwarding belongs to the Kuro launcher only; nothing below the root exe re-passes anything, so any other spawner (Steam LaunchOptions included) must supply `-krqlv=<tier>` itself. The launcher sets no working directory and injects no environment variables, so the child inherits the launcher's CWD. The root exe carries the PDB name `BootstrapPackagedGame-Win64-Shipping.pdb` and is the only binary referencing `KRLauncherGameRestarting.lock`.

The tier is not cosmetic: each bundle downloads a different resource pack (`Client/Content/SD|HD|UHD/pakchunk*-<TIER>-WindowsNoEditor.pak`, 20.5 / 42.6 / 61.5 GiB) alongside the shared `common` pack, and the shipped `defaultBundleName` is `HD`. `-krqlv=hd` is therefore the default official contract, not an arbitrary flag.

`Client/Binaries/Win64/Client-Win64-Shipping.exe` carries `-krqlv=`, `%sHD/`, `%sSD/`, `%sUHD/` and the exact fatal string `kuro: Use launcher to start game!` in one adjacent literal block belonging to `FKuroPakPlatform::StartupMount()`, which logs `launch not by commandlet, use kuro quality!` and `kuro auto mount dir %s`. That is the mechanism behind the failure/success pair recorded above: without the argument the pak platform has no tier directory to mount and aborts; with `-krqlv=hd` it mounts `Client/Content/HD/`, which is the only tier directory present in the local install.

Not settled by this: the exact instruction-level parse (the Shipping code sections are encrypted at rest, only the data sections are readable), and why the guard exists. The paired Proton comparison remains the behavioural evidence.

## Implemented in WuwaIDLauncher

This branch now starts the root `Wuthering Waves.exe` instead of Shipping and always passes a `-krqlv` argument:

- `engine/path.rs` keeps `GAME_EXE_RELATIVE` (Shipping) for install validation, process detection and force-quit ownership, and adds `LAUNCH_EXE_RELATIVE` used only by the launch path.
- `build_launch_command_with_options` builds `<game>/Wuthering Waves.exe` with the game root as the working directory and appends `-krqlv=<tier>`. `wait_for_launcher_process_tree` already tolerates the root exiting first and keeps watching for Shipping, so the monitor needed no change.
- The tier is detected from disk: only tiers with paks under `Client/Content/<TIER>/` are offered. The new `qualityLevel` setting (`auto` | `SD` | `HD` | `UHD`) appears in the settings dialog; `auto` picks the most complete installed tier and falls back to Kuro's `defaultBundleName`.
- The argument is lowercased (`-krqlv=hd`) while the directory it selects is uppercase, matching what Kuro's launcher and Steam send.
- Because the root is the bootstrap, the observable exit code is the bootstrap's, not the game's, so `wait_for_launcher_process_tree` records `handoff_observed` and `classify_game_exit` decides from that signal alone. If Shipping never appeared the launch is reported as `not_started` (`ProcessNotDetected`) rather than a normal finish — previously an exit code of 0 genuinely meant the game had finished, so treating it as normal is new behaviour and wrong. If Shipping did appear, a crash cannot be told apart from a clean exit, because WuwaID never owned that process; the bootstrap's code is logged only.
- The three Windows-only acceptance runners (`run-windows-fixture-performance.ps1`, `windows-release-gate.ps1`, `run-windows-real-acceptance.ps1`) build or validate the same contract, so their fixture trees were given the root bootstrap and a `Client/Content/HD` tier pak.

## Proposed next verification

Verify on a Windows 3.7 installation that the root bootstrap starts Shipping as a descendant and that WuwaID's monitor, tray lifecycle, and force-quit behavior remain correct across the handoff.

The Windows-only paths (`launch_game_elevated*` and `tests/game_lifecycle_windows_tests.rs`) sit behind `#![cfg(windows)]`, so a green Linux `cargo test` says nothing about them. They were compiled with `cargo xwin check --target x86_64-pc-windows-msvc --all-targets` (the toolchain `npm run launcher-build:msvc` already uses, which supplies the CRT/SDK without Visual Studio) and pass `cargo xwin clippy` for that target — but they were never *executed*, because running them needs Windows. Treat them as compile-verified, behaviour-unverified. Treat Linux/Proton support as a separate scope; this change does not provide it.

