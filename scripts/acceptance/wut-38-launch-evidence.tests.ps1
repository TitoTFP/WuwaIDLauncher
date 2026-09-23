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
    "fn remove_saved_launch_diagnostics",
    "remove_dir_all(diagnostics_dir)",
    "remove_saved_launch_diagnostics(&get_appdata_dir())",
    "wait_for_launcher_process_tree",
    "PROCESS_HANDOFF_GRACE",
    "onLaunchError",
    "onGameLaunchFinished",
    "finish_launch_lifecycle",
    "exit_code",
    "game_log_tail"
)
$removedPersistenceMarkers = @(
    "fn save_launch_evidence",
    "fn launch_error_message",
    "serde_json::to_vec_pretty(&evidence)"
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
foreach ($marker in $removedPersistenceMarkers) {
    if ($lib -match [regex]::Escape($marker)) {
        throw "WUT-38 removed diagnostics persistence marker remains: $marker"
    }
}

Write-Output "PASS: WUT-38 launch lifecycle and diagnostics cleanup contract"
