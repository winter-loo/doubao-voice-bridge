[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$target = Join-Path $repo '.target\glass-dev'
$cargo = (Get-Command cargo.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$previous = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = $target
    & $cargo run --locked --manifest-path $manifest
    if ($LASTEXITCODE -ne 0) { throw "glass-playground failed with exit code $LASTEXITCODE" }
} finally {
    $env:CARGO_TARGET_DIR = $previous
}
