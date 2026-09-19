[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'out')
)
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$target = Join-Path $repo '.target\glass-dev'
$output = [IO.Path]::GetFullPath($OutputDirectory)
$cargo = (Get-Command cargo.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$previous = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = $target
    & $cargo run --locked --manifest-path $manifest -- --check "--output=$output"
    if ($LASTEXITCODE -ne 0) { throw "glass-check failed with exit code $LASTEXITCODE" }
    Write-Output ("glass-check artifacts: " + $output)
} finally {
    $env:CARGO_TARGET_DIR = $previous
}
