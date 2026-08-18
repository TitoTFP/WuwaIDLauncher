[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("automated", "manual", "all")]
    [string]$Mode,

    [Parameter(Mandatory = $true)]
    [string]$ArtifactRoot,

    [Parameter(Mandatory = $true)]
    [string]$OutputRoot,

    [Parameter(Mandatory = $true)]
    [string]$FixtureRoot,

    [string]$GamePath,

    # Contract-test-only fault injection; normal release runs never pass this switch.
    [switch]$TestFailureAfterFixture,

    # Full local check/build/package commands are opt-in because they are expensive and mutate build output.
    [switch]$RunCommandGate
)

$ErrorActionPreference = "Stop"

function Resolve-AbsolutePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $fullPath = [IO.Path]::GetFullPath($Path)
    $pathRoot = [IO.Path]::GetPathRoot($fullPath)
    if ($fullPath.Length -gt $pathRoot.Length) {
        return $fullPath.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    }
    return $pathRoot
}

function Test-PathWithin {
    param(
        [Parameter(Mandatory = $true)][string]$Child,
        [Parameter(Mandatory = $true)][string]$Parent
    )

    $childPath = Resolve-AbsolutePath $Child
    $parentPath = Resolve-AbsolutePath $Parent
    if ($childPath.Equals($parentPath, [StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }
    $prefix = if ($parentPath.EndsWith([IO.Path]::DirectorySeparatorChar)) {
        $parentPath
    } else {
        $parentPath + [IO.Path]::DirectorySeparatorChar
    }
    return $childPath.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Test-PathOverlap {
    param(
        [Parameter(Mandatory = $true)][string]$First,
        [Parameter(Mandatory = $true)][string]$Second
    )

    return (Test-PathWithin -Child $First -Parent $Second) -or (Test-PathWithin -Child $Second -Parent $First)
}

function New-Scenario {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][ValidateSet("PASS", "FAIL", "BLOCKED")][string]$Status,
        [string]$Message = "",
        [string[]]$Evidence = @()
    )

    $now = [DateTime]::UtcNow.ToString("o")
    return [pscustomobject][ordered]@{
        name       = $Name
        status     = $Status
        startedAt  = $now
        finishedAt = $now
        evidence   = @($Evidence)
        message    = $Message
    }
}

function Get-FixtureSnapshot {
    param([Parameter(Mandatory = $true)][string]$Root)

    $files = @(Get-ChildItem -LiteralPath $Root -File -Recurse -Force)
    return @($files | ForEach-Object {
        $relativePath = [IO.Path]::GetRelativePath($Root, $_.FullName).Replace([IO.Path]::DirectorySeparatorChar, "/")
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        [pscustomobject][ordered]@{
            path   = $relativePath
            length = $_.Length
            sha256 = $hash
        }
    })
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][object]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $Value | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Add-Scenario {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][ValidateSet("PASS", "FAIL", "BLOCKED")][string]$Status,
        [string]$Message = "",
        [string[]]$Evidence = @()
    )

    $script:Scenarios.Add((New-Scenario -Name $Name -Status $Status -Message $Message -Evidence $Evidence)) | Out-Null
}

function Write-Report {
    $script:Report.finishedAt = [DateTime]::UtcNow.ToString("o")
    Write-JsonFile -Value ([pscustomobject]$script:Report) -Path $script:ReportPath
}

function Test-OwnerMarker {
    if (-not (Test-Path -LiteralPath $script:OwnerMarkerPath -PathType Leaf)) {
        return $false
    }

    $owner = Get-Content -Raw -LiteralPath $script:OwnerMarkerPath | ConvertFrom-Json
    return $owner.runId -eq $script:RunId -and
        (Resolve-AbsolutePath $owner.root).Equals((Resolve-AbsolutePath $script:RunFixtureRoot), [StringComparison]::OrdinalIgnoreCase)
}

function Remove-RunFixtureSafely {
    if (-not (Test-OwnerMarker)) {
        throw "Owner marker tidak cocok; fixture dipertahankan: $script:RunFixtureRoot"
    }

    Remove-Item -LiteralPath $script:RunFixtureRoot -Recurse -Force -ErrorAction Stop
}

function New-FixtureLayout {
    $directories = @(
        "Client\Binaries\Win64",
        "Client\Content\Paks",
        "Client\Saved\Resources\3.0.0"
    )
    foreach ($directory in $directories) {
        New-Item -ItemType Directory -Force -Path (Join-Path $script:RunFixtureRoot $directory) | Out-Null
    }

    Set-Content -LiteralPath (Join-Path $script:RunFixtureRoot "Client\Binaries\Win64\Client-Win64-Shipping.exe") -Value "fixture executable" -NoNewline
    Set-Content -LiteralPath (Join-Path $script:RunFixtureRoot "Client\Saved\Resources\3.0.0\ResManifest") -Value "fixture manifest" -NoNewline
}

function Find-ArtifactFile {
    param(
        [Parameter(Mandatory = $true)][string[]]$RelativePaths
    )

    foreach ($relativePath in $RelativePaths) {
        $candidate = Join-Path $script:ArtifactRootPath ($relativePath -replace "/", [IO.Path]::DirectorySeparatorChar)
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return Resolve-AbsolutePath $candidate
        }
    }
    return $null
}

function Get-WorkspaceMetadata {
    $packagePath = Join-Path $script:WorkspaceRoot "package.json"
    $cargoPath = Join-Path $script:WorkspaceRoot "src-tauri\Cargo.toml"
    $tauriPath = Join-Path $script:WorkspaceRoot "src-tauri\tauri.conf.json"
    if (-not (Test-Path -LiteralPath $packagePath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $cargoPath -PathType Leaf) -or
        -not (Test-Path -LiteralPath $tauriPath -PathType Leaf)) {
        throw "Metadata release tidak lengkap: package.json, Cargo.toml, dan tauri.conf.json wajib ada."
    }

    $package = Get-Content -Raw -LiteralPath $packagePath | ConvertFrom-Json
    $cargoVersionMatch = [regex]::Match((Get-Content -Raw -LiteralPath $cargoPath), '(?m)^\s*version\s*=\s*"(?<version>[^"\r\n]+)"')
    $tauri = Get-Content -Raw -LiteralPath $tauriPath | ConvertFrom-Json
    if (-not $cargoVersionMatch.Success) {
        throw "Versi package Cargo tidak ditemukan."
    }

    return [pscustomobject][ordered]@{
        packageVersion = [string]$package.version
        cargoVersion   = [string]$cargoVersionMatch.Groups["version"].Value
        tauriVersion   = [string]$tauri.version
        icons          = @($tauri.bundle.icon)
    }
}

function Set-ReleaseArtifactPaths {
    param([Parameter(Mandatory = $true)][string]$Version)

    $zipName = "WuwaIDLauncher_{0}_x64.zip" -f $Version
    $msiName = "WuwaIDLauncher_{0}_x64_en-US.msi" -f $Version
    $nsisName = "WuwaIDLauncher_{0}_x64-setup.exe" -f $Version
    $script:Artifacts = [ordered]@{
        executable = Find-ArtifactFile -RelativePaths @(
            "wuwaid-launcher.exe",
            "bundle/release/wuwaid-launcher.exe"
        )
        zip = Find-ArtifactFile -RelativePaths @(
            "bundle/release/$zipName",
            "bundle/$zipName",
            $zipName
        )
        msi = Find-ArtifactFile -RelativePaths @(
            "bundle/release/$msiName",
            "bundle/msi/$msiName",
            "bundle/$msiName",
            $msiName
        )
        nsis = Find-ArtifactFile -RelativePaths @(
            "bundle/release/$nsisName",
            "bundle/nsis/$nsisName",
            "bundle/$nsisName",
            $nsisName
        )
        manifest = Find-ArtifactFile -RelativePaths @(
            "bundle/release/SHA256sums.txt",
            "bundle/SHA256sums.txt",
            "SHA256sums.txt"
        )
    }
}

function Write-ArtifactEvidence {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][object]$Value
    )

    $path = Join-Path $script:EvidenceRoot ("artifact-" + $Name + ".json")
    Write-JsonFile -Value ([pscustomobject]$Value) -Path $path
    return $path
}

function Invoke-ArtifactGate {
    $metadata = $null
    try {
        $metadata = Get-WorkspaceMetadata
        $script:ExpectedVersion = $metadata.packageVersion
        Set-ReleaseArtifactPaths -Version $script:ExpectedVersion
    } catch {
        Add-Scenario -Name "artifact-version" -Status "FAIL" -Message $_.Exception.Message
        return $false
    }

    $missing = @($script:Artifacts.GetEnumerator() | Where-Object { [string]::IsNullOrWhiteSpace($_.Value) } | ForEach-Object { $_.Key })
    if ($missing.Count -gt 0) {
        $filesEvidence = Write-ArtifactEvidence -Name "files" -Value ([ordered]@{
            missing = $missing
            expectedVersion = $script:ExpectedVersion
            artifactRoot = $script:ArtifactRootPath
        })
        Add-Scenario -Name "artifact-files" -Status "FAIL" -Message ("Artifact release hilang: " + ($missing -join ", ")) -Evidence @($filesEvidence)
        Add-Scenario -Name "artifact-version" -Status "BLOCKED" -Message "Version gate menunggu seluruh artifact tersedia." -Evidence @($filesEvidence)
        return $false
    }

    $filesEvidence = Write-ArtifactEvidence -Name "files" -Value ([ordered]@{
        expectedVersion = $script:ExpectedVersion
        artifactRoot = $script:ArtifactRootPath
        files = [ordered]@{
            executable = $script:Artifacts.executable
            zip = $script:Artifacts.zip
            msi = $script:Artifacts.msi
            nsis = $script:Artifacts.nsis
            manifest = $script:Artifacts.manifest
        }
    })
    Add-Scenario -Name "artifact-files" -Status "PASS" -Message "EXE, ZIP, MSI, NSIS, dan checksum manifest tersedia." -Evidence @($filesEvidence)

    $versions = @($metadata.packageVersion, $metadata.cargoVersion, $metadata.tauriVersion)
    $versionNamesMatch = @(
        (Split-Path -Leaf $script:Artifacts.zip),
        (Split-Path -Leaf $script:Artifacts.msi),
        (Split-Path -Leaf $script:Artifacts.nsis)
    ) | Where-Object { $_ -notlike "*$($script:ExpectedVersion)*" }
    $versionEvidence = Write-ArtifactEvidence -Name "version" -Value ([ordered]@{
        package = $metadata.packageVersion
        cargo = $metadata.cargoVersion
        tauri = $metadata.tauriVersion
        artifactNamesWithoutVersion = $versionNamesMatch
    })
    if ($versions.Count -ne 3 -or @($versions | Select-Object -Unique).Count -ne 1 -or [string]::IsNullOrWhiteSpace($script:ExpectedVersion) -or $versionNamesMatch.Count -gt 0) {
        Add-Scenario -Name "artifact-version" -Status "FAIL" -Message "Versi package/Cargo/Tauri atau nama artifact tidak konsisten." -Evidence @($versionEvidence)
        return $false
    }
    Add-Scenario -Name "artifact-version" -Status "PASS" -Message ("Seluruh metadata dan artifact menggunakan versi {0}." -f $script:ExpectedVersion) -Evidence @($versionEvidence)

    $iconPaths = @($metadata.icons | ForEach-Object {
        $iconPath = Join-Path (Join-Path $script:WorkspaceRoot "src-tauri") ([string]$_)
        Resolve-AbsolutePath $iconPath
    })
    $missingIcons = @($iconPaths | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) })
    $iconEvidence = Write-ArtifactEvidence -Name "icons" -Value ([ordered]@{
        configured = $iconPaths
        missing = $missingIcons
    })
    if ($iconPaths.Count -eq 0 -or $missingIcons.Count -gt 0) {
        Add-Scenario -Name "artifact-icons" -Status "FAIL" -Message "Icon bundle yang dikonfigurasi tidak lengkap." -Evidence @($iconEvidence)
        return $false
    }
    Add-Scenario -Name "artifact-icons" -Status "PASS" -Message ("{0} configured icon files tersedia." -f $iconPaths.Count) -Evidence @($iconEvidence)

    $manifestEntries = @{}
    $invalidManifestLines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in @(Get-Content -LiteralPath $script:Artifacts.manifest)) {
        $trimmed = $line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed)) {
            continue
        }
        $match = [regex]::Match($trimmed, '^(?<hash>[0-9A-Fa-f]{64})\s+\*?(?<name>.+?)\s*$')
        if (-not $match.Success) {
            $invalidManifestLines.Add($trimmed) | Out-Null
            continue
        }
        $name = Split-Path -Leaf ($match.Groups["name"].Value.Trim().Replace("/", [IO.Path]::DirectorySeparatorChar))
        $manifestEntries[$name.ToLowerInvariant()] = $match.Groups["hash"].Value.ToLowerInvariant()
    }

    $checksumTargets = @(
        [pscustomobject]@{ key = "zip"; manifestRequired = $true },
        [pscustomobject]@{ key = "msi"; manifestRequired = $true },
        [pscustomobject]@{ key = "nsis"; manifestRequired = $true },
        [pscustomobject]@{ key = "executable"; manifestRequired = $false }
    )
    $checksumResults = @()
    $checksumFailures = [System.Collections.Generic.List[string]]::new()
    foreach ($target in $checksumTargets) {
        $path = [string]$script:Artifacts[$target.key]
        $name = (Split-Path -Leaf $path).ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $expected = $manifestEntries[$name]
        $matches = $null -ne $expected -and $actual -eq $expected
        if ($target.manifestRequired -and $null -eq $expected) {
            $checksumFailures.Add("$name missing from SHA256sums.txt") | Out-Null
        } elseif ($null -ne $expected -and -not $matches) {
            $checksumFailures.Add("$name checksum mismatch") | Out-Null
        }
        $checksumResults += [pscustomobject][ordered]@{
            asset = $name
            path = $path
            actualSha256 = $actual
            manifestSha256 = $expected
            manifestCompared = $null -ne $expected
            matches = if ($null -ne $expected) { $matches } else { $null }
        }
    }
    $checksumEvidence = Write-ArtifactEvidence -Name "checksum" -Value ([ordered]@{
        manifest = $script:Artifacts.manifest
        invalidLines = $invalidManifestLines
        assets = $checksumResults
    })
    if ($invalidManifestLines.Count -gt 0 -or $checksumFailures.Count -gt 0) {
        $details = @($invalidManifestLines + $checksumFailures)
        Add-Scenario -Name "artifact-checksum" -Status "FAIL" -Message ("Checksum manifest tidak valid: " + ($details -join "; ")) -Evidence @($checksumEvidence)
        return $false
    }
    Add-Scenario -Name "artifact-checksum" -Status "PASS" -Message "Checksum ZIP/MSI/NSIS cocok; hash executable standalone dicatat." -Evidence @($checksumEvidence)

    $zipEvidencePath = Join-Path $script:EvidenceRoot "artifact-zip-contents.json"
    $archive = $null
    try {
        Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop | Out-Null
        $archive = [IO.Compression.ZipFile]::OpenRead($script:Artifacts.zip)
        $entries = @($archive.Entries | ForEach-Object { $_.FullName })
        $unsafeEntries = @($entries | Where-Object {
            $_.StartsWith("/", [StringComparison]::Ordinal) -or
            $_.StartsWith("\\", [StringComparison]::Ordinal) -or
            $_.Split("/", [StringSplitOptions]::RemoveEmptyEntries) -contains ".."
        })
        $launcherEntry = @($archive.Entries | Where-Object { $_.FullName.Equals("wuwaid-launcher.exe", [StringComparison]::OrdinalIgnoreCase) })
        Write-JsonFile -Value ([ordered]@{
            archive = $script:Artifacts.zip
            entries = $entries
            unsafeEntries = $unsafeEntries
            launcherEntry = $launcherEntry.Count -gt 0
        }) -Path $zipEvidencePath
        if ($unsafeEntries.Count -gt 0 -or $launcherEntry.Count -eq 0) {
            throw "ZIP harus berisi wuwaid-launcher.exe dan tidak boleh memiliki path traversal."
        }
        Add-Scenario -Name "artifact-zip-contents" -Status "PASS" -Message "Updater ZIP berisi executable packaged yang aman." -Evidence @($zipEvidencePath)
    } catch {
        if (-not (Test-Path -LiteralPath $zipEvidencePath -PathType Leaf)) {
            Write-JsonFile -Value ([ordered]@{
                archive = $script:Artifacts.zip
                error = $_.Exception.Message
            }) -Path $zipEvidencePath
        }
        Add-Scenario -Name "artifact-zip-contents" -Status "FAIL" -Message $_.Exception.Message -Evidence @($zipEvidencePath)
        return $false
    } finally {
        if ($null -ne $archive) {
            $archive.Dispose()
        }
    }

    return $true
}

function Invoke-CommandGate {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    $cargoExecutable = (Get-Command cargo -ErrorAction SilentlyContinue | Select-Object -First 1).Source
    if ([string]::IsNullOrWhiteSpace($cargoExecutable) -and (Test-Path -LiteralPath (Join-Path $cargoBin "cargo.exe") -PathType Leaf)) {
        $cargoExecutable = Join-Path $cargoBin "cargo.exe"
    }
    if (-not [string]::IsNullOrWhiteSpace($cargoExecutable) -and -not $env:Path.Contains($cargoBin)) {
        $env:Path = $cargoBin + [IO.Path]::PathSeparator + $env:Path
    }

    $commands = @(
        [pscustomobject]@{ name = "command-npm-check"; executable = "npm"; arguments = @("run", "check") },
        [pscustomobject]@{ name = "command-npm-build"; executable = "npm"; arguments = @("run", "build") },
        [pscustomobject]@{ name = "command-cargo-test"; executable = if ([string]::IsNullOrWhiteSpace($cargoExecutable)) { "cargo" } else { $cargoExecutable }; arguments = @("test", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--", "--test-threads=1") },
        [pscustomobject]@{ name = "command-tauri-build"; executable = "npm"; arguments = @("run", "tauri", "--", "build") }
    )
    $allPassed = $true
    foreach ($command in $commands) {
        $logPath = Join-Path $script:EvidenceRoot ($command.name + ".log")
        $output = @()
        $exitCode = 1
        $locationPushed = $false
        try {
            Push-Location -LiteralPath $script:WorkspaceRoot
            $locationPushed = $true
            $output = @(& $command.executable @($command.arguments) 2>&1)
            $exitCode = $LASTEXITCODE
        } catch {
            $output = @($_.Exception.Message)
            $exitCode = 1
        } finally {
            if ($locationPushed) {
                Pop-Location
            }
        }
        $output | Out-File -LiteralPath $logPath -Encoding utf8
        if ($exitCode -eq 0) {
            Add-Scenario -Name $command.name -Status "PASS" -Message (("Command exit code 0: {0} {1}" -f $command.executable, ($command.arguments -join " ")).Trim()) -Evidence @($logPath)
        } else {
            $allPassed = $false
            Add-Scenario -Name $command.name -Status "FAIL" -Message (("Command exit code {0}: {1} {2}" -f $exitCode, $command.executable, ($command.arguments -join " ")).Trim()) -Evidence @($logPath)
        }
    }
    return $allPassed
}

function Assert-Input {
    $script:ArtifactRootPath = Resolve-AbsolutePath $ArtifactRoot
    $script:OutputRootPath = Resolve-AbsolutePath $OutputRoot
    $script:FixtureParentPath = Resolve-AbsolutePath $FixtureRoot

    if (-not (Test-Path -LiteralPath $script:ArtifactRootPath -PathType Container)) {
        throw "ArtifactRoot harus berupa folder yang ada: $script:ArtifactRootPath"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $script:ArtifactRootPath "wuwaid-launcher.exe") -PathType Leaf)) {
        throw "ArtifactRoot tidak memiliki wuwaid-launcher.exe: $script:ArtifactRootPath"
    }
    if (Test-Path -LiteralPath $script:OutputRootPath -PathType Leaf) {
        throw "OutputRoot menunjuk ke file: $script:OutputRootPath"
    }
    if (Test-Path -LiteralPath $script:FixtureParentPath -PathType Leaf) {
        throw "FixtureRoot menunjuk ke file: $script:FixtureParentPath"
    }
    if ([IO.Path]::GetPathRoot($script:FixtureParentPath).Equals($script:FixtureParentPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "FixtureRoot tidak boleh berupa drive root: $script:FixtureParentPath"
    }
    if (Test-PathOverlap -First $script:OutputRootPath -Second $script:FixtureParentPath) {
        throw "OutputRoot dan FixtureRoot tidak boleh overlap."
    }
    if (Test-PathOverlap -First $script:ArtifactRootPath -Second $script:FixtureParentPath) {
        throw "ArtifactRoot dan FixtureRoot tidak boleh overlap."
    }

    if (-not [string]::IsNullOrWhiteSpace($GamePath)) {
        $script:GamePathValue = Resolve-AbsolutePath $GamePath
        if (Test-PathOverlap -First $script:GamePathValue -Second $script:FixtureParentPath) {
            throw "FixtureRoot tidak boleh sama atau berada di dalam/sekitar GamePath."
        }
    }

    if (-not (Test-Path -LiteralPath $script:OutputRootPath)) {
        New-Item -ItemType Directory -Force -Path $script:OutputRootPath | Out-Null
    }
    if (-not (Test-Path -LiteralPath $script:FixtureParentPath)) {
        New-Item -ItemType Directory -Force -Path $script:FixtureParentPath | Out-Null
    }
}

$script:WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$script:Scenarios = [System.Collections.Generic.List[object]]::new()
$script:RunId = "{0}-p{1}" -f [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssfffZ"), $PID
$script:StartedAt = [DateTime]::UtcNow.ToString("o")
$script:RunFixtureRoot = $null
$script:OwnerMarkerPath = $null
$script:ReportPath = $null
$script:EvidenceRoot = $null
$script:Report = [ordered]@{
    schemaVersion = 1
    runId         = $script:RunId
    mode          = $Mode
    commandGateRequested = [bool]$RunCommandGate
    startedAt     = $script:StartedAt
    finishedAt    = $null
    status        = "FAIL"
    artifactRoot  = $null
    fixtureParent = $null
    fixtureRoot   = $null
    ownerMarker   = $null
    baselineSnapshot = $null
    reportPath    = $null
    scenarios     = @()
    cleanup       = $null
}

try {
    Assert-Input

    $script:RunFixtureRoot = Join-Path $script:FixtureParentPath ("wuwaid-release-gate-" + $script:RunId)
    $script:OwnerMarkerPath = Join-Path $script:RunFixtureRoot ".wuwaid-acceptance-owner.json"
    $script:EvidenceRoot = Join-Path $script:OutputRootPath $script:RunId
    $script:ReportPath = Join-Path $script:OutputRootPath ("windows-release-gate-" + $script:RunId + ".json")
    $script:Report.artifactRoot = $script:ArtifactRootPath
    $script:Report.fixtureParent = $script:FixtureParentPath
    $script:Report.fixtureRoot = $script:RunFixtureRoot
    $script:Report.ownerMarker = $script:OwnerMarkerPath
    $script:Report.reportPath = $script:ReportPath

    New-Item -ItemType Directory -Force -Path $script:RunFixtureRoot, $script:EvidenceRoot | Out-Null
    $owner = [ordered]@{
        tool      = "wuwaid-windows-release-gate"
        runId     = $script:RunId
        root      = $script:RunFixtureRoot
        createdAt = [DateTime]::UtcNow.ToString("o")
        processId = $PID
    }
    Write-JsonFile -Value ([pscustomobject]$owner) -Path $script:OwnerMarkerPath
    Add-Scenario -Name "input-validation" -Status "PASS" -Message "Input paths and safety boundaries validated."

    New-FixtureLayout
    $layoutManifestPath = Join-Path $script:EvidenceRoot "fixture-layout.json"
    $layout = [ordered]@{
        root = $script:RunFixtureRoot
        files = @(
            "Client/Binaries/Win64/Client-Win64-Shipping.exe",
            "Client/Saved/Resources/3.0.0/ResManifest"
        )
        directories = @(
            "Client/Content/Paks"
        )
    }
    Write-JsonFile -Value ([pscustomobject]$layout) -Path $layoutManifestPath
    Add-Scenario -Name "fixture-layout" -Status "PASS" -Message "Disposable game fixture created under the run-owned root." -Evidence @($layoutManifestPath, $script:OwnerMarkerPath)

    $baseline = Get-FixtureSnapshot -Root $script:RunFixtureRoot
    $baselinePath = Join-Path $script:EvidenceRoot "baseline-snapshot.json"
    Write-JsonFile -Value ([pscustomobject][ordered]@{
        runId = $script:RunId
        root  = $script:RunFixtureRoot
        files = $baseline
    }) -Path $baselinePath
    $script:Report.baselineSnapshot = $baselinePath
    Add-Scenario -Name "baseline-snapshot" -Status "PASS" -Message ("Baseline captured for {0} files." -f @($baseline).Count) -Evidence @($baselinePath)

    $artifactGatePassed = Invoke-ArtifactGate
    if ($RunCommandGate) {
        if ($artifactGatePassed) {
            Invoke-CommandGate | Out-Null
        } else {
            Add-Scenario -Name "command-gate" -Status "BLOCKED" -Message "Command gate dilewati karena artifact gate gagal." -Evidence @($script:EvidenceRoot)
        }
    }

    if ($TestFailureAfterFixture) {
        throw "Injected failure after fixture and baseline creation for cleanup contract test."
    }

    if ($Mode -in @("manual", "all")) {
        if ([string]::IsNullOrWhiteSpace($GamePath)) {
            Add-Scenario -Name "manual-prerequisites" -Status "BLOCKED" -Message "GamePath belum diberikan; manual release-machine checks belum dapat dijalankan."
        } elseif (-not (Test-Path -LiteralPath $script:GamePathValue -PathType Container)) {
            Add-Scenario -Name "manual-prerequisites" -Status "BLOCKED" -Message "GamePath tidak ditemukan: $script:GamePathValue"
        } else {
            Add-Scenario -Name "manual-prerequisites" -Status "PASS" -Message "GamePath tersedia untuk manual release-machine checks."
        }
    }

    $nonPass = @($script:Scenarios | Where-Object { $_.status -ne "PASS" })
    if ($nonPass.Count -eq 0) {
        try {
            Remove-RunFixtureSafely
            $script:Report.cleanup = [pscustomobject][ordered]@{
                status  = "CLEANED"
                root    = $script:RunFixtureRoot
                message = "Run-owned fixture removed after a fully passing run."
            }
            Add-Scenario -Name "cleanup" -Status "PASS" -Message "Run-owned fixture cleaned safely."
        } catch {
            $script:Report.cleanup = [pscustomobject][ordered]@{
                status  = "PRESERVED"
                root    = $script:RunFixtureRoot
                message = $_.Exception.Message
            }
            Add-Scenario -Name "cleanup" -Status "FAIL" -Message $_.Exception.Message -Evidence @($script:OwnerMarkerPath)
        }
    } else {
        $preserveStatus = if (@($nonPass | Where-Object status -eq "FAIL").Count -gt 0) { "FAIL" } else { "BLOCKED" }
        $script:Report.cleanup = [pscustomobject][ordered]@{
            status  = "PRESERVED"
            root    = $script:RunFixtureRoot
            message = "Fixture dipertahankan karena run memiliki FAIL atau BLOCKED scenario."
        }
        Add-Scenario -Name "cleanup" -Status $preserveStatus -Message $script:Report.cleanup.message -Evidence @($script:OwnerMarkerPath, $script:EvidenceRoot)
    }

    $script:Report.scenarios = @($script:Scenarios)
    $script:Report.status = if (@($script:Scenarios | Where-Object status -eq "FAIL").Count -gt 0) {
        "FAIL"
    } elseif (@($script:Scenarios | Where-Object status -eq "BLOCKED").Count -gt 0) {
        "BLOCKED"
    } else {
        "PASS"
    }
    Write-Report
    Write-Output $script:ReportPath

    if ($script:Report.status -eq "FAIL") {
        exit 1
    }
    if ($script:Report.status -eq "BLOCKED") {
        exit 2
    }
    exit 0
} catch {
    $message = $_.Exception.Message
    if (-not [string]::IsNullOrWhiteSpace($script:RunFixtureRoot) -and (Test-Path -LiteralPath $script:RunFixtureRoot)) {
        $script:Report.cleanup = [pscustomobject][ordered]@{
            status  = "PRESERVED"
            root    = $script:RunFixtureRoot
            message = "Fixture dipertahankan setelah error: $message"
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($script:ReportPath) -and (Test-Path -LiteralPath $script:OutputRootPath)) {
        Add-Scenario -Name "runner-error" -Status "FAIL" -Message $message -Evidence @($script:EvidenceRoot, $script:OwnerMarkerPath)
        $script:Report.scenarios = @($script:Scenarios)
        $script:Report.status = "FAIL"
        try {
            Write-Report
            Write-Output $script:ReportPath
        } catch {
            Write-Error ("Gagal menulis report setelah error: " + $_.Exception.Message)
        }
    }
    Write-Error $message
    exit 1
}
