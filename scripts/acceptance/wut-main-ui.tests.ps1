$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$topBar = Get-Content -Raw -LiteralPath (Join-Path $root "src\components\TopBar.svelte")
$app = Get-Content -Raw -LiteralPath (Join-Path $root "src\App.svelte")
$modal = Get-Content -Raw -LiteralPath (Join-Path $root "src\components\UpdateModal.svelte")

if ($topBar -match '>\s*PERFORMA\s*<' -or $topBar -match 'SETTINGS|LOGS|ABOUT') {
    throw "Top navigation still exposes a removed performance or legacy page"
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

Write-Output "PASS: main UI removal contract"
