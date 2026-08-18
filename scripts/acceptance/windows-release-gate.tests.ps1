$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Invoke-Runner {
    param(
        [Parameter(Mandatory = $true)][string]$Mode,
        [Parameter(Mandatory = $true)][string]$ArtifactRoot,
        [Parameter(Mandatory = $true)][string]$OutputRoot,
        [Parameter(Mandatory = $true)][string]$FixtureRoot,
        [string]$GamePath,
        [switch]$TestFailureAfterFixture
    )

    $arguments = @(
        "-Mode", $Mode,
        "-ArtifactRoot", $ArtifactRoot,
        "-OutputRoot", $OutputRoot,
        "-FixtureRoot", $FixtureRoot
    )
    if (-not [string]::IsNullOrWhiteSpace($GamePath)) {
        $arguments += @("-GamePath", $GamePath)
    }
    if ($TestFailureAfterFixture) {
        $arguments += "-TestFailureAfterFixture"
    }

    $output = @(& $script:Pwsh -NoProfile -File $script:Runner @arguments 2>&1)
    $exitCode = $LASTEXITCODE
    $reportPath = @($output | ForEach-Object {
        $candidate = $_.ToString().Trim()
        if (Test-Path -LiteralPath $candidate -PathType Leaf -ErrorAction SilentlyContinue) {
            $candidate
        }
    } | Select-Object -Last 1)

    return [pscustomobject]@{
        exitCode   = $exitCode
        output     = $output
        reportPath = if ($reportPath.Count -eq 1) { $reportPath[0] } else { $null }
    }
}

function New-CaseDirectory {
    param([Parameter(Mandatory = $true)][string]$Name)

    $caseRoot = Join-Path $script:TestRoot $Name
    New-Item -ItemType Directory -Force -Path $caseRoot | Out-Null
    return [pscustomobject][ordered]@{
        root        = $caseRoot
        artifact    = Join-Path $caseRoot "artifacts"
        output      = Join-Path $caseRoot "evidence"
        fixture     = Join-Path $caseRoot "fixtures"
    }
}

function Initialize-Artifact {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [bool]$IncludeExecutable = $true
    )

    New-Item -ItemType Directory -Force -Path $Path | Out-Null
    if ($IncludeExecutable) {
        $executable = Join-Path $Path "WuwaIDLauncher.exe"
        Set-Content -LiteralPath $executable -Value "fixture executable" -NoNewline

        $zip = Join-Path $Path "WuwaIDLauncher-v2.8.0.zip"
        Compress-Archive -LiteralPath $executable -DestinationPath $zip

        $manifestLines = @($zip | ForEach-Object {
            $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
            "{0} *{1}" -f $hash, (Split-Path -Leaf $_)
        })
        Set-Content -LiteralPath (Join-Path $Path "SHA256sums.txt") -Value $manifestLines
    }
}

function Set-UnsafeArtifactZip {
    param([Parameter(Mandatory = $true)][string]$Path)

    Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop | Out-Null
    Remove-Item -LiteralPath $Path -Force
    $archive = [IO.Compression.ZipFile]::Open($Path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entryName in @("../escape.txt", "WuwaIDLauncher.exe")) {
            $entry = $archive.CreateEntry($entryName)
            $writer = [IO.StreamWriter]::new($entry.Open())
            try {
                $writer.Write("fixture")
            } finally {
                $writer.Dispose()
            }
        }
    } finally {
        $archive.Dispose()
    }
}

function Read-Report {
    param([Parameter(Mandatory = $true)][string]$Path)

    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "Runner report path does not exist."
    return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
}

function Assert-WorkflowContract {
    $workflowRoot = Join-Path $PSScriptRoot "..\..\.github\workflows"
    $ciPath = Join-Path $workflowRoot "ci.yml"
    $releasePath = Join-Path $workflowRoot "release.yml"
    Assert-True (Test-Path -LiteralPath $ciPath -PathType Leaf) "Professional CI workflow is required."
    Assert-True (Test-Path -LiteralPath $releasePath -PathType Leaf) "Professional release workflow is required."

    $ci = Get-Content -Raw -LiteralPath $ciPath
    $release = Get-Content -Raw -LiteralPath $releasePath
    $allWorkflows = @(Get-ChildItem -LiteralPath $workflowRoot -Filter "*.yml" -File | Get-Content -Raw) -join "`n"

    Assert-True ($ci -match "permissions:\s*\r?\n\s+contents:\s+read") "CI must use read-only contents permission."
    Assert-True ($release -match "permissions:\s*\r?\n\s+contents:\s+write") "Release workflow must use contents write permission."
    Assert-True ($release -match "tags:" -and $release -match "v\*\.\*\.\*") "Release workflow must trigger on semantic version tags."
    Assert-True ($release -match "workflow_dispatch:") "Release workflow must support manual dispatch."
    Assert-True ($allWorkflows -notmatch "tauri-action|MSI|NSIS|\\.msi|nsis") "Workflows must not build installer bundles."
    Assert-True ($allWorkflows -match "npm ci") "Workflows must use reproducible npm ci installs."
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("wuwaid-release-gate-test-" + [guid]::NewGuid().ToString("N"))
$runner = Join-Path $PSScriptRoot "windows-release-gate.ps1"

try {
    $script:TestRoot = $testRoot
    $script:Runner = $runner
    $script:Pwsh = (Get-Command pwsh -ErrorAction Stop).Source
    Assert-WorkflowContract

    $successCase = New-CaseDirectory -Name "success"
    Initialize-Artifact -Path $successCase.artifact
    New-Item -ItemType Directory -Force -Path $successCase.fixture | Out-Null
    Set-Content -LiteralPath (Join-Path $successCase.fixture "foreign-file.txt") -Value "keep me" -NoNewline
    $successResult = Invoke-Runner -Mode automated -ArtifactRoot $successCase.artifact -OutputRoot $successCase.output -FixtureRoot $successCase.fixture
    Assert-True ($successResult.exitCode -eq 0) ("Runner exited with code $($successResult.exitCode). Output: " + ($successResult.output -join [Environment]::NewLine))
    Assert-True ([IO.Path]::IsPathRooted($successResult.reportPath)) "Runner must return an absolute report path."
    $report = Read-Report -Path $successResult.reportPath
    Assert-True (-not [string]::IsNullOrWhiteSpace($report.runId)) "runId is required."
    Assert-True (-not [string]::IsNullOrWhiteSpace($report.startedAt)) "startedAt is required."
    Assert-True (-not [string]::IsNullOrWhiteSpace($report.finishedAt)) "finishedAt is required."
    Assert-True ($null -ne $report.scenarios -and @($report.scenarios).Count -gt 0) "scenarios are required."
    Assert-True (@($report.scenarios | Where-Object { $_.status -notin @("PASS", "FAIL", "BLOCKED") }).Count -eq 0) "Unknown scenario status."
    Assert-True (@($report.scenarios | Where-Object { [string]::IsNullOrWhiteSpace($_.name) -or [string]::IsNullOrWhiteSpace($_.startedAt) -or [string]::IsNullOrWhiteSpace($_.finishedAt) }).Count -eq 0) "Every scenario needs name and timestamps."
    Assert-True ($report.status -eq "PASS") "A valid automated run must pass."
    Assert-True (@($report.scenarios | Where-Object { $_.name -eq "artifact-version" -and $_.status -eq "PASS" }).Count -eq 1) "Artifact version gate must pass."
    Assert-True (@($report.scenarios | Where-Object { $_.name -eq "artifact-icons" -and $_.status -eq "PASS" }).Count -eq 1) "Artifact icon gate must pass."
    Assert-True (@($report.scenarios | Where-Object { $_.name -eq "artifact-checksum" -and $_.status -eq "PASS" }).Count -eq 1) "Artifact checksum gate must pass."
    Assert-True (@($report.scenarios | Where-Object { $_.name -eq "artifact-zip-contents" -and $_.status -eq "PASS" }).Count -eq 1) "Artifact ZIP gate must pass."
    Assert-True ($report.cleanup.status -eq "CLEANED") "A passing run must clean its owned fixture."
    Assert-True (-not (Test-Path -LiteralPath $report.fixtureRoot)) "Owned fixture must be removed after a passing run."
    Assert-True (Test-Path -LiteralPath (Join-Path $successCase.fixture "foreign-file.txt") -PathType Leaf) "Foreign file must survive cleanup."
    Assert-True (Test-Path -LiteralPath $report.baselineSnapshot -PathType Leaf) "Baseline snapshot evidence must survive cleanup."
    $baseline = Read-Report -Path $report.baselineSnapshot
    Assert-True (@($baseline.files).Count -ge 3) "Baseline must include owner marker and fixture files."

    $blockedCase = New-CaseDirectory -Name "blocked"
    Initialize-Artifact -Path $blockedCase.artifact
    New-Item -ItemType Directory -Force -Path $blockedCase.fixture | Out-Null
    Set-Content -LiteralPath (Join-Path $blockedCase.fixture "foreign-file.txt") -Value "keep me too" -NoNewline
    $blockedResult = Invoke-Runner -Mode manual -ArtifactRoot $blockedCase.artifact -OutputRoot $blockedCase.output -FixtureRoot $blockedCase.fixture
    Assert-True ($blockedResult.exitCode -eq 2) "A blocked manual prerequisite must return exit code 2."
    $blockedReport = Read-Report -Path $blockedResult.reportPath
    Assert-True ($blockedReport.status -eq "BLOCKED") "Missing manual GamePath must be BLOCKED."
    Assert-True ($blockedReport.cleanup.status -eq "PRESERVED") "A blocked run must preserve its fixture."
    Assert-True (Test-Path -LiteralPath $blockedReport.fixtureRoot -PathType Container) "Blocked fixture must remain available for diagnosis."
    Assert-True (Test-Path -LiteralPath $blockedReport.ownerMarker -PathType Leaf) "Blocked run owner marker must remain available."
    Assert-True (Test-Path -LiteralPath (Join-Path $blockedCase.fixture "foreign-file.txt") -PathType Leaf) "Foreign file must survive blocked cleanup."

    $failureCase = New-CaseDirectory -Name "failure"
    Initialize-Artifact -Path $failureCase.artifact
    New-Item -ItemType Directory -Force -Path $failureCase.fixture | Out-Null
    Set-Content -LiteralPath (Join-Path $failureCase.fixture "foreign-file.txt") -Value "keep me after failure" -NoNewline
    $failureResult = Invoke-Runner -Mode automated -ArtifactRoot $failureCase.artifact -OutputRoot $failureCase.output -FixtureRoot $failureCase.fixture -TestFailureAfterFixture
    Assert-True ($failureResult.exitCode -eq 1) "A forced post-fixture failure must return exit code 1."
    $failureReport = Read-Report -Path $failureResult.reportPath
    Assert-True ($failureReport.status -eq "FAIL") "Forced post-fixture failure must be reported as FAIL."
    Assert-True ($failureReport.cleanup.status -eq "PRESERVED") "A failed run must preserve its fixture."
    Assert-True (Test-Path -LiteralPath $failureReport.fixtureRoot -PathType Container) "Failed fixture must remain available for diagnosis."
    Assert-True (Test-Path -LiteralPath $failureReport.ownerMarker -PathType Leaf) "Failed owner marker must remain available."
    Assert-True (@($failureReport.scenarios | Where-Object { $_.name -eq "runner-error" -and $_.status -eq "FAIL" }).Count -eq 1) "Failure evidence scenario is required."
    Assert-True (Test-Path -LiteralPath (Join-Path $failureCase.fixture "foreign-file.txt") -PathType Leaf) "Foreign file must survive failed cleanup."

    $tamperedCase = New-CaseDirectory -Name "tampered-checksum"
    Initialize-Artifact -Path $tamperedCase.artifact
    $tamperedManifest = Join-Path $tamperedCase.artifact "SHA256sums.txt"
    $tamperedLines = @(Get-Content -LiteralPath $tamperedManifest)
    $tamperedParts = $tamperedLines[0] -split "\s+", 2
    $tamperedLines[0] = "{0} {1}" -f (("0" * 64) -join ""), $tamperedParts[1]
    Set-Content -LiteralPath $tamperedManifest -Value $tamperedLines
    $tamperedResult = Invoke-Runner -Mode automated -ArtifactRoot $tamperedCase.artifact -OutputRoot $tamperedCase.output -FixtureRoot $tamperedCase.fixture
    Assert-True ($tamperedResult.exitCode -eq 1) "Tampered checksum must fail the automated gate."
    $tamperedReport = Read-Report -Path $tamperedResult.reportPath
    Assert-True (@($tamperedReport.scenarios | Where-Object { $_.name -eq "artifact-checksum" -and $_.status -eq "FAIL" }).Count -eq 1) "Tampered checksum must never be reported as PASS."
    Assert-True ($tamperedReport.cleanup.status -eq "PRESERVED") "Tampered checksum evidence must preserve the fixture."

    $invalidZipCase = New-CaseDirectory -Name "invalid-zip"
    Initialize-Artifact -Path $invalidZipCase.artifact
    $invalidZip = Join-Path $invalidZipCase.artifact "WuwaIDLauncher-v2.8.0.zip"
    Set-UnsafeArtifactZip -Path $invalidZip
    $invalidZipManifest = Join-Path $invalidZipCase.artifact "SHA256sums.txt"
    $invalidZipLines = @(Get-Content -LiteralPath $invalidZipManifest)
    $invalidZipParts = $invalidZipLines[0] -split "\s+", 2
    $invalidZipHash = (Get-FileHash -LiteralPath $invalidZip -Algorithm SHA256).Hash.ToLowerInvariant()
    $invalidZipLines[0] = "{0} {1}" -f $invalidZipHash, $invalidZipParts[1]
    Set-Content -LiteralPath $invalidZipManifest -Value $invalidZipLines
    $invalidZipResult = Invoke-Runner -Mode automated -ArtifactRoot $invalidZipCase.artifact -OutputRoot $invalidZipCase.output -FixtureRoot $invalidZipCase.fixture
    Assert-True ($invalidZipResult.exitCode -eq 1) "Unsafe ZIP must fail the artifact gate."
    $invalidZipReport = Read-Report -Path $invalidZipResult.reportPath
    Assert-True (@($invalidZipReport.scenarios | Where-Object { $_.name -eq "artifact-zip-contents" -and $_.status -eq "FAIL" }).Count -eq 1) "Unsafe ZIP must never be reported as PASS."

    $missingArtifactCase = New-CaseDirectory -Name "missing-artifact"
    Initialize-Artifact -Path $missingArtifactCase.artifact -IncludeExecutable $false
    $missingArtifactResult = Invoke-Runner -Mode automated -ArtifactRoot $missingArtifactCase.artifact -OutputRoot $missingArtifactCase.output -FixtureRoot $missingArtifactCase.fixture
    Assert-True ($missingArtifactResult.exitCode -ne 0) "Missing launcher artifact must be rejected."
    Assert-True (-not (Test-Path -LiteralPath $missingArtifactCase.fixture)) "Input validation must happen before fixture creation."

    $invalidModeCase = New-CaseDirectory -Name "invalid-mode"
    Initialize-Artifact -Path $invalidModeCase.artifact
    $invalidModeResult = Invoke-Runner -Mode unsupported -ArtifactRoot $invalidModeCase.artifact -OutputRoot $invalidModeCase.output -FixtureRoot $invalidModeCase.fixture
    Assert-True ($invalidModeResult.exitCode -ne 0) "Unsupported mode must be rejected."
    Assert-True (-not (Test-Path -LiteralPath $invalidModeCase.fixture)) "Unsupported mode must not create a fixture."

    $conflictCase = New-CaseDirectory -Name "game-conflict"
    Initialize-Artifact -Path $conflictCase.artifact
    New-Item -ItemType Directory -Force -Path $conflictCase.fixture | Out-Null
    $conflictResult = Invoke-Runner -Mode automated -ArtifactRoot $conflictCase.artifact -OutputRoot $conflictCase.output -FixtureRoot $conflictCase.fixture -GamePath $conflictCase.fixture
    Assert-True ($conflictResult.exitCode -ne 0) "FixtureRoot equal to GamePath must be rejected."
    Assert-True (@(Get-ChildItem -LiteralPath $conflictCase.fixture -Force).Count -eq 0) "GamePath conflict must not mutate the fixture root."

    Write-Output "PASS: runner contract, fixture safety, snapshot, cleanup, and evidence"
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
