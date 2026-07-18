param(
    [string]$Configuration = "Release",
    [string]$Runtime = "win-x64",
    [string]$OutputRoot = "publish/benchmark"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$project = Join-Path $repo "WuwaIDLauncher.csproj"

foreach ($variant in @(
    @{ Name = "compressed"; Compression = "true" },
    @{ Name = "uncompressed"; Compression = "false" }
)) {
    $name = $variant.Name
    $publishDir = Join-Path $repo (Join-Path $OutputRoot $name)
    $objDir = Join-Path $repo "obj/benchmark/$name/"
    $binDir = Join-Path $repo "bin/benchmark/$name/"
    & dotnet publish $project -c $Configuration -r $Runtime `
        "-p:EnableCompressionInSingleFile=$($variant.Compression)" `
        "-p:IntermediateOutputPath=$objDir" `
        "-p:OutputPath=$binDir" `
        -o $publishDir
    if ($LASTEXITCODE -ne 0) { throw "Build $name gagal." }
}
