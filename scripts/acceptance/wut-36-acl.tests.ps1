$ErrorActionPreference = "Stop"

$permissionPath = Join-Path $PSScriptRoot "..\..\src-tauri\permissions\app-commands.toml"
$manifestPath = Join-Path $PSScriptRoot "..\..\src-tauri\gen\schemas\acl-manifests.json"
$requiredCommands = @(
    "is_game_running",
    "browse_game_folder",
    "save_settings",
    "load_settings"
)

if (-not (Test-Path -LiteralPath $permissionPath -PathType Leaf)) {
    throw "Permission source not found: $permissionPath"
}
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Generated ACL manifest not found: $manifestPath"
}

$permissionText = Get-Content -Raw -LiteralPath $permissionPath
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$allowedCommands = @($manifest.'__app-acl__'.permissions.'app-commands'.commands.allow)

foreach ($command in $requiredCommands) {
    if ($permissionText -notmatch ('(?m)^\s*"' + [regex]::Escape($command) + '",?\s*$')) {
        throw "ACL source is missing required command: $command"
    }
    if ($allowedCommands -notcontains $command) {
        throw "Generated ACL manifest is missing required command: $command"
    }
}

Write-Output "PASS: WUT-36 ACL command contract"
