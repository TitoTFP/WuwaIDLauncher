$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "WUT game lifecycle smoke requires Windows process APIs."
}

$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$runtimePath = Join-Path $root "src-tauri\src\engine\runtime.rs"
$libPath = Join-Path $root "src-tauri\src\lib.rs"
$runtime = Get-Content -Raw -LiteralPath $runtimePath
$lib = Get-Content -Raw -LiteralPath $libPath

foreach ($required in @(
    "fn spawn_elevated",
    "LaunchMode::Elevated",
    "find_launcher_game_process_id",
    "process_tree_contains",
    "has_process_tree_descendant",
    "terminate_verified_game_tree",
    "duplicate_termination_handle",
    "force_quit_game_with_identity",
    "PROCESS_HANDOFF_GRACE"
)) {
    if (($runtime + $lib) -notmatch [regex]::Escape($required)) {
        throw "Lifecycle implementation is missing required contract: $required"
    }
}

$cargoManifest = Join-Path $root "src-tauri\Cargo.toml"
$unitTests = @(
    "engine::runtime::tests::direct_process_try_wait_does_not_join_inherited_pipes",
    "engine::runtime::tests::launch_command_contains_working_directory_and_dx11_argument",
    "engine::runtime::tests::process_tree_matching_requires_verified_ancestry",
    "engine::runtime::tests::owned_runtime_state_survives_child_handoff_without_claiming_external_games",
    "engine::runtime::tests::spawn_error_classification_distinguishes_elevation_and_cancel",
    "tests::window_minimize_action_distinguishes_normal_and_tray_modes",
    "tests::launcher_game_minimize_stays_in_tray_after_restore",
    "tests::external_game_never_emits_launcher_exit_notice_or_restores_tray",
    "tests::force_quit_exit_restores_launcher_lifecycle"
)
foreach ($testName in $unitTests) {
    & cargo test --locked --manifest-path $cargoManifest --lib $testName -- --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "Launcher lifecycle unit test failed: $testName"
    }
}

& cargo test --locked --manifest-path $cargoManifest --test milestone3_contract_tests -- --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "Runtime ownership contract tests failed with exit code $LASTEXITCODE."
}

& cargo build --locked --manifest-path (Join-Path $root "src-tauri\Cargo.toml") --bin wut-game-lifecycle-fixture
if ($LASTEXITCODE -ne 0) {
    throw "Windows lifecycle fixture build failed with exit code $LASTEXITCODE."
}

& cargo test --locked --manifest-path (Join-Path $root "src-tauri\Cargo.toml") --test game_lifecycle_windows_tests -- --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "Windows process-tree lifecycle integration tests failed with exit code $LASTEXITCODE."
}

Write-Output "PASS: direct/elevated UAC, DX11 on/off, launcher handoff, external isolation, tray contract tests, and verified force quit"
