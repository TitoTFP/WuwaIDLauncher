# Method Numbering and Default Selection

## Goal

Make Resource Mount the user-facing **Metode 1** and the default installation
method, while making the existing signature-bypass installation the user-facing
**Metode 3**. Keep Metode 2 unchanged.

## Compatibility decision

The existing internal identifiers remain semantic and unchanged:

- `method3` continues to mean Resource Mount.
- `method2` continues to mean the `winhttp.dll` manual loader.
- `method1` continues to mean PAK plus signature bypass.

Only the user-facing numbering and default fallback change. This preserves
existing settings and `versions.json` entries, so an installed launcher does
not reinterpret a user's previous method selection after upgrading.

## Behavior

| User-facing method | Internal ID | Behavior | Default |
| --- | --- | --- | --- |
| Metode 1 | `method3` | Resource Mount, no signature bypass | Yes |
| Metode 2 | `method2` | `winhttp.dll` manual loader | No |
| Metode 3 | `method1` | Canonical PAK plus signature bypass | No |

When no saved setting exists, or a saved setting cannot be read, all native and
web UI defaults use internal `method3`. Existing saved internal IDs are read as
before and therefore retain their original behavior.

## Changes

1. Change native fallback defaults in `MainWindow` and the active-player
   heartbeat service from `method1` to `method3`.
2. Change web UI state and fallback values to `method3`.
3. Swap the method menu's user-facing labels/descriptions and `data-method`
   values so Resource Mount is Metode 1 and signature bypass is Metode 3.
4. Make selection toasts use the new user-facing names.
5. Update README, CONTEXT, and relevant test comments/assertions to describe
   the new numbering.
6. Leave `InstallMethods` normalization, cache keys, asset paths, and the
   internal method IDs unchanged.

## Data flow

The UI stores an internal ID in the existing settings file. Native code
normalizes that ID and routes it to the existing installation/status logic.
Only the UI label-to-ID mapping changes:

```text
Metode 1 (Resource Mount) -> method3 -> existing Resource Mount flow
Metode 2 (manual loader)  -> method2 -> existing manual-loader flow
Metode 3 (signature bypass)-> method1 -> existing signature-bypass flow
```

## Error handling

No new error paths are introduced. Existing normalization continues to fall
back to its current canonical internal ID for invalid explicit values; only
call sites that mean “no selection/default” switch to `method3`.

## Verification

- Add/adjust focused tests proving the new default resolves to Resource Mount
  and that the three user-facing mappings route to the intended internal IDs.
- Run the focused .NET tests first, then the complete test suite and the static
  consistency check.
- Confirm no tracked files outside the intended source, web, documentation, and
  test files change.
