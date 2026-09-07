[CmdletBinding()]
param(
    [string]$LauncherPath = "",
    [string]$FixturePath = "",
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [ValidateRange(10, 600)][int]$VisibleDurationSeconds = 20,
    [ValidateRange(10, 600)][int]$TrayDurationSeconds = 30,
    [ValidateRange(0.25, 10)][double]$SampleIntervalSeconds = 2,
    [ValidateRange(10, 180)][int]$StartupTimeoutSeconds = 90,
    [ValidateRange(10, 180)][int]$TransitionTimeoutSeconds = 60,
    [ValidateRange(10, 180)][int]$RestoreTimeoutSeconds = 60,
    [ValidateRange(0.1, 100)][double]$MaxLauncherCpuPercent = 10,
    [ValidateRange(0.1, 100)][double]$MaxWebViewCpuPercent = 10,
    [ValidateRange(1, 4096)][double]$MaxLauncherPrivateMemoryMB = 512,
    [ValidateRange(1, 4096)][double]$MaxLauncherWorkingSetMB = 512,
    [ValidateRange(1, 8192)][double]$MaxWebViewPrivateMemoryMB = 1024,
    [ValidateRange(1, 8192)][double]$MaxWebViewWorkingSetMB = 1024,
    [ValidateRange(0, 4096)][double]$MaxLauncherMemoryGrowthMB = 32,
    [ValidateRange(1, 5000)][int]$MaxCadenceJitterMilliseconds = 1000,
    [ValidateRange(1024, 1073741824)][double]$MaxLauncherReadBytesPerSecond = 8388608,
    [ValidateRange(1024, 1073741824)][double]$MaxLauncherWriteBytesPerSecond = 4194304,
    [ValidateRange(1024, 1073741824)][double]$MaxWebViewReadBytesPerSecond = 16777216,
    [ValidateRange(1024, 1073741824)][double]$MaxWebViewWriteBytesPerSecond = 8388608,
    [ValidateRange(1000, 300000)][int]$MaxStartupMilliseconds = 60000,
    [ValidateRange(1000, 300000)][int]$MaxLauncherReadyMilliseconds = 120000,
    [ValidateRange(1000, 300000)][int]$MaxTrayTransitionMilliseconds = 60000,
    [ValidateRange(1000, 300000)][int]$MaxRestoreMilliseconds = 60000
)

$ErrorActionPreference = "Stop"
$scriptRoot = (Resolve-Path -LiteralPath $PSScriptRoot).Path
$workspaceRoot = (Resolve-Path -LiteralPath (Join-Path $scriptRoot "..\..")).Path
$resourceSampler = Join-Path $scriptRoot "wut-launcher-resource.tests.ps1"
$resolvedOutputRoot = [IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Force -Path $resolvedOutputRoot | Out-Null

$script:Thresholds = [ordered]@{
    maxLauncherCpuPercent = $MaxLauncherCpuPercent
    maxWebViewCpuPercent = $MaxWebViewCpuPercent
    maxLauncherPrivateMemoryMB = $MaxLauncherPrivateMemoryMB
    maxLauncherWorkingSetMB = $MaxLauncherWorkingSetMB
    maxWebViewPrivateMemoryMB = $MaxWebViewPrivateMemoryMB
    maxWebViewWorkingSetMB = $MaxWebViewWorkingSetMB
    maxLauncherMemoryGrowthMB = $MaxLauncherMemoryGrowthMB
    maxCadenceJitterMilliseconds = $MaxCadenceJitterMilliseconds
    maxLauncherReadBytesPerSecond = $MaxLauncherReadBytesPerSecond
    maxLauncherWriteBytesPerSecond = $MaxLauncherWriteBytesPerSecond
    maxWebViewReadBytesPerSecond = $MaxWebViewReadBytesPerSecond
    maxWebViewWriteBytesPerSecond = $MaxWebViewWriteBytesPerSecond
    maxStartupMilliseconds = $MaxStartupMilliseconds
    maxLauncherReadyMilliseconds = $MaxLauncherReadyMilliseconds
    maxTrayTransitionMilliseconds = $MaxTrayTransitionMilliseconds
    maxRestoreMilliseconds = $MaxRestoreMilliseconds
}

function Resolve-AbsolutePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    return [IO.Path]::GetFullPath($Path)
}

function Resolve-ExistingCandidate {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$ExplicitPath,
        [Parameter(Mandatory = $true)][string[]]$Candidates,
        [Parameter(Mandatory = $true)][string]$Description
    )

    $paths = if ([string]::IsNullOrWhiteSpace($ExplicitPath)) { $Candidates } else { @($ExplicitPath) }
    foreach ($candidate in $paths) {
        $absolute = Resolve-AbsolutePath $candidate
        if (Test-Path -LiteralPath $absolute -PathType Leaf) {
            return $absolute
        }
    }
    throw "$Description tidak ditemukan. Checked: $($paths -join ', ')"
}

function Get-ElapsedMilliseconds {
    param([Parameter(Mandatory = $true)][long]$StartedAt)

    return ([Diagnostics.Stopwatch]::GetTimestamp() - $StartedAt) * 1000.0 / [Diagnostics.Stopwatch]::Frequency
}

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
            if ($null -ne $result -and [bool]$result) {
                return $result
            }
        } catch {
            # Windows process/window state can be transient during startup and handoff.
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Description."
}

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class WuwaIdFixturePerformanceNative {
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

function Get-LauncherProcesses {
    @(Get-Process -Name "WuwaIDLauncher", "WuwaIDLauncher-resource-audit" -ErrorAction SilentlyContinue)
}

function Get-GameProcesses {
    @(Get-Process -Name "Client-Win64-Shipping" -ErrorAction SilentlyContinue)
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$RootPid)

    if (-not (Get-Process -Id $RootPid -ErrorAction SilentlyContinue)) { return }
    & taskkill.exe /PID $RootPid /T /F 2>&1 | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0 -and (Get-Process -Id $RootPid -ErrorAction SilentlyContinue)) {
        throw "Could not stop process tree rooted at PID $RootPid."
    }
}

function Find-LaunchButton {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $Process.Refresh()
    $handle = [WuwaIdFixturePerformanceNative]::FindLauncherWindow([uint32]$Process.Id)
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

function New-FixtureGame {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$FixtureBinary
    )

    $binaryDirectory = Join-Path $Root "Client\Binaries\Win64"
    New-Item -ItemType Directory -Force -Path $binaryDirectory | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Root "Client\Content\Paks") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $Root "Client\Saved\Resources\3.0.0") | Out-Null
    Copy-Item -LiteralPath $FixtureBinary -Destination (Join-Path $binaryDirectory "Client-Win64-Shipping.exe") -Force
    Set-Content -LiteralPath (Join-Path $Root "Client\Saved\Resources\3.0.0\ResManifest") -Value "fixture manifest" -NoNewline
    Set-Content -LiteralPath (Join-Path $Root "Client\Content\Paks\unrelated-fixture.pak") -Value "fixture data" -NoNewline
    return (Join-Path $Root "Client\Binaries\Win64\Client-Win64-Shipping.exe")
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][object]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Get-P95 {
    param([Parameter(Mandatory = $true)][double[]]$Values)

    $sorted = @($Values | Sort-Object)
    $index = [Math]::Max(0, [int][Math]::Ceiling($sorted.Count * 0.95) - 1)
    return [double]$sorted[$index]
}

function Get-ResourceSummary {
    param(
        [Parameter(Mandatory = $true)][string]$Scenario,
        [Parameter(Mandatory = $true)][string]$CsvPath,
        [Parameter(Mandatory = $true)][bool]$ExpectVisible
    )

    $rows = @(Import-Csv -LiteralPath $CsvPath)
    if ($rows.Count -eq 0) {
        throw "$Scenario resource sampler produced no rows."
    }

    $launcherCpu = @($rows | ForEach-Object { [double]$_.LauncherCpuPercent })
    $webViewCpu = @($rows | ForEach-Object { [double]$_.WebViewCpuPercent })
    $launcherPrivate = @($rows | ForEach-Object { [double]$_.LauncherPrivateMB })
    $launcherWorkingSet = @($rows | ForEach-Object { [double]$_.LauncherWorkingSetMB })
    $webViewPrivate = @($rows | ForEach-Object { [double]$_.WebViewPrivateMB })
    $webViewWorkingSet = @($rows | ForEach-Object { [double]$_.WebViewWorkingSetMB })
    $launcherRead = @($rows | ForEach-Object { [double]$_.LauncherReadBytesPerSecond })
    $launcherWrite = @($rows | ForEach-Object { [double]$_.LauncherWriteBytesPerSecond })
    $webViewRead = @($rows | ForEach-Object { [double]$_.WebViewReadBytesPerSecond })
    $webViewWrite = @($rows | ForEach-Object { [double]$_.WebViewWriteBytesPerSecond })
    $jitter = @($rows | ForEach-Object {
        [Math]::Abs(([double]$_.IntervalSeconds - $SampleIntervalSeconds) * 1000)
    })
    $visibleRows = @($rows | Where-Object { $_.WindowVisible -eq "true" })
    $webViewRows = @($rows | Where-Object { [int]$_.WebViewCount -gt 0 })

    return [pscustomobject][ordered]@{
        scenario = $Scenario
        csv = [IO.Path]::GetFileName($CsvPath)
        sampleCount = $rows.Count
        launcherCpuP95Percent = [Math]::Round((Get-P95 -Values $launcherCpu), 4)
        launcherCpuMaxPercent = [Math]::Round((($launcherCpu | Measure-Object -Maximum).Maximum), 4)
        webViewCpuP95Percent = [Math]::Round((Get-P95 -Values $webViewCpu), 4)
        webViewCpuMaxPercent = [Math]::Round((($webViewCpu | Measure-Object -Maximum).Maximum), 4)
        launcherPrivateMemoryStartMB = [Math]::Round($launcherPrivate[0], 2)
        launcherPrivateMemoryMaxMB = [Math]::Round((($launcherPrivate | Measure-Object -Maximum).Maximum), 2)
        launcherPrivateMemoryGrowthMB = [Math]::Round($launcherPrivate[$launcherPrivate.Count - 1] - $launcherPrivate[0], 2)
        launcherWorkingSetMaxMB = [Math]::Round((($launcherWorkingSet | Measure-Object -Maximum).Maximum), 2)
        webViewPrivateMemoryMaxMB = [Math]::Round((($webViewPrivate | Measure-Object -Maximum).Maximum), 2)
        webViewWorkingSetMaxMB = [Math]::Round((($webViewWorkingSet | Measure-Object -Maximum).Maximum), 2)
        launcherReadBytesPerSecondMax = [Math]::Round((($launcherRead | Measure-Object -Maximum).Maximum), 2)
        launcherWriteBytesPerSecondMax = [Math]::Round((($launcherWrite | Measure-Object -Maximum).Maximum), 2)
        webViewReadBytesPerSecondMax = [Math]::Round((($webViewRead | Measure-Object -Maximum).Maximum), 2)
        webViewWriteBytesPerSecondMax = [Math]::Round((($webViewWrite | Measure-Object -Maximum).Maximum), 2)
        maxCadenceJitterMilliseconds = [Math]::Round((($jitter | Measure-Object -Maximum).Maximum), 2)
        webViewSamples = $webViewRows.Count
        visibleSamples = $visibleRows.Count
        hiddenSamples = $rows.Count - $visibleRows.Count
        expectedVisibility = if ($ExpectVisible) { "visible" } else { "hidden" }
    }
}

function Assert-ResourceSummary {
    param(
        [Parameter(Mandatory = $true)][object]$Metrics,
        [Parameter(Mandatory = $true)][bool]$ExpectVisible
    )

    $failures = [System.Collections.Generic.List[string]]::new()
    $t = $script:Thresholds
    if ([double]$Metrics.launcherCpuMaxPercent -gt $t.maxLauncherCpuPercent) {
        $failures.Add("launcher CPU max $($Metrics.launcherCpuMaxPercent)% > $($t.maxLauncherCpuPercent)%")
    }
    if ([double]$Metrics.webViewCpuMaxPercent -gt $t.maxWebViewCpuPercent) {
        $failures.Add("WebView2 CPU max $($Metrics.webViewCpuMaxPercent)% > $($t.maxWebViewCpuPercent)%")
    }
    if ([double]$Metrics.launcherPrivateMemoryMaxMB -gt $t.maxLauncherPrivateMemoryMB) {
        $failures.Add("launcher private memory $($Metrics.launcherPrivateMemoryMaxMB)MB > $($t.maxLauncherPrivateMemoryMB)MB")
    }
    if ([double]$Metrics.launcherWorkingSetMaxMB -gt $t.maxLauncherWorkingSetMB) {
        $failures.Add("launcher working set $($Metrics.launcherWorkingSetMaxMB)MB > $($t.maxLauncherWorkingSetMB)MB")
    }
    if ([double]$Metrics.webViewPrivateMemoryMaxMB -gt $t.maxWebViewPrivateMemoryMB) {
        $failures.Add("WebView2 private memory $($Metrics.webViewPrivateMemoryMaxMB)MB > $($t.maxWebViewPrivateMemoryMB)MB")
    }
    if ([double]$Metrics.webViewWorkingSetMaxMB -gt $t.maxWebViewWorkingSetMB) {
        $failures.Add("WebView2 working set $($Metrics.webViewWorkingSetMaxMB)MB > $($t.maxWebViewWorkingSetMB)MB")
    }
    if ([double]$Metrics.launcherPrivateMemoryGrowthMB -gt $t.maxLauncherMemoryGrowthMB) {
        $failures.Add("launcher private memory growth $($Metrics.launcherPrivateMemoryGrowthMB)MB > $($t.maxLauncherMemoryGrowthMB)MB")
    }
    if ([double]$Metrics.launcherReadBytesPerSecondMax -gt $t.maxLauncherReadBytesPerSecond) {
        $failures.Add("launcher read rate $($Metrics.launcherReadBytesPerSecondMax)B/s > $($t.maxLauncherReadBytesPerSecond)B/s")
    }
    if ([double]$Metrics.launcherWriteBytesPerSecondMax -gt $t.maxLauncherWriteBytesPerSecond) {
        $failures.Add("launcher write rate $($Metrics.launcherWriteBytesPerSecondMax)B/s > $($t.maxLauncherWriteBytesPerSecond)B/s")
    }
    if ([double]$Metrics.webViewReadBytesPerSecondMax -gt $t.maxWebViewReadBytesPerSecond) {
        $failures.Add("WebView2 read rate $($Metrics.webViewReadBytesPerSecondMax)B/s > $($t.maxWebViewReadBytesPerSecond)B/s")
    }
    if ([double]$Metrics.webViewWriteBytesPerSecondMax -gt $t.maxWebViewWriteBytesPerSecond) {
        $failures.Add("WebView2 write rate $($Metrics.webViewWriteBytesPerSecondMax)B/s > $($t.maxWebViewWriteBytesPerSecond)B/s")
    }
    if ([double]$Metrics.maxCadenceJitterMilliseconds -gt $t.maxCadenceJitterMilliseconds) {
        $failures.Add("cadence jitter $($Metrics.maxCadenceJitterMilliseconds)ms > $($t.maxCadenceJitterMilliseconds)ms")
    }
    if ([int]$Metrics.webViewSamples -ne [int]$Metrics.sampleCount) {
        $failures.Add("WebView2 was absent from $([int]$Metrics.sampleCount - [int]$Metrics.webViewSamples) samples")
    }
    if ($ExpectVisible -and [int]$Metrics.visibleSamples -ne [int]$Metrics.sampleCount) {
        $failures.Add("launcher was visible for $($Metrics.visibleSamples)/$($Metrics.sampleCount) samples")
    }
    if (-not $ExpectVisible -and [int]$Metrics.hiddenSamples -ne [int]$Metrics.sampleCount) {
        $failures.Add("launcher was hidden for $($Metrics.hiddenSamples)/$($Metrics.sampleCount) samples")
    }
    if ($failures.Count -gt 0) {
        throw "Resource gate failed for $($Metrics.scenario): $($failures -join '; ')"
    }
}

function Invoke-ResourceSample {
    param(
        [Parameter(Mandatory = $true)][string]$Scenario,
        [Parameter(Mandatory = $true)][int]$DurationSeconds,
        [Parameter(Mandatory = $true)][bool]$RequireHiddenWindow,
        [Parameter(Mandatory = $true)][bool]$RequireVisibleWindow
    )

    $csvPath = Join-Path $resolvedOutputRoot ("resource-{0}.csv" -f $Scenario)
    $logPath = Join-Path $resolvedOutputRoot ("resource-{0}.log" -f $Scenario)
    $arguments = @(
        "-NoProfile",
        "-File", $resourceSampler,
        "-DurationSeconds", $DurationSeconds.ToString(),
        "-SampleIntervalSeconds", $SampleIntervalSeconds.ToString([Globalization.CultureInfo]::InvariantCulture),
        "-MaxLauncherCpuPercent", $MaxLauncherCpuPercent.ToString([Globalization.CultureInfo]::InvariantCulture),
        "-MaxWebViewCpuPercent", $MaxWebViewCpuPercent.ToString([Globalization.CultureInfo]::InvariantCulture),
        "-MaxCadenceJitterMilliseconds", $MaxCadenceJitterMilliseconds.ToString(),
        "-MaxLauncherMemoryGrowthMB", $MaxLauncherMemoryGrowthMB.ToString([Globalization.CultureInfo]::InvariantCulture),
        "-RequireHiddenWindow:$RequireHiddenWindow",
        "-RequireVisibleWindow:$RequireVisibleWindow",
        "-RequireWebView:$true",
        "-OutputPath", $csvPath
    )
    $output = & pwsh @arguments 2>&1
    $exitCode = $LASTEXITCODE
    $output | Set-Content -LiteralPath $logPath -Encoding utf8
    if ($exitCode -ne 0) {
        throw "$Scenario resource sampler failed with exit code $exitCode; see $(Split-Path -Leaf $logPath)."
    }

    $metrics = Get-ResourceSummary -Scenario $Scenario -CsvPath $csvPath -ExpectVisible $RequireVisibleWindow
    Assert-ResourceSummary -Metrics $metrics -ExpectVisible $RequireVisibleWindow
    return $metrics
}

function Invoke-FixtureBuild {
    param([Parameter(Mandatory = $true)][string]$ManifestPath)

    & cargo build --locked --manifest-path $ManifestPath --release --bin wut-game-lifecycle-fixture
    if ($LASTEXITCODE -ne 0) {
        throw "Could not build wut-game-lifecycle-fixture.exe."
    }
}

$summaryPath = Join-Path $resolvedOutputRoot "summary.json"
$launcher = $null
$game = $null
$failure = $null
$previousLocalAppData = $env:LOCALAPPDATA
$summary = [ordered]@{
    schemaVersion = 1
    status = "FAIL"
    generatedAt = $null
    launcherPath = $null
    fixturePath = $null
    fakeGameExecutable = $null
    thresholds = $script:Thresholds
    timingsMilliseconds = [ordered]@{}
    states = [ordered]@{}
    evidence = [ordered]@{
        visibleCsv = "resource-visible.csv"
        trayCsv = "resource-tray.csv"
        visibleLog = "resource-visible.log"
        trayLog = "resource-tray.log"
    }
    error = $null
}

try {
    $resolvedLauncher = Resolve-ExistingCandidate `
        -ExplicitPath $LauncherPath `
        -Candidates @(
            (Join-Path $workspaceRoot "src-tauri\target\release\WuwaIDLauncher.exe"),
            (Join-Path $workspaceRoot "src-tauri\target\x86_64-pc-windows-msvc\release\WuwaIDLauncher.exe")
        ) `
        -Description "WuwaIDLauncher.exe"
    $fixtureCandidates = @(
        (Join-Path $workspaceRoot "src-tauri\target\release\wut-game-lifecycle-fixture.exe"),
        (Join-Path $workspaceRoot "src-tauri\target\debug\wut-game-lifecycle-fixture.exe"),
        (Join-Path $workspaceRoot "src-tauri\target\x86_64-pc-windows-msvc\release\wut-game-lifecycle-fixture.exe")
    )
    try {
        $resolvedFixture = Resolve-ExistingCandidate -ExplicitPath $FixturePath -Candidates $fixtureCandidates -Description "wut-game-lifecycle-fixture.exe"
    } catch {
        if (-not [string]::IsNullOrWhiteSpace($FixturePath)) { throw }
        Invoke-FixtureBuild -ManifestPath (Join-Path $workspaceRoot "src-tauri\Cargo.toml")
        $resolvedFixture = Resolve-ExistingCandidate -ExplicitPath "" -Candidates $fixtureCandidates -Description "wut-game-lifecycle-fixture.exe after build"
    }

    $summary["launcherPath"] = $resolvedLauncher
    $summary["fixturePath"] = $resolvedFixture
    if ((Get-LauncherProcesses).Count -gt 0) {
        throw "A WuwaIDLauncher process is already running on the performance runner."
    }
    if ((Get-GameProcesses).Count -gt 0) {
        throw "A Client-Win64-Shipping process is already running on the performance runner."
    }

    $fixtureRoot = Join-Path $resolvedOutputRoot "fixture-game"
    if (Test-Path -LiteralPath $fixtureRoot) {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
    }
    $fakeGameExecutable = New-FixtureGame -Root $fixtureRoot -FixtureBinary $resolvedFixture
    $summary["fakeGameExecutable"] = $fakeGameExecutable

    $isolatedLocalAppData = Join-Path $resolvedOutputRoot "localappdata"
    New-Item -ItemType Directory -Force -Path $isolatedLocalAppData | Out-Null
    $env:LOCALAPPDATA = $isolatedLocalAppData
    $settingsPath = Join-Path $isolatedLocalAppData "WuwaIDLauncher\settings.json"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $settingsPath) | Out-Null
    Write-JsonFile -Path $settingsPath -Value ([ordered]@{
        gamePath = $fixtureRoot
        installMethod = "resource_mount"
        dx11 = $false
        csharpEnvironment = $false
        uidMode = "default"
        uidText = ""
        bgmVolume = 0
        bgmEnabled = $false
    })

    $env:WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS = ([Math]::Max(120, $TrayDurationSeconds + 60)).ToString()
    $launcherStart = [Diagnostics.Stopwatch]::GetTimestamp()
    $launcher = Start-Process `
        -FilePath $resolvedLauncher `
        -WorkingDirectory (Split-Path -Parent $resolvedLauncher) `
        -PassThru
    [void](Wait-Until -TimeoutSeconds $StartupTimeoutSeconds -Description "launcher window" -Condition {
        $launcher.Refresh()
        -not $launcher.HasExited -and
            [WuwaIdFixturePerformanceNative]::FindLauncherWindow([uint32]$launcher.Id) -ne [IntPtr]::Zero
    })
    $startupMilliseconds = [Math]::Round((Get-ElapsedMilliseconds -StartedAt $launcherStart), 2)

    $readyStart = [Diagnostics.Stopwatch]::GetTimestamp()
    $button = Wait-Until -TimeoutSeconds $StartupTimeoutSeconds -Description "enabled Mainkan Game button" -Condition {
        Find-LaunchButton -Process $launcher
    }
    $readyMilliseconds = [Math]::Round((Get-ElapsedMilliseconds -StartedAt $readyStart), 2)
    $summary["timingsMilliseconds"]["startupWindow"] = $startupMilliseconds
    $summary["timingsMilliseconds"]["launcherReadyAfterWindow"] = $readyMilliseconds
    if ($startupMilliseconds -gt $MaxStartupMilliseconds) {
        throw "Launcher startup took $startupMilliseconds ms; maximum is $MaxStartupMilliseconds ms."
    }
    if ($readyMilliseconds -gt $MaxLauncherReadyMilliseconds) {
        throw "Launcher ready-state took $readyMilliseconds ms; maximum is $MaxLauncherReadyMilliseconds ms."
    }

    $visibleMetrics = Invoke-ResourceSample `
        -Scenario "visible" `
        -DurationSeconds $VisibleDurationSeconds `
        -RequireHiddenWindow:$false `
        -RequireVisibleWindow:$true
    $summary["states"]["visible"] = $visibleMetrics

    $trayStart = [Diagnostics.Stopwatch]::GetTimestamp()
    Invoke-LaunchButton -Button $button
    $game = Wait-Until -TimeoutSeconds $TransitionTimeoutSeconds -Description "fake game process" -Condition {
        Get-GameProcesses | Select-Object -First 1
    }
    [void](Wait-Until -TimeoutSeconds $TransitionTimeoutSeconds -Description "launcher tray visibility" -Condition {
        $launcher.Refresh()
        -not $launcher.HasExited -and
            -not [WuwaIdFixturePerformanceNative]::HasVisibleLauncherWindow([uint32]$launcher.Id)
    })
    $trayTransitionMilliseconds = [Math]::Round((Get-ElapsedMilliseconds -StartedAt $trayStart), 2)
    $summary["timingsMilliseconds"]["visibleToTray"] = $trayTransitionMilliseconds
    if ($trayTransitionMilliseconds -gt $MaxTrayTransitionMilliseconds) {
        throw "Visible-to-tray transition took $trayTransitionMilliseconds ms; maximum is $MaxTrayTransitionMilliseconds ms."
    }

    $trayMetrics = Invoke-ResourceSample `
        -Scenario "tray" `
        -DurationSeconds $TrayDurationSeconds `
        -RequireHiddenWindow:$true `
        -RequireVisibleWindow:$false
    $summary["states"]["tray"] = $trayMetrics

    $restoreStart = [Diagnostics.Stopwatch]::GetTimestamp()
    $currentGame = Get-GameProcesses | Select-Object -First 1
    if ($null -eq $currentGame) {
        throw "Fake game exited before restore measurement."
    }
    Stop-ProcessTree -RootPid ([int]$currentGame.Id)
    [void](Wait-Until -TimeoutSeconds $RestoreTimeoutSeconds -Description "launcher restore after fake game exit" -Condition {
        $launcher.Refresh()
        -not $launcher.HasExited -and
            [WuwaIdFixturePerformanceNative]::HasVisibleLauncherWindow([uint32]$launcher.Id)
    })
    $restoreMilliseconds = [Math]::Round((Get-ElapsedMilliseconds -StartedAt $restoreStart), 2)
    $summary["timingsMilliseconds"]["trayToVisibleRestore"] = $restoreMilliseconds
    if ($restoreMilliseconds -gt $MaxRestoreMilliseconds) {
        throw "Tray-to-visible restore took $restoreMilliseconds ms; maximum is $MaxRestoreMilliseconds ms."
    }

    $summary["status"] = "PASS"
} catch {
    $failure = $_.Exception.Message
} finally {
    try {
        if ($null -ne $game) {
            $currentGame = Get-GameProcesses | Select-Object -First 1
            if ($null -ne $currentGame) { Stop-ProcessTree -RootPid ([int]$currentGame.Id) }
        }
    } catch { Write-Warning $_.Exception.Message }
    try {
        if ($null -ne $launcher) {
            $launcher.Refresh()
            if (-not $launcher.HasExited) { Stop-ProcessTree -RootPid ([int]$launcher.Id) }
        }
    } catch { Write-Warning $_.Exception.Message }
    $env:LOCALAPPDATA = $previousLocalAppData
    Remove-Item Env:WUWAID_LAUNCHER_FIXTURE_CHILD_LIFETIME_SECONDS -ErrorAction SilentlyContinue
    $summary["generatedAt"] = [DateTime]::UtcNow.ToString("o")
    $summary["error"] = $failure
    Write-JsonFile -Value ([pscustomobject]$summary) -Path $summaryPath
}

if ($null -ne $failure) {
    throw $failure
}

Write-Output "PASS: visible and tray fixture performance matrix stayed within resource and latency thresholds"
Write-Output ("summary={0}" -f (Resolve-Path -LiteralPath $summaryPath))
