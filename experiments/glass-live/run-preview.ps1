[CmdletBinding()]
param([switch]$BuildOnly, [switch]$CompileOnly, [switch]$Compact)
$ErrorActionPreference = 'Stop'

# This script never updates Git or touches the native/production binaries.
# All text is ASCII so Windows PowerShell 5.1 reads it without a BOM requirement.
if ($CompileOnly) {
    $tokens = $null; $errors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($PSCommandPath, [ref]$tokens, [ref]$errors)
    if (@($errors).Count -gt 0) { throw 'Launcher PowerShell syntax check failed.' }
    Write-Output 'Launcher parsed; no build, process, capture or file creation.'
    return
}

$root = $PSScriptRoot
$manifest = Join-Path $root 'Cargo.toml'
$target = Join-Path $root 'target'
$exe = Join-Path $target 'release\GlassLivePreview.exe'
$cargo = (Get-Command cargo.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
if (@(Get-Process | Where-Object { $_.ProcessName -eq 'GlassLivePreview' }).Count -gt 0) {
    throw 'An existing custom preview is running; it was not stopped.'
}
$logs = Join-Path $env:TEMP ('doubao-custom-live-' + [Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $logs)

function Invoke-Logged([string]$program, [string]$arguments, [string]$stem) {
    $p = Start-Process -FilePath $program -ArgumentList $arguments -WorkingDirectory $root -RedirectStandardOutput (Join-Path $logs ($stem + '.stdout.log')) -RedirectStandardError (Join-Path $logs ($stem + '.stderr.log')) -NoNewWindow -PassThru
    if ($null -eq $p) { throw ('No process returned: ' + $stem) }
    $null = $p.Handle
    try {
        $p.WaitForExit()
        if ($null -eq $p.ExitCode -or $p.ExitCode -ne 0) {
            Get-Content -LiteralPath (Join-Path $logs ($stem + '.stderr.log')) -Encoding UTF8 -Tail 12 | ForEach-Object {
                if ($_.Length -gt 220) { $_.Substring(0,220) } else { $_ }
            }
            throw ($stem + ' failed. Full logs: ' + $logs)
        }
    } finally { $p.Dispose() }
}
function Read-SharedLog([string]$path) {
    if (-not [IO.File]::Exists($path)) { return '' }
    $file = [IO.FileStream]::new($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        $reader = [IO.StreamReader]::new($file, [Text.Encoding]::UTF8)
        try { return [string]$reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $file.Dispose() }
}

Write-Output ('Building only GlassLivePreview (no GPUI dependency). Logs: ' + $logs)
Invoke-Logged $cargo ('build --release --locked -j 2 --manifest-path "{0}" --target-dir "{1}"' -f $manifest,$target) 'build'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Built preview is missing.' }
$hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
Invoke-Logged $exe ('--self-test "--snapshot-dir={0}"' -f (Join-Path $logs 'synthetic-gpu-fixtures')) 'self-test'
$result = [ordered]@{ build_completed=$true; gpu_self_test='passed on WARP, no desktop capture'; executable=$exe; sha256=$hash; logs=$logs; preview_started=$false; production_client_changed=$false }
if ($BuildOnly) {
    $result | ConvertTo-Json -Compress
    $global:LASTEXITCODE = 0
    return
}

Write-Host "`n[USER ACTION REQUIRED] Open Notepad with non-private test text. This TEMPORARY preview will capture your PRIMARY SDR display locally on GPU; it does not upload or continuously save screen pixels. Its window will be excluded from system screenshots/recorders to prevent recursion. Middle-clicking it explicitly saves ONE small local GPU-composited PNG. No microphone, global hotkeys or production client changes."
Write-Host 'Controls: LEFT drag moves; LEFT click switches light/dark; MIDDLE click saves; RIGHT click closes. It also exits after 10 minutes.'
$answer = Read-Host 'Type START to authorize and show the real-time preview, or anything else to cancel'
if ($answer -cne 'START') {
    $result | ConvertTo-Json -Compress
    Write-Output 'Cancelled before any desktop capture. Build and synthetic fixtures are retained.'
    $global:LASTEXITCODE = 0
    return
}
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $hash) { throw 'Executable changed during confirmation.' }
$snapshots = Join-Path $logs 'snapshots'
$arguments = '--allow-desktop-capture --theme=light --seconds=600 "--snapshot-dir={0}"' -f $snapshots
if ($Compact) { $arguments += ' --compact' }
$stderr = Join-Path $logs 'preview.stderr.log'
$preview = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $root -RedirectStandardOutput (Join-Path $logs 'preview.stdout.log') -RedirectStandardError $stderr -NoNewWindow -PassThru
if ($null -eq $preview) { throw 'Preview process was not returned.' }
$null = $preview.Handle
try {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($timer.Elapsed.TotalSeconds -lt 30 -and -not $preview.HasExited) {
        $text = [string](Read-SharedLog $stderr)
        if ($text.Contains('[glass-live] READY')) { $ready = $true; break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ready -or $preview.HasExited) {
        if (-not $preview.HasExited) { $preview.Kill(); $null = $preview.WaitForExit(5000) }
        $text = [string](Read-SharedLog $stderr)
        if ($text.Length -gt 1600) { $text = $text.Substring($text.Length-1600) }
        Write-Output $text
        throw ('The owned test preview did not reach READY. Logs: ' + $logs)
    }
    $result.preview_started = $true
    $result.pid = $preview.Id
    $result.snapshots = $snapshots
    $result.capture_exclusion = 'only this preview; also affects external system screen capture'
    $result.visual_acceptance = 'not evaluated; READY is initialization acknowledgement'
    $json = $result | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $logs 'launcher-result.json'),$json)
    Write-Output $json
    Write-Output 'Preview is running. Move it over Notepad text; click to compare themes. Right-click closes only the preview.'
    $global:LASTEXITCODE = 0
} finally { $preview.Dispose() }
