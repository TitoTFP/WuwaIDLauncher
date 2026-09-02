$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$topBar = Get-Content -Raw -LiteralPath (Join-Path $root "src\components\TopBar.svelte")
$app = Get-Content -Raw -LiteralPath (Join-Path $root "src\App.svelte")
$state = Get-Content -Raw -LiteralPath (Join-Path $root "src\lib\launcherState.svelte.ts")
$types = Get-Content -Raw -LiteralPath (Join-Path $root "src\lib\types.ts")
$modal = Get-Content -Raw -LiteralPath (Join-Path $root "src\components\UpdateModal.svelte")
$rightPanel = Get-Content -Raw -LiteralPath (Join-Path $root "src\components\RightPanel.svelte")
$bridge = Get-Content -Raw -LiteralPath (Join-Path $root "src\lib\bridge.ts")

if ($topBar -match '>\s*PERFORMA\s*<' -or $topBar -match 'HOME|TENTANG|LOGS|ABOUT') {
    throw "Top navigation still exposes a removed page"
}
if ($topBar -notmatch 'settingsTrigger' -or $topBar -notmatch 'aria-haspopup="dialog"') {
    throw "Settings dialog trigger is missing from the top bar"
}
foreach ($removedPath in @('src\components\SettingsPanel.svelte', 'src\components\AboutPanel.svelte')) {
    if (Test-Path (Join-Path $root $removedPath)) {
        throw "Removed page component still exists: $removedPath"
    }
}
foreach ($removed in @('LauncherPage', 'appState.page', 'config.autoCheckUpdate', 'this.config.autoCheckUpdate')) {
    if (($app + $topBar + $state + $types) -match [regex]::Escape($removed)) {
        throw "Removed launcher surface is still referenced: $removed"
    }
}
foreach ($required in @('ToastHost', 'adminModal')) {
    if ($app -notmatch [regex]::Escape($required)) {
        throw "App shell is missing main UI element: $required"
    }
}
foreach ($removed in @('PerformancePanel', 'page === ''performance''', 'getPerformanceConfigActive', 'applyPerformanceConfig', 'clearPerformanceConfig')) {
    if (($app + $topBar) -match [regex]::Escape($removed)) {
        throw "Removed performance feature is still referenced: $removed"
    }
}
if ($modal -match 'Versi \{version\}') {
    throw "Update modal still adds the legacy Versi prefix"
}
if ($rightPanel -match 'progressPercent\s*>\s*0' -or $rightPanel -notmatch '#if appState\.installing') {
    throw "Progress UI is not restricted to an active installation"
}
foreach ($required in @('handleSupport', 'bridge.openSupport', 'menuDukung')) {
    if ($rightPanel -notmatch [regex]::Escape($required)) {
        throw "Main menu action is missing: $required"
    }
}
if ($bridge -notmatch 'onLauncherUpdateStatus') {
    throw "Launcher update status event bridge is missing"
}

Write-Output "PASS: main UI removal contract"
