[CmdletBinding()]
param([switch]$Compact, [switch]$BuildOnly, [switch]$CompileOnly)
$ErrorActionPreference = 'Stop'

# ASCII for Windows PowerShell 5.1. No Git operations, foreign-window input,
# microphone, global hotkey or driver/settings changes.
$tokens = $null; $parseErrors = $null
$null = [System.Management.Automation.Language.Parser]::ParseFile($PSCommandPath, [ref]$tokens, [ref]$parseErrors)
if (@($parseErrors).Count -gt 0) { throw 'Review launcher syntax check failed.' }
if (-not ('GlassLiveReviewControllerV2' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class GlassLiveReviewControllerV2 {
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", SetLastError=true)] static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
    public static bool Owned(IntPtr h, uint expected) {
        uint actual; GetWindowThreadProcessId(h, out actual);
        return h != IntPtr.Zero && actual == expected;
    }
    static void PostOwned(IntPtr h, uint expected, uint message) {
        if (!Owned(h, expected)) throw new InvalidOperationException("Owned preview window no longer exists.");
        if (!PostMessageW(h,message,new IntPtr(1),IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error());
    }
    public static void SavePair(IntPtr h, uint expected) {
        if (!IsWindowVisible(h)) throw new InvalidOperationException("Preview is no longer visible.");
        PostOwned(h,expected,0x8031);
    }
    public static void Close(IntPtr h, uint expected) {
        PostOwned(h,expected,0x8032);
    }
}
'@
}
if ($CompileOnly) {
    Write-Output 'Review launcher parsed and control helper compiled; no window, capture or process started.'
    $global:LASTEXITCODE = 0
    return
}

$root = $PSScriptRoot
$exe = Join-Path $root 'target\release\GlassLivePreview.exe'
$cargo = (Get-Command cargo.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
if (@(Get-Process | Where-Object { $_.ProcessName -eq 'GlassLivePreview' }).Count -gt 0) {
    throw 'An existing preview is running; it was not changed.'
}
$logs = Join-Path $env:TEMP ('doubao-glass-review-v2-' + [Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $logs)
if (Test-Path -LiteralPath $exe -PathType Leaf) {
    Copy-Item -LiteralPath $exe -Destination (Join-Path $logs 'previous-GlassLivePreview.exe')
}
function Read-SharedText([string]$path) {
    if (-not [IO.File]::Exists($path)) { return '' }
    $stream = [IO.FileStream]::new($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        $reader = [IO.StreamReader]::new($stream,[Text.Encoding]::UTF8)
        try { return [string]$reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
}
function Show-Tail([string]$text) {
    @($text -split '\r?\n' | Where-Object { $_ } | Select-Object -Last 10) | ForEach-Object {
        if ($_.Length -gt 220) { $_.Substring(0,220) } else { $_ }
    }
}
function Invoke-BuildStep([string]$program, [string]$arguments, [string]$name) {
    $stderr = Join-Path $logs ($name + '.stderr.log')
    $process = Start-Process -FilePath $program -ArgumentList $arguments -WorkingDirectory $root -RedirectStandardOutput (Join-Path $logs ($name + '.stdout.log')) -RedirectStandardError $stderr -NoNewWindow -PassThru
    if ($null -eq $process) { throw ('No process returned: ' + $name) }
    $null = $process.Handle
    try {
        $process.WaitForExit()
        if ($null -eq $process.ExitCode -or $process.ExitCode -ne 0) {
            Show-Tail ([string](Read-SharedText $stderr))
            throw ($name + ' failed; logs: ' + $logs)
        }
    } finally { $process.Dispose() }
}
Write-Output ('Building only the independent custom preview; old EXE backed up. Logs: ' + $logs)
Invoke-BuildStep $cargo ('build --release --locked -j 2 --manifest-path "{0}" --target-dir "{1}"' -f (Join-Path $root 'Cargo.toml'),(Join-Path $root 'target')) 'build'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Built preview executable is missing.' }
$hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
Invoke-BuildStep $exe '--self-test' 'gpu-self-test'
$result = [ordered]@{ build_completed=$true; sha256=$hash; logs=$logs; gpu_self_test='passed on WARP; no desktop capture'; capture_started=$false; saved_pair=$false; cancelled=$false; error=$null; exit_code=$null; exit_reason=$null; forced_cleanup=$false; production_client_changed=$false }
if ($BuildOnly) {
    $result | ConvertTo-Json -Compress
    $global:LASTEXITCODE = 0
    return
}

Write-Host "`n[USER ACTION REQUIRED] Open Notepad with non-private test text. START authorizes local GPU capture of the primary SDR display for this temporary preview, up to 30 minutes. Nothing is uploaded or saved automatically. This preview is excluded from ordinary system screenshots/recording. One later SAVE will explicitly save TWO small same-frame GPU composites, light and dark, then close this preview. These PNGs are NOT screenshots of final DWM presentation. No microphone or production client changes."
$answer = Read-Host 'Type START to open the review preview; anything else cancels'
if ($answer -cne 'START') {
    $result.cancelled = $true
    $result | ConvertTo-Json -Compress
    $global:LASTEXITCODE = 0
    return
}
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $hash) { throw 'Executable changed during confirmation.' }
if (@(Get-Process | Where-Object { $_.ProcessName -eq 'GlassLivePreview' }).Count -gt 0) { throw 'Another preview appeared; no duplicate was started.' }
$snapshots = Join-Path $logs 'snapshots'
$stderr = Join-Path $logs 'preview.stderr.log'
$arguments = '--allow-desktop-capture --review-mode --theme=light --seconds=1800 "--snapshot-dir={0}"' -f $snapshots
if ($Compact) { $arguments += ' --compact' }
$preview = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $root -RedirectStandardOutput (Join-Path $logs 'preview.stdout.log') -RedirectStandardError $stderr -NoNewWindow -PassThru
if ($null -eq $preview) { throw 'No preview process returned.' }
$null = $preview.Handle
$hwnd = [IntPtr]::Zero
try {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $pattern = '\[glass-live\] READY pid=' + $preview.Id + '; hwnd=0x([0-9A-Fa-f]+);[^\r\n]*review=true'
    while ($timer.Elapsed.TotalSeconds -lt 30 -and -not $preview.HasExited) {
        $text = [string](Read-SharedText $stderr)
        $match = [regex]::Match($text,$pattern)
        if ($match.Success -and $text.Contains('[glass-review] control-v1;')) {
            $hwnd = [IntPtr]([Convert]::ToInt64($match.Groups[1].Value,16))
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if ($hwnd -eq [IntPtr]::Zero -or $preview.HasExited) { throw 'Review preview did not remain running through READY; see exit log below.' }
    $result.capture_started = $true
    Write-Output ('Review READY; PID=' + $preview.Id + '. Mouse right/middle clicks do not close or save in this mode.')
    Write-Host "`n[USER ACTION REQUIRED] LEFT-drag the capsule over Notepad text. Keep the terminal from covering that area. You may LEFT-click to inspect either theme and scroll Notepad to observe updates. When ready, return here and type SAVE ONCE. Both themes will be generated from the same cached background and animation phase, with no second click or repositioning. Type anything else to cancel and close only this preview."
    $answer = Read-Host 'Type SAVE for both PNGs; anything else cancels'
    if ($answer -cne 'SAVE') {
        $result.cancelled = $true
    } else {
        if ($preview.HasExited) { throw 'Preview exited before SAVE; its observed exit reason and process exit code are reported below.' }
        [GlassLiveReviewControllerV2]::SavePair($hwnd,[uint32]$preview.Id)
        $timer.Restart()
        $pairPath = Join-Path $snapshots 'pair.json'
        while ($timer.Elapsed.TotalSeconds -lt 15 -and -not (Test-Path -LiteralPath $pairPath -PathType Leaf)) {
            if ($preview.HasExited) { break }
            Start-Sleep -Milliseconds 100
        }
        if (-not (Test-Path -LiteralPath $pairPath -PathType Leaf)) { throw 'No completed pair acknowledgement; partial files, if any, are retained, not accepted.' }
        $pair = Get-Content -LiteralPath $pairPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if (-not $pair.complete -or $pair.request -ne 1 -or $pair.pid -ne $preview.Id -or $pair.light -cne 'snapshot-0001.png' -or $pair.dark -cne 'snapshot-0002.png') { throw 'Pair acknowledgement identity or filenames did not match.' }
        Add-Type -AssemblyName System.Drawing
        foreach ($name in @($pair.light,$pair.dark)) {
            $path = Join-Path $snapshots $name
            $image = [Drawing.Bitmap]::new($path)
            try {
                if ($image.Width -ne ([int]$pair.lens[2]+2*[int]$pair.padding) -or $image.Height -ne ([int]$pair.lens[3]+2*[int]$pair.padding)) { throw 'Saved PNG dimensions do not match the acknowledged local crop.' }
            } finally { $image.Dispose() }
        }
        $result.saved_pair = $true
        $result.snapshots = $snapshots
        $result.pair = $pair
    }
} catch {
    $result.error = [string]$_.Exception.Message
} finally {
    try {
        if (-not $preview.HasExited) {
            if ([GlassLiveReviewControllerV2]::Owned($hwnd,[uint32]$preview.Id)) {
                try { [GlassLiveReviewControllerV2]::Close($hwnd,[uint32]$preview.Id) } catch { Write-Output 'Owned controller-close failed; cleanup will check process termination.' }
            }
            if (-not $preview.WaitForExit(3000)) {
                $result.forced_cleanup = $true
                $preview.Kill()
                if (-not $preview.WaitForExit(5000)) { $result.error = 'The owned preview did not terminate after cleanup.' }
            }
        }
        $result.preview_stopped = $preview.HasExited
        if ($preview.HasExited) { $result.exit_code = $preview.ExitCode }
        $runtimeLog = [string](Read-SharedText $stderr)
        $reasons = [regex]::Matches($runtimeLog,'\[glass-live\] stopped; reason=([^;]+);')
        if ($reasons.Count -gt 0) { $result.exit_reason = $reasons[$reasons.Count-1].Groups[1].Value }
        if ($null -eq $result.error -and $result.exit_code -ne 0) { $result.error = 'Preview process returned a nonzero exit status; inspect runtime log.' }
        Write-Output '=== Preview lifecycle log ==='
        Show-Tail $runtimeLog
        $json = $result | ConvertTo-Json -Depth 5 -Compress
        [IO.File]::WriteAllText((Join-Path $logs 'review-result.json'),$json)
        Write-Output $json
        if ($result.saved_pair) {
            Start-Process -FilePath explorer.exe -ArgumentList ('"{0}"' -f $snapshots) | Out-Null
            Write-Output 'Folder opened. Attach snapshot-0001.png (light) and snapshot-0002.png (dark). Source is a GPU composite, not final desktop presentation.'
        }
    } finally { $preview.Dispose() }
}
if ($null -ne $result.error -or $result.forced_cleanup) { $global:LASTEXITCODE = 1 } else { $global:LASTEXITCODE = 0 }
