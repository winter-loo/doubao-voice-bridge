#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedCommit,
    [Parameter(Mandatory=$true)][string]$BuildRoot,
    [Parameter(Mandatory=$true)][string]$ProductionExe,
    [ValidateRange(1,8)][int]$Jobs = 2,
    [switch]$AttachEvidence
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

# Run in a separate powershell.exe -NoProfile process. Source is authored and
# committed on GitHub; this script does NOT fetch, edit, reset or clean source.
# Only explicitly allowlisted OFFSCREEN tests run. Never run the whole desktop
# suite, spoof CI authorization, launch normal UI, or install the resulting EXE.
function Full-Path([string]$value) { [IO.Path]::GetFullPath($value).TrimEnd([char]'\') }
function Is-Within([string]$child, [string]$parent) {
    return $child.Equals($parent, [StringComparison]::OrdinalIgnoreCase) -or $child.StartsWith($parent + '\', [StringComparison]::OrdinalIgnoreCase)
}
function Reject-ReparseAncestors([string]$path) {
    $at = $path
    while (-not [string]::IsNullOrWhiteSpace($at)) {
        if (Test-Path -LiteralPath $at) {
            $item = Get-Item -LiteralPath $at -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Build/output path has a reparse-point ancestor; refusing ambiguous isolation.' }
        }
        $parent = [IO.Path]::GetDirectoryName($at)
        if ($parent -eq $at) { break }
        $at = $parent
    }
}
function Hash([string]$path) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Save-Json([string]$path, $value) {
    [IO.File]::WriteAllText($path, (ConvertTo-Json -InputObject $value -Depth 8), [Text.UTF8Encoding]::new($false))
}
function Quote-Arg([string]$value) {
    if ($value.IndexOfAny([char[]]@('"', "`r", "`n")) -ge 0 -or $value.EndsWith('\')) { throw 'Unsupported command argument.' }
    return '"' + $value + '"'
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'This build requires the existing Windows/MSVC toolchain.' }
$repo = Full-Path (Join-Path $PSScriptRoot '..\..')
$build = Full-Path $BuildRoot
$production = Full-Path $ProductionExe
$git = (Get-Command git.exe -CommandType Application | Select-Object -First 1).Source
$cargo = (Get-Command cargo.exe -CommandType Application | Select-Object -First 1).Source
function Git-Text([string[]]$arguments) {
    $text = (& $git -C $repo @arguments | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Git metadata check failed; source was not modified.' }
    return $text
}
function Check-Source {
    if ((Git-Text @('rev-parse','HEAD')) -cne $ExpectedCommit) { throw 'Checkout changed; synchronize the exact reviewed commit first.' }
    if ((Git-Text @('branch','--show-current')) -cne 'feat/glass-custom-material') { throw 'Not the custom worktree branch.' }
    if ((Git-Text @('status','--porcelain','--untracked-files=normal')).Length -ne 0) { throw 'Worktree has local changes or untracked source; nothing will be cleaned or overwritten.' }
}
Check-Source
foreach ($line in ((Git-Text @('worktree','list','--porcelain')) -split '\r?\n')) {
    if ($line.StartsWith('worktree ')) {
        $tree = Full-Path ($line.Substring(9).Replace('/','\'))
        if ((Is-Within $build $tree) -or (Is-Within $tree $build)) { throw 'Build root must be outside and separate from every registered worktree.' }
    }
}
if (Is-Within $production $build) { throw 'Build root overlaps the installed client.' }
if (-not (Test-Path -LiteralPath $production -PathType Leaf)) { throw 'Installed client path is unavailable; nothing built or replaced.' }
Reject-ReparseAncestors $build
$productionBefore = Hash $production
$reference = 'experiments/glass-live/src/liquid_reference.hlsl'
if ((Git-Text @('hash-object','--', $reference)) -cne '4e72b8fbb6b8db4ac6f8d426a26d2d368fb27692') { throw 'Frozen optical reference differs from the reviewed baseline.' }
[void][IO.Directory]::CreateDirectory($build)
# Exclusive lock protects this persistent cache from a second local build job.
$lock = [IO.FileStream]::new((Join-Path $build 'pipeline.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
$run = Join-Path $build ('r-' + $ExpectedCommit.Substring(0,7) + '-' + [Guid]::NewGuid().ToString('N').Substring(0,6))
$review = Join-Path $run 'review'
$logs = Join-Path $review 'logs'
$frames = Join-Path $review 'frames'
$product = Join-Path $run 'product'
$target = Join-Path $build 'target'
$environmentBefore = @{}
$steps = New-Object 'System.Collections.Generic.List[object]'
$verifiedTests = New-Object 'System.Collections.Generic.List[string]'
$result = [ordered]@{
    completed=$false; commit=$ExpectedCommit; material='LENS_ADAPTIVE_2'
    build_root=$build; run_directory=$run; executable=$null; executable_sha256=$null
    formal_self_test='not-run'; source_unchanged=$false; production_executable_unchanged=$false
    offscreen_tests=@(); steps=@(); error=$null
    native_desktop_validation='pending after previous CI fixture failure; deliberately NOT executed on this desktop'
    acceptance='offscreen/build evidence only; not final native-window, visual, latency or voice acceptance'
    desktop_capture_started=$false; microphone_started=$false; normal_client_started=$false
    installed_client_replaced=$false; startup_or_voice_settings_written=$false
    dependency_downloads='Cargo may use the network to retrieve locked build dependencies'
}
function Invoke-Step([string]$name, [string]$program, [string[]]$arguments) {
    $stdout = Join-Path $logs ($name + '.stdout.log')
    $stderr = Join-Path $logs ($name + '.stderr.log')
    $argv = (@($arguments | ForEach-Object { Quote-Arg $_ }) -join ' ')
    Write-Host ('[isolated-build] ' + $name + ' started; output goes to local logs.')
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $program -ArgumentList $argv -WorkingDirectory $repo -RedirectStandardOutput $stdout -RedirectStandardError $stderr -NoNewWindow -PassThru
    if ($null -eq $process) { throw ('Process not returned: ' + $name) }
    $null = $process.Handle
    try {
        while (-not $process.WaitForExit(30000)) {
            Write-Host ('[isolated-build] {0}: still running, {1}s; production client untouched.' -f $name, [int]$clock.Elapsed.TotalSeconds)
        }
        $process.WaitForExit()
        $code = $process.ExitCode
        if ($null -eq $code) { throw ('Exit status unavailable: ' + $name) }
    } finally { $process.Dispose() }
    $entry = [pscustomobject]@{name=$name; exit_code=[int]$code; seconds=[Math]::Round($clock.Elapsed.TotalSeconds,1)}
    [void]$steps.Add($entry)
    # Read only after the process exited, avoiding redirected-log sharing races.
    $text = [string][IO.File]::ReadAllText($stdout) + "`n" + [string][IO.File]::ReadAllText($stderr)
    if ($code -ne 0) {
        @($text -split '\r?\n' | Where-Object { $_ } | Select-Object -Last 12) | ForEach-Object {
            Write-Host $_.Substring(0, [Math]::Min(220, $_.Length))
        }
        throw ($name + ' failed; exact logs will be included in review.zip.')
    }
    Write-Host ('[isolated-build] ' + $name + ' exited successfully.')
    return $text
}
try {
    foreach ($path in @($logs,$frames,$product,$target)) { [void][IO.Directory]::CreateDirectory($path) }
    foreach ($key in @('GITHUB_ACTIONS','GLASS_CI_NATIVE_FIXTURE','GLASS_MATERIAL_FIXTURES','CARGO_TERM_COLOR')) {
        $environmentBefore[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
    }
    [Environment]::SetEnvironmentVariable('GITHUB_ACTIONS',$null,'Process')
    [Environment]::SetEnvironmentVariable('GLASS_CI_NATIVE_FIXTURE',$null,'Process')
    [Environment]::SetEnvironmentVariable('GLASS_MATERIAL_FIXTURES',(Join-Path $frames 'fixture'),'Process')
    [Environment]::SetEnvironmentVariable('CARGO_TERM_COLOR','never','Process')
    Write-Host ('[isolated-build] Run: ' + $run)
    Write-Host 'First build may populate a new cache. No existing target directory is deleted or reused as an installation.'
    $common = @('--locked','--release','--target','x86_64-pc-windows-msvc','--target-dir',$target,'-j',[string]$Jobs)
    $lib = Join-Path $repo 'experiments\glass-live\Cargo.toml'
    $client = Join-Path $repo 'clients\desktop-client\Cargo.toml'
    # Listing compiles test code but executes no test bodies. Resolve exact names
    # from this binary, then require one selected and passed test per invocation.
    $listing = Invoke-Step 'list-offscreen-tests' $cargo (@('test') + $common + @('--manifest-path',$lib,'--lib','--','--list'))
    $allowed = @(
        @{suffix='actual_production_adaptive_contracts'; marker='[adaptive-glass] PASS;'; step='adaptive-contracts'}
        @{suffix='current_adaptive_voice_states_and_review_frames'; marker='[adaptive-current] PASS;'; step='current-states-and-frames'}
        @{suffix='all_production_states_use_actual_optics'; marker='[voice-render-contract] PASS;'; step='reference-states-and-palette'}
    )
    foreach ($test in $allowed) {
        $pattern = '(?m)^([A-Za-z0-9_:]+::' + [regex]::Escape($test.suffix) + '): test\r?$'
        $matches = [regex]::Matches($listing, $pattern)
        if ($matches.Count -ne 1) { throw ('Cannot uniquely resolve allowlisted test: ' + $test.suffix) }
        $qualified = $matches[0].Groups[1].Value
        $output = Invoke-Step $test.step $cargo (@('test') + $common + @('--manifest-path',$lib,'--lib',$qualified,'--','--exact','--nocapture','--test-threads=1'))
        if (-not $output.Contains($test.marker) -or $output -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;') { throw ('Test did not actually run and pass: ' + $qualified) }
        [void]$verifiedTests.Add($qualified)
    }
    # Compile formal tests without running unknown UI/audio/network test bodies.
    $null = Invoke-Step 'compile-formal-tests-no-run' $cargo (@('test') + $common + @('--manifest-path',$client,'--bin','DoubaoVoiceClient','--no-run'))
    $null = Invoke-Step 'build-formal-client' $cargo (@('build') + $common + @('--manifest-path',$client,'--bin','DoubaoVoiceClient'))
    Check-Source
    $built = Join-Path $target 'x86_64-pc-windows-msvc\release\DoubaoVoiceClient.exe'
    $delivery = Join-Path $product 'DoubaoVoiceClient.exe'
    [IO.File]::Copy($built,$delivery,$false)
    if ((Hash $built) -ne (Hash $delivery)) { throw 'Formal EXE copy checksum mismatch.' }
    $result.executable = $delivery
    $result.executable_sha256 = Hash $delivery
    $selfTest = Invoke-Step 'formal-offscreen-self-test' $delivery @('--liquid-glass-self-test')
    foreach ($marker in @('[voice-glass-self-test] PASS','[adaptive-glass] PASS;','[adaptive-current] PASS;','[voice-waveform-palette] PASS;','BGRA input readback exact')) {
        if (-not $selfTest.Contains($marker)) { throw ('Formal EXE self-test missing actual-execution marker: ' + $marker) }
    }
    if ((Hash $delivery) -ne $result.executable_sha256) { throw 'Tested EXE changed.' }
    $result.formal_self_test = 'passed; actual shared production shader, generated inputs only'
    Check-Source
    $result.source_unchanged = $true
    $result.production_executable_unchanged = (Hash $production) -ceq $productionBefore
    if (-not $result.production_executable_unchanged) { throw 'Installed EXE changed during this build; do not assume this is a deployment.' }
    $result.completed = $true
} catch {
    $result.error = [string]$_.Exception.Message
    Write-Host ('[isolated-build] stopped: ' + $result.error)
} finally {
    foreach ($key in $environmentBefore.Keys) { [Environment]::SetEnvironmentVariable($key,$environmentBefore[$key],'Process') }
    $result.offscreen_tests = @($verifiedTests.ToArray())
    $result.steps = @($steps.ToArray())
    $lock.Dispose()
}
# Always return evidence from this run, including failed checks. Never package
# user settings, desktop images, source worktrees, build caches or old logs.
[void][IO.Directory]::CreateDirectory($review)
Save-Json (Join-Path $review 'result.json') $result
if ($result.completed) { Save-Json (Join-Path $product 'build-info.json') $result }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = Join-Path $run 'review.zip'
[IO.Compression.ZipFile]::CreateFromDirectory($review,$archive,[IO.Compression.CompressionLevel]::Optimal,$false)
Write-Output (ConvertTo-Json -InputObject $result -Depth 8 -Compress)
Write-Host ('[isolated-build] Evidence: ' + $archive)
if ($AttachEvidence) { Write-Host ("`n[UPLOAD FILE] " + $archive) }
if ($result.completed) { exit 0 } else { exit 1 }
