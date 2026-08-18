# WUT-38 Launch Evidence and Elevation Design

## Context

The release launcher was reproduced against `F:\\Wuthering Waves`. The current
`Command::spawn()` path returns Windows error 740 (`The requested operation
requires elevation`) without showing a UAC prompt. Launching the same game with
Windows `runas` succeeds after elevation. The current monitor also discards
child output, exit status, and useful game-log context, so immediate exits are
reported as an empty generic failure.

## Goals

- Show the standard Windows UAC prompt when the game requires elevation.
- Distinguish normal spawn failure, cancelled elevation, immediate exit, and a
  later non-zero process exit.
- Record PID, command, launch mode, timestamps, exit code, output tails, and
  relevant game-log tails in a local evidence JSON file.
- Restore signature-bypass state and return the launcher window to a usable
  state on every failed or completed lifecycle path.
- Keep the existing command/API boundary and avoid changing the game itself.

## Non-goals

- Automatically accepting or suppressing UAC.
- Uploading launch evidence without an explicit user action.
- Replacing the existing game process discovery or force-quit behavior.
- Treating a successful process start as proof that the game reached its full
  title screen.

## Design

`engine::runtime` will expose a small process wrapper around either a normal
Rust child process or a Windows `ShellExecuteExW` process handle. It will first
try the existing direct spawn path with piped stdout/stderr. On Windows error
740 it retries with the `runas` verb and `SEE_MASK_NOCLOSEPROCESS`, which makes
the UAC prompt visible and still gives the launcher a PID and waitable handle.
Cancellation is reported as a distinct elevation-cancelled failure.

The wrapper returns a structured process result containing PID, exit code, and
captured output tails. The launch command owns the lifecycle timeline: it
records the command before spawning, waits for process detection, marks the
runtime active only after detection, and treats an exit before detection as an
immediate-exit failure. Signature restoration and UI reset happen in one
cleanup path for spawn failure, elevation cancellation, early exit, crash, and
normal completion.

Each attempt writes `Diagnostics/launch-*.json` under the launcher app-data
directory. The JSON contains command metadata, launch mode, PID, phase
timestamps, exit code, stderr/stdout tails, game-log tail, and the evidence path
itself. User-facing `onLaunchError` text includes the failure kind and the
non-empty diagnostic fields, including the evidence path.

## Testing

- Unit tests cover error-740 classification, UAC cancellation classification,
  command argument construction, bounded output/log tails, and evidence
  serialization.
- Existing full Rust and frontend suites remain green.
- Release verification repeats the published/release launch against the game
  copy, with the user approving UAC manually when Windows presents it.
