[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })][string]$LauncherPath,
    [Parameter(Mandatory = $true)][ValidateScript({ Test-Path -LiteralPath $_ -PathType Container })][string]$GamePath,
    [ValidateRange(10, 3600)][int]$ResourceDurationSeconds = 60,
    [string]$OutputRoot = (Join-Path ([IO.Path]::GetTempPath()) ("wuwaid-real-acceptance-" + [guid]::NewGuid().ToString("N")))
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class WuwaIdAcceptanceNative {
    private delegate bool EnumWindowsProc(IntPtr handle, IntPtr data);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr handle, StringBuilder text, int length);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);

    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr handle);

    public static IntPtr FindLauncherWindow(uint processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate(IntPtr handle, IntPtr data) {
            uint ownerProcessId;
            GetWindowThreadProcessId(handle, out ownerProcessId);
            if (ownerProcessId != processId) return true;
            var title = new StringBuilder(256);
            GetWindowText(handle, title, title.Capacity);
            if (title.ToString() == "WuwaID Launcher") {
                found = handle;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static bool HasVisibleLauncherWindow(uint processId) {
        var handle = FindLauncherWindow(processId);
        return handle != IntPtr.Zero && IsWindowVisible(handle);
    }
}
"@

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Wait-Until {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Condition,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds,
        [Parameter(Mandatory = $true)][string]$Description
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $result = & $Condition
            if ($result) { return $result }
        } catch {
            # The process/window can be between startup states; retry until the deadline.
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Description."
}

function Get-LauncherProcesses {
    @(Get-Process -Name "WuwaIDLauncher", "WuwaIDLauncher-resource-audit" -ErrorAction SilentlyContinue)
}

function Get-GameProcesses {
    @(Get-Process -Name "Client-Win64-Shipping" -ErrorAction SilentlyContinue)
}

function Invoke-ElevationSmoke {
    $probe = Start-Process `
        -FilePath $env:ComSpec `
        -ArgumentList "/d /c exit 0" `
        -Verb RunAs `
        -WindowStyle Hidden `
        -PassThru
    try {
        if (-not $probe.WaitForExit(60000)) {
            throw "UAC elevation smoke timed out waiting for the elevated probe."
        }
        if ($probe.ExitCode -ne 0) {
            throw "UAC elevation smoke exited with code $($probe.ExitCode)."
        }
    } finally {
        if (-not $probe.HasExited) {
            Stop-Process -Id $probe.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

function Find-LaunchButton {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $Process.Refresh()
    $handle = [WuwaIdAcceptanceNative]::FindLauncherWindow([uint32]$Process.Id)
    if ($handle -eq [IntPtr]::Zero) { return $null }
    $window = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
    $buttons = $window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
    )
    foreach ($button in $buttons) {
        try {
            $name = $button.Current.Name
            $type = $button.Current.ControlType
            if ($type -eq [System.Windows.Automation.ControlType]::Button -and
                $name -match "^(Mainkan Game|Play Game|Mainkan)$" -and
                $button.Current.IsEnabled) {
                return $button
            }
        } catch {
            continue
        }
    }
    return $null
}

function Invoke-LaunchButton {
    param([Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$Button)

    $pattern = $Button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$RootPid)

    if (-not (Get-Process -Id $RootPid -ErrorAction SilentlyContinue)) { return }
    & taskkill.exe /PID $RootPid /T /F 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0 -and (Get-Process -Id $RootPid -ErrorAction SilentlyContinue)) {
        throw "Could not stop process tree rooted at PID $RootPid."
    }
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Value
    )

    $Value | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}

$resolvedLauncher = (Resolve-Path -LiteralPath $LauncherPath).Path
$resolvedGame = (Resolve-Path -LiteralPath $GamePath).Path
$gameExecutable = Join-Path $resolvedGame "Client\Binaries\Win64\Client-Win64-Shipping.exe"
if (-not (Test-Path -LiteralPath $gameExecutable -PathType Leaf)) {
    throw "GamePath is not a Wuthering Waves installation: $gameExecutable"
}

New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$summaryPath = Join-Path $OutputRoot "summary.json"
$resourcePath = Join-Path $OutputRoot "resource-samples.csv"
$settingsPath = Join-Path $env:LOCALAPPDATA "WuwaIDLauncher\settings.json"
$settingsDirectory = Split-Path -Parent $settingsPath
$settingsBackup = Join-Path $OutputRoot "settings.json.before"
$settingsExisted = Test-Path -LiteralPath $settingsPath -PathType Leaf
$launcher = $null
$gamePid = $null
$startedAt = [DateTime]::UtcNow
$failure = $null
$elevationSmokeVerified = $false
$hiddenWindowVerified = $false
$webviewVerified = $false
$lifecycleRestored = $false
$gameStarted = $false

try {
    if ((Get-LauncherProcesses).Count -gt 0) {
        throw "A WuwaIDLauncher process is already running on the acceptance runner."
    }
    if ((Get-GameProcesses).Count -gt 0) {
        throw "A Client-Win64-Shipping process is already running on the acceptance runner."
    }

    Invoke-ElevationSmoke
    $elevationSmokeVerified = $true

    if ($settingsExisted) {
        Copy-Item -LiteralPath $settingsPath -Destination $settingsBackup -Force
    }
    New-Item -ItemType Directory -Force -Path $settingsDirectory | Out-Null
    Write-JsonFile -Path $settingsPath -Value ([ordered]@{
        gamePath = $resolvedGame
        installMethod = "resource_mount"
        dx11 = $false
        csharpEnvironment = $false
        uidMode = "default"
        uidText = ""
        bgmVolume = 0
        bgmEnabled = $false
    })

    $launcher = Start-Process -FilePath $resolvedLauncher -WorkingDirectory (Split-Path -Parent $resolvedLauncher) -PassThru
    Wait-Until -TimeoutSeconds 60 -Description "launcher window" -Condition {
        $launcher.Refresh()
        $launcher.HasExited -eq $false -and
            [WuwaIdAcceptanceNative]::FindLauncherWindow([uint32]$launcher.Id) -ne [IntPtr]::Zero
    } | Out-Null

    $button = Wait-Until -TimeoutSeconds 120 -Description "enabled Mainkan Game button" -Condition {
        Find-LaunchButton -Process $launcher
    }
    Invoke-LaunchButton -Button $button

    $game = Wait-Until -TimeoutSeconds 180 -Description "the Wuthering Waves game process" -Condition {
        $process = Get-GameProcesses | Select-Object -First 1
        if ($null -ne $process) {
            $gamePid = $process.Id
            $process
        }
    }
    $gameStarted = $true

    $hiddenWindowVerified = [bool](Wait-Until -TimeoutSeconds 30 -Description "launcher tray minimization" -Condition {
        if ([WuwaIdAcceptanceNative]::FindLauncherWindow([uint32]$launcher.Id) -eq [IntPtr]::Zero) { return $false }
        -not [WuwaIdAcceptanceNative]::HasVisibleLauncherWindow([uint32]$launcher.Id)
    })

    & pwsh -NoProfile -File (Join-Path $PSScriptRoot "wut-launcher-resource.tests.ps1") `
        -DurationSeconds $ResourceDurationSeconds `
        -SampleIntervalSeconds 2 `
        -RequireHiddenWindow:$true `
        -RequireWebView:$true `
        -OutputPath $resourcePath
    if ($LASTEXITCODE -ne 0) {
        throw "Launcher resource acceptance failed."
    }
    $resourceRows = @(Import-Csv -LiteralPath $resourcePath)
    if (@($resourceRows | Where-Object { [int]$_.WebViewCount -gt 0 }).Count -eq 0) {
        throw "Real acceptance did not observe a WebView2 process."
    }
    $webviewVerified = $true

    Write-Host "Real game smoke completed; stopping the test game process tree."
    if ($null -ne $gamePid) {
        Stop-ProcessTree -RootPid ([int]$gamePid)
    }
    $gamePid = $null
    [void](Wait-Until -TimeoutSeconds 60 -Description "launcher lifecycle restoration" -Condition {
        $launcher.Refresh()
        -not $launcher.HasExited -and [WuwaIdAcceptanceNative]::HasVisibleLauncherWindow([uint32]$launcher.Id)
    })
    $lifecycleRestored = $true
} catch {
    $failure = $_.Exception.Message
} finally {
    if ($null -ne $gamePid) {
        try { Stop-ProcessTree -RootPid ([int]$gamePid) } catch { Write-Warning $_.Exception.Message }
    }
    if ($null -ne $launcher) {
        try {
            if (-not $launcher.HasExited) {
                Stop-ProcessTree -RootPid ([int]$launcher.Id)
            }
        } catch {
            Write-Warning $_.Exception.Message
        }
    }

    if ($settingsExisted -and (Test-Path -LiteralPath $settingsBackup -PathType Leaf)) {
        New-Item -ItemType Directory -Force -Path $settingsDirectory | Out-Null
        Copy-Item -LiteralPath $settingsBackup -Destination $settingsPath -Force
    } elseif (-not $settingsExisted) {
        Remove-Item -LiteralPath $settingsPath -Force -ErrorAction SilentlyContinue
    }

    Write-JsonFile -Path $summaryPath -Value ([ordered]@{
        status = if ($null -eq $failure) { "PASS" } else { "FAIL" }
        startedAt = $startedAt.ToString("o")
        finishedAt = [DateTime]::UtcNow.ToString("o")
        launcherPath = $resolvedLauncher
        gamePath = $resolvedGame
        elevationSmokeVerified = $elevationSmokeVerified
        gameStarted = $gameStarted
        hiddenWindowVerified = $hiddenWindowVerified
        webviewVerified = $webviewVerified
        lifecycleRestored = $lifecycleRestored
        resourceSamples = if (Test-Path -LiteralPath $resourcePath -PathType Leaf) { $resourcePath } else { $null }
        error = $failure
    })
}

if ($null -ne $failure) {
    throw $failure
}
Write-Host "PASS: real UAC, launcher tray, WebView2 resource, lifecycle, and game smoke acceptance"
