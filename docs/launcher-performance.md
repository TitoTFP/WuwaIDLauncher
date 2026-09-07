# Launcher Performance Matrix

This document defines the deterministic Windows performance gate for the launcher.
It measures the launcher and WebView2 in two runtime states without requiring a
real Wuthering Waves installation.

## States

### Visible foreground

The release launcher starts with no game process. The test waits for the main
window and an enabled **Mainkan Game** button, then samples the visible steady
state.

### System tray

The runner creates a disposable fixture game directory and copies
`wut-game-lifecycle-fixture.exe` to the expected
`Client/Binaries/Win64/Client-Win64-Shipping.exe` path. The launcher starts that
fixture through its normal UI flow, enters the tray, and samples the hidden
steady state. The fixture lifetime is controlled by
`WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS`.

After the tray sample, the fixture process tree is terminated and the runner
measures the time until the launcher window is visible again.

## Measurements and thresholds

The defaults are deliberately broad enough for a shared GitHub-hosted runner,
while still catching a runaway process or a broken lifecycle transition.

| Metric | Gate |
| --- | ---: |
| Launcher CPU, every sample | ≤ 10% |
| WebView2 CPU, every sample | ≤ 10% |
| Launcher private memory | ≤ 512 MB |
| Launcher working set | ≤ 512 MB |
| WebView2 private memory (summed) | ≤ 1024 MB |
| WebView2 working set (summed) | ≤ 1024 MB |
| Launcher private-memory growth per state sample | ≤ 32 MB |
| Launcher read I/O | ≤ 8 MiB/s |
| Launcher write I/O | ≤ 4 MiB/s |
| WebView2 read I/O | ≤ 16 MiB/s |
| WebView2 write I/O | ≤ 8 MiB/s |
| Sampling cadence jitter | ≤ 1000 ms |
| Startup to main window | ≤ 60 s |
| Launcher ready after window | ≤ 120 s |
| Visible-to-tray transition | ≤ 60 s |
| Tray-to-visible restore | ≤ 60 s |

The raw sampler records CPU P95 as well as the enforced maximum. Every sample
must include a WebView2 process and the expected window visibility.

Default sampling is 20 seconds for visible foreground and 30 seconds for tray,
with a two-second interval. The matrix emits enough samples to detect a broken
steady state without making CI unnecessarily long.

## Reproduction

Run on Windows with an interactive desktop, PowerShell 7, WebView2, and the
release launcher build:

```powershell
npm run build
pwsh -NoProfile -File scripts/acceptance/run-windows-fixture-performance.ps1 `
  -OutputRoot .\performance-evidence
```

The runner builds `wut-game-lifecycle-fixture.exe` automatically when it is not
already present. It uses isolated fixture files and an isolated local-app-data
folder, and cleans up the launcher and fixture process tree in `finally`.

Evidence files include:

- `summary.json` — scenario metrics, latency measurements, thresholds, and status;
- `resource-visible.csv` and `resource-tray.csv` — raw per-sample data;
- `resource-visible.log` and `resource-tray.log` — sampler output.

## GitHub Actions

The matrix runs in a dedicated GitHub-hosted `windows-latest` Windows fixture
performance job. The workflow uploads `performance-evidence` as a retained
artifact even when the separate Windows regression job fails. No self-hosted
runner is permitted by the workflow contract.

The hosted run still needs to create a real Tauri window and WebView2 process;
the test must fail rather than silently switching to a headless or synthetic
measurement if that desktop capability is unavailable.

## Boundaries and limitations

- The fixture is not the real game and does not prove real-game rendering,
  patch compatibility, GPU behavior, or game CPU/resource usage.
- Real game acceptance remains manual and is not replaced by this matrix.
- UAC, administrator elevation, self-update restart, and other destructive or
  interactive flows remain manual or existing contract coverage.
- Other operation states such as install, media sync, and download throughput
  are outside this two-state performance matrix.
- Results are runner-specific observations, not a universal hardware
  benchmark. Compare artifacts from equivalent runner images before changing
  thresholds.
