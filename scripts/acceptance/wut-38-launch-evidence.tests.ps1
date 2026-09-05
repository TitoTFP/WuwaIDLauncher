$ErrorActionPreference = "Stop"

$runtimePath = Join-Path $PSScriptRoot "..\..\src-tauri\src\engine\runtime.rs"
$libPath = Join-Path $PSScriptRoot "..\..\src-tauri\src\lib.rs"

foreach ($path in @($runtimePath, $libPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "WUT-38 source file not found: $path"
    }
}

$runtime = Get-Content -Raw -LiteralPath $runtimePath
$lib = Get-Content -Raw -LiteralPath $libPath

$runtimeMarkers = @(
    "pub struct LaunchEvidence",
    "pub enum SpawnFailureKind",
    "ShellExecuteExW",
    "SEE_MASK_NOCLOSEPROCESS",
    "bounded_output_tail",
    "pub fn collect_game_log_tail",
    "pub fn classify_spawn_error"
)
$libMarkers = @(
    "fn save_launch_evidence",
    "Diagnostics",
    "wait_for_launcher_process_tree",
    "PROCESS_HANDOFF_GRACE",
    "onLaunchError",
    "onGameLaunchFinished",
    "finish_launch_lifecycle",
    "exit_code",
    "game_log_tail"
)

foreach ($marker in $runtimeMarkers) {
    if ($runtime -notmatch [regex]::Escape($marker)) {
        throw "WUT-38 runtime marker missing: $marker"
    }
}
foreach ($marker in $libMarkers) {
    if ($lib -notmatch [regex]::Escape($marker)) {
        throw "WUT-38 lifecycle marker missing: $marker"
    }
}

Write-Output "PASS: WUT-38 launch evidence contract"
