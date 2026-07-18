param(
    [Parameter(Mandatory = $true)][string]$ExePath,
    [int]$Runs = 6,
    [ValidateSet("CleanEveryRun", "CleanFirst", "Warm")]
    [string]$ProfileMode = "Warm",
    [switch]$MinimizeAfterInteractive,
    [string]$OutputCsv = "startup-benchmark.csv"
)

$ErrorActionPreference = "Stop"
$appData = Join-Path $env:LOCALAPPDATA "WuwaIDLauncher"
$logDir = Join-Path $appData "Logs"
$profile = Join-Path $appData "WebView2"
$logicalCpus = [Environment]::ProcessorCount
$results = @()

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class BenchmarkWindow {
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr handle, int command);
}
"@

function Get-ProcessTreeIds([int]$RootPid) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootPid)
    do {
        $added = $false
        foreach ($item in $all) {
            if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $added = $true }
        }
    } while ($added)
    return @($ids)
}

for ($run = 1; $run -le $Runs; $run++) {
    $cleanThisRun = $ProfileMode -eq "CleanEveryRun" -or
        ($ProfileMode -eq "CleanFirst" -and $run -eq 1)
    if ($cleanThisRun -and (Test-Path $profile)) { Remove-Item $profile -Recurse -Force }
    $log = Join-Path $logDir ("launcher-{0}.log" -f (Get-Date -Format "yyyyMMdd"))
    $beforeLength = if (Test-Path $log) { (Get-Item $log).Length } else { 0 }
    $process = Start-Process -FilePath $ExePath -PassThru
    $deadline = (Get-Date).AddSeconds(45)
    $interactiveMs = $null
    $patchReadyMs = $null
    while ((Get-Date) -lt $deadline -and -not $process.HasExited) {
        if (Test-Path $log) {
            $stream = [System.IO.File]::Open($log, 'Open', 'Read', 'ReadWrite')
            try {
                $stream.Seek($beforeLength, 'Begin') | Out-Null
                $reader = [System.IO.StreamReader]::new($stream)
                $newText = $reader.ReadToEnd()
                $match = [regex]::Match($newText, 'Startup milestone: ui_interactive elapsed_ms=(\d+)')
                if ($match.Success) { $interactiveMs = [int]$match.Groups[1].Value }
                $patchMatch = [regex]::Match($newText, 'Startup milestone: patch_ready elapsed_ms=(\d+)')
                if ($patchMatch.Success) { $patchReadyMs = [int]$patchMatch.Groups[1].Value }
                if ($null -ne $interactiveMs -and $null -ne $patchReadyMs) { break }
            } finally { $stream.Dispose() }
        }
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    if ($null -eq $interactiveMs) { throw "Run $run did not reach ui_interactive." }
    if ($null -eq $patchReadyMs) { throw "Run $run did not reach patch_ready." }

    if ($MinimizeAfterInteractive) {
        $process.Refresh()
        [BenchmarkWindow]::ShowWindow($process.MainWindowHandle, 6) | Out-Null
    }
    Start-Sleep -Seconds 2
    $ids = Get-ProcessTreeIds $process.Id
    $tree = Get-Process -Id $ids -ErrorAction SilentlyContinue
    $workingSetMb = [math]::Round((($tree | Measure-Object WorkingSet64 -Sum).Sum / 1MB), 2)
    $privateBytesMb = [math]::Round((($tree | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB), 2)
    $cpuStart = (($tree | ForEach-Object { $_.TotalProcessorTime.TotalSeconds }) | Measure-Object -Sum).Sum
    $sampleStart = Get-Date
    Start-Sleep -Seconds 10
    $tree = Get-Process -Id (Get-ProcessTreeIds $process.Id) -ErrorAction SilentlyContinue
    $cpuEnd = (($tree | ForEach-Object { $_.TotalProcessorTime.TotalSeconds }) | Measure-Object -Sum).Sum
    $elapsed = ((Get-Date) - $sampleStart).TotalSeconds
    $cpuPercent = [math]::Round((($cpuEnd - $cpuStart) / $elapsed / $logicalCpus) * 100, 2)

    $results += [pscustomobject]@{
        Run = $run
        ProfileMode = $ProfileMode
        ProfileCleaned = $cleanThisRun
        State = if ($MinimizeAfterInteractive) { "Minimized" } else { "Visible" }
        UiInteractiveMs = $interactiveMs
        PatchReadyMs = $patchReadyMs
        WorkingSetMb = $workingSetMb
        PrivateBytesMb = $privateBytesMb
        IdleCpuPercent = $cpuPercent
    }

    if (-not $process.HasExited) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(5000)) { Stop-Process -Id $process.Id -Force }
    }
    Start-Sleep -Seconds 1
}

$results | Export-Csv -Path $OutputCsv -NoTypeInformation
$results | Format-Table -AutoSize
