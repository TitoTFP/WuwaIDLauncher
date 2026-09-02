[CmdletBinding()]
param(
    [ValidateRange(10, 3600)][int]$DurationSeconds = 60,
    [ValidateRange(0.25, 10)][double]$SampleIntervalSeconds = 2,
    [ValidateRange(0, 3600)][int]$MinimumSamples = 0,
    [ValidateRange(0.1, 100)][double]$MaxLauncherCpuPercent = 1.0,
    [ValidateRange(0.1, 100)][double]$MaxWebViewCpuPercent = 1.0,
    [ValidateRange(1, 5000)][int]$MaxCadenceJitterMilliseconds = 1000,
    [bool]$RequireHiddenWindow = $true,
    [bool]$RequireWebView = $true,
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class WutNativeProbe {
    public delegate bool EnumWindowsProc(IntPtr handle, IntPtr data);

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeFileTime {
        public uint LowDateTime;
        public uint HighDateTime;

        public ulong ToUInt64() {
            return ((ulong)HighDateTime << 32) | LowDateTime;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MemoryStatusEx {
        public uint Length;
        public uint MemoryLoad;
        public ulong TotalPhysical;
        public ulong AvailablePhysical;
        public ulong TotalPageFile;
        public ulong AvailablePageFile;
        public ulong TotalVirtual;
        public ulong AvailableVirtual;
        public ulong AvailableExtendedVirtual;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct IoCounters {
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
    }

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr data);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr handle);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr handle, StringBuilder text, int length);

    [DllImport("kernel32.dll")]
    private static extern bool GetSystemTimes(
        out NativeFileTime idleTime,
        out NativeFileTime kernelTime,
        out NativeFileTime userTime);

    [DllImport("kernel32.dll")]
    private static extern bool GlobalMemoryStatusEx(ref MemoryStatusEx status);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(uint access, bool inheritHandle, uint processId);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetProcessIoCounters(IntPtr handle, out IoCounters counters);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    public static string GetWindowTitle(IntPtr handle) {
        var text = new StringBuilder(256);
        GetWindowText(handle, text, text.Capacity);
        return text.ToString();
    }

    public static IntPtr[] GetProcessWindows(uint processId) {
        var handles = new List<IntPtr>();
        EnumWindows(delegate(IntPtr handle, IntPtr data) {
            uint ownerProcessId;
            GetWindowThreadProcessId(handle, out ownerProcessId);
            if (ownerProcessId == processId) {
                handles.Add(handle);
            }
            return true;
        }, IntPtr.Zero);
        return handles.ToArray();
    }

    public static ulong[] GetSystemTimesSnapshot() {
        NativeFileTime idleTime;
        NativeFileTime kernelTime;
        NativeFileTime userTime;
        if (!GetSystemTimes(out idleTime, out kernelTime, out userTime)) {
            return new ulong[] { 0, 0, 0 };
        }
        return new ulong[] {
            idleTime.ToUInt64(),
            kernelTime.ToUInt64(),
            userTime.ToUInt64()
        };
    }

    public static double GetSystemCpuPercent(ulong[] previous, ulong[] current) {
        if (previous == null || current == null || previous.Length < 3 || current.Length < 3) {
            return 0;
        }
        var idle = current[0] - previous[0];
        var total = (current[1] - previous[1]) + (current[2] - previous[2]);
        if (total == 0) {
            return 0;
        }
        return Math.Max(0, Math.Min(100, (1.0 - ((double)idle / total)) * 100.0));
    }

    public static double GetSystemMemoryPercent() {
        var status = new MemoryStatusEx();
        status.Length = (uint)Marshal.SizeOf(typeof(MemoryStatusEx));
        return GlobalMemoryStatusEx(ref status) ? status.MemoryLoad : 0;
    }

    public static ulong[] GetProcessIoCounters(uint processId) {
        const uint processQueryLimitedInformation = 0x1000;
        var handle = OpenProcess(processQueryLimitedInformation, false, processId);
        if (handle == IntPtr.Zero) {
            return new ulong[] { 0, 0 };
        }
        IoCounters counters;
        var success = GetProcessIoCounters(handle, out counters);
        CloseHandle(handle);
        if (!success) {
            return new ulong[] { 0, 0 };
        }
        return new ulong[] { counters.ReadTransferCount, counters.WriteTransferCount };
    }
}
"@

function Get-LauncherProcess {
    $processes = @(
        Get-Process -Name "WuwaIDLauncher", "WuwaIDLauncher-resource-audit" -ErrorAction SilentlyContinue
    )
    if ($processes.Count -ne 1) {
        throw "Expected exactly one WuwaIDLauncher.exe (or resource-audit build) process; found $($processes.Count)."
    }
    return $processes[0]
}

function Get-ProcessTreeIds {
    param(
        [Parameter(Mandatory = $true)][int]$RootPid,
        [Parameter(Mandatory = $true)][object[]]$ProcessRows
    )

    $childrenByParent = @{}
    foreach ($row in $ProcessRows) {
        $parent = [int]$row.ParentProcessId
        if (-not $childrenByParent.ContainsKey($parent)) {
            $childrenByParent[$parent] = [System.Collections.Generic.List[int]]::new()
        }
        $childrenByParent[$parent].Add([int]$row.ProcessId)
    }

    $seen = @{$RootPid = $true}
    $queue = [System.Collections.Generic.Queue[int]]::new()
    $queue.Enqueue($RootPid)
    while ($queue.Count -gt 0) {
        $parent = $queue.Dequeue()
        if (-not $childrenByParent.ContainsKey($parent)) { continue }
        foreach ($child in $childrenByParent[$parent]) {
            if (-not $seen.ContainsKey($child)) {
                $seen[$child] = $true
                $queue.Enqueue($child)
            }
        }
    }
    return @($seen.Keys | ForEach-Object { [int]$_ })
}

function Get-LauncherWindowHandle {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $Process.Refresh()
    foreach ($handle in @([WutNativeProbe]::GetProcessWindows([uint32]$Process.Id))) {
        if ([WutNativeProbe]::GetWindowTitle($handle) -eq "WuwaID Launcher") {
            return [IntPtr]$handle
        }
    }
    return [IntPtr]::Zero
}

function Get-WindowVisibility {
    param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

    $handle = Get-LauncherWindowHandle -Process $Process
    if ($handle -eq [IntPtr]::Zero) { return "unknown" }
    return ([WutNativeProbe]::IsWindowVisible($handle)).ToString().ToLowerInvariant()
}

function Get-NonNegativeDelta {
    param(
        [Parameter(Mandatory = $true)][uint64]$Current,
        [Parameter(Mandatory = $true)][uint64]$Previous
    )

    if ($Current -lt $Previous) { return 0.0 }
    return [double]($Current - $Previous)
}

function Get-ProcessSample {
    param(
        [Parameter(Mandatory = $true)][int]$LauncherPid,
        [Parameter(Mandatory = $true)][int[]]$WebViewPids,
        [Parameter(Mandatory = $true)][uint64[]]$PreviousSystemTimes
    )

    $launcher = Get-Process -Id $LauncherPid -ErrorAction Stop
    $launcher.Refresh()
    $launcherIo = [WutNativeProbe]::GetProcessIoCounters([uint32]$LauncherPid)
    $webviewCpu = 0.0
    $webviewPrivateBytes = [int64]0
    $webviewReadBytes = [uint64]0
    $webviewWriteBytes = [uint64]0
    $webviewCount = 0

    foreach ($processId in $WebViewPids) {
        $child = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($null -eq $child -or $child.ProcessName -ine "msedgewebview2") { continue }
        try {
            $child.Refresh()
            $childIo = [WutNativeProbe]::GetProcessIoCounters([uint32]$processId)
            $webviewCount++
            $webviewCpu += $child.TotalProcessorTime.TotalSeconds
            $webviewPrivateBytes += [int64]$child.PrivateMemorySize64
            $webviewReadBytes += [uint64]$childIo[0]
            $webviewWriteBytes += [uint64]$childIo[1]
        } catch [System.InvalidOperationException] {
            continue
        }
    }

    $systemTimes = [WutNativeProbe]::GetSystemTimesSnapshot()
    [pscustomobject]@{
        Timestamp = [DateTime]::UtcNow
        LauncherCpuSeconds = [double]$launcher.TotalProcessorTime.TotalSeconds
        LauncherPrivateBytes = [int64]$launcher.PrivateMemorySize64
        LauncherWorkingSetBytes = [int64]$launcher.WorkingSet64
        LauncherReadBytes = [uint64]$launcherIo[0]
        LauncherWriteBytes = [uint64]$launcherIo[1]
        WebViewCpuSeconds = [double]$webviewCpu
        WebViewPrivateBytes = [int64]$webviewPrivateBytes
        WebViewReadBytes = $webviewReadBytes
        WebViewWriteBytes = $webviewWriteBytes
        WebViewCount = $webviewCount
        WindowVisible = Get-WindowVisibility -Process $launcher
        SystemTimes = $systemTimes
        SystemCpuPercent = [double]([WutNativeProbe]::GetSystemCpuPercent($PreviousSystemTimes, $systemTimes))
        SystemMemoryPercent = [double]([WutNativeProbe]::GetSystemMemoryPercent())
    }
}

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path ([IO.Path]::GetTempPath()) ("wut-launcher-resource-{0}.csv" -f [guid]::NewGuid().ToString("N"))
}

$launcher = Get-LauncherProcess
$launcherPid = $launcher.Id
$initialVisibility = Get-WindowVisibility -Process $launcher
if ($RequireHiddenWindow -and $initialVisibility -ne "false") {
    throw "Launcher window is not proven hidden (visibility=$initialVisibility). Put it in tray and rerun."
}

$processRows = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name | Select-Object ProcessId, ParentProcessId, Name)
$treeIds = @(Get-ProcessTreeIds -RootPid $launcherPid -ProcessRows $processRows)
$treePidSet = @{}
foreach ($processId in $treeIds) {
    $treePidSet[$processId] = $true
}
$webviewPids = @(
    $processRows |
        Where-Object {
            $treePidSet.ContainsKey([int]$_.ProcessId) -and $_.Name -ieq "msedgewebview2.exe"
        } |
        ForEach-Object { [int]$_.ProcessId }
)

$expectedSamples = [Math]::Max(1, [int][Math]::Floor($DurationSeconds / $SampleIntervalSeconds))
$minimumRequired = if ($MinimumSamples -gt 0) {
    $MinimumSamples
} else {
    $expectedSamples
}
$rows = [System.Collections.Generic.List[object]]::new()
$previous = Get-ProcessSample -LauncherPid $launcherPid -WebViewPids $webviewPids -PreviousSystemTimes ([WutNativeProbe]::GetSystemTimesSnapshot())
# Schedule from completed probes so baseline latency cannot create a catch-up interval.
$nextDeadline = $previous.Timestamp.AddSeconds($SampleIntervalSeconds)
for ($sampleIndex = 0; $sampleIndex -lt $expectedSamples; $sampleIndex++) {
    $delayMilliseconds = ($nextDeadline - [DateTime]::UtcNow).TotalMilliseconds
    if ($delayMilliseconds -gt 0) {
        Start-Sleep -Milliseconds ([Math]::Max(1, [int][Math]::Round($delayMilliseconds)))
    }

    $current = Get-ProcessSample -LauncherPid $launcherPid -WebViewPids $webviewPids -PreviousSystemTimes $previous.SystemTimes
    $wallSeconds = ($current.Timestamp - $previous.Timestamp).TotalSeconds
    if ($wallSeconds -le 0) {
        throw "Resource sampler clock did not advance between samples."
    }
    $launcherCpu = [Math]::Max(0, (($current.LauncherCpuSeconds - $previous.LauncherCpuSeconds) / $wallSeconds) / [Environment]::ProcessorCount * 100)
    $webviewCpu = [Math]::Max(0, (($current.WebViewCpuSeconds - $previous.WebViewCpuSeconds) / $wallSeconds) / [Environment]::ProcessorCount * 100)
    $rows.Add([pscustomobject]@{
        Timestamp = $current.Timestamp.ToString("o")
        IntervalSeconds = [Math]::Round($wallSeconds, 4)
        LauncherCpuPercent = [Math]::Round($launcherCpu, 4)
        LauncherPrivateMB = [Math]::Round($current.LauncherPrivateBytes / 1MB, 2)
        LauncherWorkingSetMB = [Math]::Round($current.LauncherWorkingSetBytes / 1MB, 2)
        LauncherReadBytes = $current.LauncherReadBytes
        LauncherWriteBytes = $current.LauncherWriteBytes
        LauncherReadBytesPerSecond = [Math]::Round((Get-NonNegativeDelta -Current $current.LauncherReadBytes -Previous $previous.LauncherReadBytes) / $wallSeconds, 2)
        LauncherWriteBytesPerSecond = [Math]::Round((Get-NonNegativeDelta -Current $current.LauncherWriteBytes -Previous $previous.LauncherWriteBytes) / $wallSeconds, 2)
        WebViewCpuPercent = [Math]::Round($webviewCpu, 4)
        WebViewPrivateMB = [Math]::Round($current.WebViewPrivateBytes / 1MB, 2)
        WebViewReadBytes = $current.WebViewReadBytes
        WebViewWriteBytes = $current.WebViewWriteBytes
        WebViewReadBytesPerSecond = [Math]::Round((Get-NonNegativeDelta -Current $current.WebViewReadBytes -Previous $previous.WebViewReadBytes) / $wallSeconds, 2)
        WebViewWriteBytesPerSecond = [Math]::Round((Get-NonNegativeDelta -Current $current.WebViewWriteBytes -Previous $previous.WebViewWriteBytes) / $wallSeconds, 2)
        WebViewCount = $current.WebViewCount
        WindowVisible = $current.WindowVisible
        SystemCpuPercent = [Math]::Round($current.SystemCpuPercent, 2)
        SystemMemoryPercent = [Math]::Round($current.SystemMemoryPercent, 2)
    })
    $previous = $current
    $nextDeadline = $current.Timestamp.AddSeconds($SampleIntervalSeconds)
}

$rows | Export-Csv -LiteralPath $OutputPath -NoTypeInformation
if ($rows.Count -lt $minimumRequired) {
    throw "Resource sampler collected $($rows.Count) samples; minimum required is $minimumRequired."
}

$launcherCpuValues = @($rows | ForEach-Object { [double]$_.LauncherCpuPercent })
$webviewCpuValues = @($rows | ForEach-Object { [double]$_.WebViewCpuPercent })
$launcherCpuMax = ($launcherCpuValues | Measure-Object -Maximum).Maximum
$webviewCpuMax = ($webviewCpuValues | Measure-Object -Maximum).Maximum
$launcherCpuP95 = ($launcherCpuValues | Sort-Object)[[Math]::Max(0, [Math]::Ceiling($launcherCpuValues.Count * 0.95) - 1)]
$webviewCpuP95 = ($webviewCpuValues | Sort-Object)[[Math]::Max(0, [Math]::Ceiling($webviewCpuValues.Count * 0.95) - 1)]
$firstMemory = [double]$rows[0].LauncherPrivateMB
$lastMemory = [double]$rows[$rows.Count - 1].LauncherPrivateMB
$memoryGrowth = [Math]::Round($lastMemory - $firstMemory, 2)
$visibleRows = @($rows | Where-Object { $_.WindowVisible -eq "true" })
$webviewRows = @($rows | Where-Object { [int]$_.WebViewCount -gt 0 })
$cadenceJitter = @($rows | ForEach-Object { [Math]::Abs(([double]$_.IntervalSeconds - $SampleIntervalSeconds) * 1000) })
$maxCadenceJitter = ($cadenceJitter | Measure-Object -Maximum).Maximum
$systemCpuAverage = [Math]::Round(($rows | Measure-Object -Property SystemCpuPercent -Average).Average, 2)
$systemMemoryAverage = [Math]::Round(($rows | Measure-Object -Property SystemMemoryPercent -Average).Average, 2)

Write-Output ("samples={0} minimumSamples={1} intervalSeconds={2} durationSeconds={3}" -f $rows.Count, $minimumRequired, $SampleIntervalSeconds, $DurationSeconds)
Write-Output ("launcherCpuP95={0}% launcherCpuMax={1}%" -f $launcherCpuP95, $launcherCpuMax)
Write-Output ("webviewCpuP95={0}% webviewCpuMax={1}%" -f $webviewCpuP95, $webviewCpuMax)
Write-Output ("launcherPrivateMemoryStart={0}MB end={1}MB growth={2}MB" -f $firstMemory, $lastMemory, $memoryGrowth)
Write-Output ("launcherReadBytes={0} launcherWriteBytes={1} webviewReadBytes={2} webviewWriteBytes={3}" -f $rows[$rows.Count - 1].LauncherReadBytes, $rows[$rows.Count - 1].LauncherWriteBytes, $rows[$rows.Count - 1].WebViewReadBytes, $rows[$rows.Count - 1].WebViewWriteBytes)
Write-Output ("visibleWindowSamples={0} webviewPids={1} webviewSamples={2} maxCadenceJitterMs={3} systemCpuAvg={4}% systemMemoryAvg={5}%" -f $visibleRows.Count, $webviewPids.Count, $webviewRows.Count, [Math]::Round($maxCadenceJitter, 2), $systemCpuAverage, $systemMemoryAverage)
Write-Output ("rawSamples={0}" -f (Resolve-Path -LiteralPath $OutputPath))

if ($RequireWebView -and $webviewRows.Count -eq 0) {
    throw "Resource sampler did not observe a WebView2 process in the launcher process tree."
}
if ($RequireHiddenWindow -and $visibleRows.Count -gt 0) {
    throw "Launcher became visible during the tray sample."
}
if ($launcherCpuMax -gt $MaxLauncherCpuPercent) {
    throw "Launcher CPU sample max $launcherCpuMax% exceeds $MaxLauncherCpuPercent%."
}
if ($webviewCpuMax -gt $MaxWebViewCpuPercent) {
    throw "WebView2 CPU sample max $webviewCpuMax% exceeds $MaxWebViewCpuPercent%."
}
if ($maxCadenceJitter -gt $MaxCadenceJitterMilliseconds) {
    throw "Sampler cadence jitter max $([Math]::Round($maxCadenceJitter, 2))ms exceeds $MaxCadenceJitterMilliseconds ms."
}
if ($memoryGrowth -gt 32) {
    throw "Launcher private memory grew by $memoryGrowth MB during the sample."
}

Write-Output "PASS: tray resource sample stayed within CPU, WebView2, visibility, cadence, I/O, and memory bounds"
