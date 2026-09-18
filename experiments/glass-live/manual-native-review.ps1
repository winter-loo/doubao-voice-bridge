#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Build','Review')][string]$Mode,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedCommit,
    [Parameter(Mandatory=$true)][string]$BuildRoot,
    [Parameter(Mandatory=$true)][string]$ProductionExe,
    [Parameter(Mandatory=$true)][string]$CandidateExe,
    [string]$Manifest
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2.0
$base='dbd4d4f0b054c8a462ecf2bb8ee7adc75ddc790c'
$test='voice_window::manual_native_review::local_manual_native_review'
$replayTest='gpu::retained::optical::adaptive::manual_replay_reset::reused_manual_replay_matches_fresh_pipeline'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$root=[IO.Path]::GetFullPath($BuildRoot).TrimEnd([char]'\')
function Hash([string]$p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }
function GitText([string[]]$a) {
    $v=(& git --no-optional-locks -C $repo @a | Out-String).TrimEnd([char[]]"`r`n")
    if ($LASTEXITCODE -ne 0) { throw ('Git check failed: '+($a -join ' ')) }; return $v
}
function SaveJson([string]$path,$value) {
    [IO.File]::WriteAllText($path,($value | ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
}
function ChildOf([string]$a,[string]$b) {
    $a=$a.TrimEnd([char]'\'); $b=$b.TrimEnd([char]'\')
    return $a.Equals($b,[StringComparison]::OrdinalIgnoreCase) -or $a.StartsWith($b+'\',[StringComparison]::OrdinalIgnoreCase)
}
function NoReparse([string]$p) {
    while ($p) {
        if (Test-Path -LiteralPath $p) {
            if (((Get-Item -LiteralPath $p -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse-point path refused' }
        }
        $next=[IO.Path]::GetDirectoryName($p); if ($next -eq $p) { break }; $p=$next
    }
}
function SourceCheck {
    if ((GitText @('rev-parse','HEAD')) -cne $ExpectedCommit) { throw 'Review checkout differs from exact GitHub commit' }
    if ((GitText @('status','--porcelain=v1','--untracked-files=all')).Length) { throw 'Dirty source; no reset or cleanup will be attempted' }
    & git --no-optional-locks -C $repo merge-base --is-ancestor $base HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Review branch does not descend from approved production source' }
    $allowed=@('experiments/glass-live/src/voice_window.rs','experiments/glass-live/src/manual_native_review.rs','experiments/glass-live/manual-native-review.ps1','experiments/glass-live/MANUAL-NATIVE-REVIEW.md','experiments/glass-live/src/adaptive.rs','experiments/glass-live/src/manual_replay_reset.rs')
    foreach ($p in ((GitText @('diff','--name-only',$base,'HEAD')) -split '\r?\n')) {
        if ($p -and $p -notin $allowed) { throw ('Unexpected production change in review branch: '+$p) }
    }
    $old=GitText @('show',($base+':experiments/glass-live/src/voice_window.rs'))
    $new=GitText @('show','HEAD:experiments/glass-live/src/voice_window.rs')
    $suffix="`n`n#[cfg(test)]`n#[path=`"manual_native_review.rs`"]`nmod manual_native_review;"
    if ($new.Replace("`r`n","`n") -cne ($old.Replace("`r`n","`n")+$suffix)) { throw 'Production window functions were changed rather than adding a test-only module' }
    $old=GitText @('show',($base+':experiments/glass-live/src/adaptive.rs'))
    $new=GitText @('show','HEAD:experiments/glass-live/src/adaptive.rs')
    $suffix="`n`n#[cfg(test)]`n#[path=`"manual_replay_reset.rs`"]`nmod manual_replay_reset;"
    if ($new.Replace("`r`n","`n") -cne ($old.Replace("`r`n","`n")+$suffix)) { throw 'Production renderer changed rather than adding the test-only reset module' }
}
function ProtectedFiles {
    if ((Hash $ProductionExe) -ne '9cde74c73ec57f7fa0d9a0855447f732ad0064fb0fe309b3f86f4b38bbcf32c9') { throw 'Installed EXE identity changed; stop for inspection' }
    if ((Hash $CandidateExe) -ne 'd0d88b08e20e4bcadb5250dad8cb9047b4bea7cb189cfaa56db66224120c26ab') { throw 'Approved production candidate EXE identity changed' }
}
function Logged([string]$program,[string[]]$arguments,[string]$folder,[string]$name,[int]$timeoutSeconds=0) {
    $out=Join-Path $folder ($name+'.stdout.log'); $err=Join-Path $folder ($name+'.stderr.log')
    $quoted=(@($arguments | ForEach-Object { if ($_ -match '["\r\n]') { throw 'Unsupported argument' }; '"'+$_+'"' }) -join ' ')
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $p=Start-Process -FilePath $program -ArgumentList $quoted -WorkingDirectory $repo -RedirectStandardOutput $out -RedirectStandardError $err -NoNewWindow -PassThru
    $null=$p.Handle
    try {
        while (-not $p.WaitForExit(1000)) {
            if ($timeoutSeconds -gt 0 -and $clock.Elapsed.TotalSeconds -gt $timeoutSeconds) {
                # Only this uniquely launched review test process, never cargo or the installed client.
                $p.Kill(); $p.WaitForExit(); throw 'Review deadline reached; this review process was stopped, evidence retained'
            }
        }
        $p.WaitForExit(); $code=$p.ExitCode
        if ($null -eq $code) { throw 'Process exit status unavailable' }
    } finally { $p.Dispose() }
    return [int]$code
}
SourceCheck; ProtectedFiles; NoReparse $root
foreach ($line in ((GitText @('worktree','list','--porcelain')) -split '\r?\n')) {
    if ($line.StartsWith('worktree ')) {
        $tree=[IO.Path]::GetFullPath($line.Substring(9).Replace('/','\'))
        if ((ChildOf $root $tree) -or (ChildOf $tree $root)) { throw 'Build root overlaps a registered source worktree' }
    }
}
foreach ($exe in @($ProductionExe,$CandidateExe)) { if (ChildOf $exe $repo) { throw 'Review worktree overlaps a protected executable' } }
if ($Mode -eq 'Build') {
    [void][IO.Directory]::CreateDirectory($root)
    $lock=[IO.File]::Open((Join-Path $root 'pipeline.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {
        $run=Join-Path $root ('native-review-'+$ExpectedCommit.Substring(0,8)+'-'+[Guid]::NewGuid().ToString('N').Substring(0,8))
        [void][IO.Directory]::CreateDirectory($run)
        Write-Host ('[BUILD + OFFSCREEN REPLAY CHECK] '+$run)
        $cargo=(Get-Command cargo.exe -CommandType Application | Select-Object -First 1).Source
        $a=@('test','--locked','--offline','--release','--target','x86_64-pc-windows-msvc','--target-dir',(Join-Path $root 'target'),'-j','2','--manifest-path',(Join-Path $repo 'experiments\glass-live\Cargo.toml'),'--lib','--no-run','--message-format=json')
        $code=Logged $cargo $a $run 'compile'
        if ($code -ne 0) { Write-Host ('[UPLOAD FILE] '+(Join-Path $run 'compile.stderr.log')); throw 'Review harness compilation failed; no windows started' }
        $artifacts=@(Get-Content -LiteralPath (Join-Path $run 'compile.stdout.log') | ForEach-Object {
            if ($_.StartsWith('{')) { $o=$_ | ConvertFrom-Json; if ($o.reason -eq 'compiler-artifact') {
                if ($o.target.name -eq 'doubao_glass_live' -and $o.profile.test -and $o.executable) { $o }
            } }
        })
        if ($artifacts.Count -ne 1) { throw 'Cannot identify one compiled library test executable' }
        $original=[string]$artifacts[0].executable
        if (-not (ChildOf ([IO.Path]::GetFullPath($original)) (Join-Path $root 'target'))) { throw 'Compiler returned unexpected executable path' }
        $delivery=Join-Path $run 'GlassNativeReviewTests.exe'; [IO.File]::Copy($original,$delivery,$false)
        if ((Hash $original) -ne (Hash $delivery)) { throw 'Test binary copy hash mismatch' }
        $deliveryHash=Hash $delivery
        $code=Logged $delivery @('--list') $run 'list' 30
        $listing=[IO.File]::ReadAllText((Join-Path $run 'list.stdout.log'))
        foreach ($entry in @($test,$replayTest)) {
            if ($code -ne 0 -or [regex]::Matches($listing,('(?m)^'+[regex]::Escape($entry)+': test\r?$')).Count -ne 1) { throw ('Exact test entry not found: '+$entry) }
        }
        # Execute ONLY the no-window fresh-vs-reused replay regression. Never the
        # manual/CI desktop tests, and never a broad --ignored selection.
        Write-Host '[REPLAY RESET CHECK] Comparing reused resources with fresh production pipelines; no windows.'
        $code=Logged $delivery @($replayTest,'--exact','--nocapture','--test-threads=1') $run 'replay-reset'
        $out=[IO.File]::ReadAllText((Join-Path $run 'replay-reset.stdout.log'))
        $err=[IO.File]::ReadAllText((Join-Path $run 'replay-reset.stderr.log'))
        if ($code -ne 0 -or -not $err.Contains('[manual-replay-reset] PASS; 36 fresh-vs-reused canvas comparisons;') -or $out -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;') {
            Write-Host ('[UPLOAD FILE] '+(Join-Path $run 'replay-reset.stderr.log')); throw 'Replay reset regression failed; no visible session permitted'
        }
        SourceCheck; ProtectedFiles
        if ((Hash $delivery) -cne $deliveryHash) { throw 'Review binary changed during replay check' }
        $record=[ordered]@{review_commit=$ExpectedCommit;production_source_commit=$base;production_window_body_unchanged=$true;production_renderer_body_unchanged=$true;run_directory=$run;executable=$delivery;sha256=$deliveryHash;test=$test;build_exit_code=0;replay_reset_test=$replayTest;replay_reset_passed=$true;windows_started=$false;scope='Review harness built and reuse checked offscreen; not a formal product EXE or native-window acceptance'}
        $manifestPath=Join-Path $run 'build-manifest.json'; SaveJson $manifestPath $record
        Write-Host ('[BUILD COMPLETE] review_commit='+$ExpectedCommit+'; production_source='+$base+'; replay_reset_passed=True')
        Write-Host ('[UPLOAD FILE] '+$manifestPath)
        Write-Host ('[UPLOAD FILE] '+(Join-Path $run 'replay-reset.stderr.log'))
    } finally { $lock.Dispose() }
} else {
    if ([string]::IsNullOrWhiteSpace($Manifest)) { throw 'Review mode requires the completed build manifest' }
    NoReparse ([IO.Path]::GetFullPath($Manifest)); $m=Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
    if ($m.review_commit -cne $ExpectedCommit -or $m.production_source_commit -cne $base -or $m.test -cne $test -or $m.build_exit_code -ne 0 -or $m.replay_reset_passed -ne $true -or $m.replay_reset_test -cne $replayTest) { throw 'Review manifest identity or replay check mismatch' }
    if (-not (ChildOf $m.run_directory $root) -or $m.executable -ine (Join-Path $m.run_directory 'GlassNativeReviewTests.exe')) { throw 'Manifest output path mismatch' }
    if ((Hash $m.executable) -cne $m.sha256) { throw 'Review binary hash mismatch' }
    Write-Host "`n[USER ACTION REQUIRED] 将出现独立的生成背景与玻璃窗口；不采集桌面、不录音、不自动点击。"
    Write-Host '结束正在进行的语音输入。准备手动检查重播/换背景/波形的响应，以及点击胶囊、点阴影边缘、拖动后再点阴影。'
    Write-Host '初始化一次后显示按钮；点击“退出”结束，窗口最长三分钟自动关闭。当前正式客户端不会退出或替换。'
    $consent=Read-Host '准备好后输入 REVIEW 再回车；其他输入取消'
    if ($consent -cne 'REVIEW') { throw 'Manual review cancelled before window creation' }
    SourceCheck; ProtectedFiles
    if ((Hash $m.executable) -cne $m.sha256) { throw 'Review binary changed while waiting for consent' }
    $session=Join-Path $m.run_directory ('session-'+[Guid]::NewGuid().ToString('N').Substring(0,8)); [void][IO.Directory]::CreateDirectory($session)
    $reportDir=Join-Path $session 'observations'; $saved=@{}
    try {
        foreach ($key in @('GITHUB_ACTIONS','GLASS_CI_NATIVE_FIXTURE','GLASS_MANUAL_NATIVE_REVIEW','GLASS_MANUAL_REVIEW_OUTPUT','GLASS_MATERIAL_FIXTURES')) { $saved[$key]=[Environment]::GetEnvironmentVariable($key,'Process') }
        foreach ($key in @('GITHUB_ACTIONS','GLASS_CI_NATIVE_FIXTURE','GLASS_MATERIAL_FIXTURES')) { [Environment]::SetEnvironmentVariable($key,$null,'Process') }
        $env:GLASS_MANUAL_NATIVE_REVIEW='OWNED_WINDOWS_ONLY'; $env:GLASS_MANUAL_REVIEW_OUTPUT=$reportDir
        Write-Host ('[NATIVE MANUAL SESSION] '+$session)
        $code=Logged $m.executable @($test,'--ignored','--exact','--nocapture','--test-threads=1') $session 'review' 300
    } finally { foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key,$saved[$key],'Process') } }
    ProtectedFiles; SourceCheck
    $reportPath=Join-Path $reportDir 'native-session.json'
    Write-Host ('[UPLOAD FILE] '+(Join-Path $session 'review.stderr.log'))
    if ($code -ne 0 -or -not (Test-Path -LiteralPath $reportPath -PathType Leaf)) { throw 'Native session failed; preserve logs, do not infer acceptance' }
    $native=Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    Write-Host ('[MANUAL INPUT] passed='+$native.manual_input_passed+' body='+$native.body_clicks+' margin='+$native.margin_clicks+' moves='+$native.paired_move_updates+' moved_margin='+$native.margin_clicks_after_move+' leaks='+$native.core_click_leaks)
    Write-Host ('[CONTROL TIMING] samples='+$native.control_update_samples+' max_request_to_submit_ms='+$native.max_button_request_to_submit_ms+' over_100ms='+$native.controls_over_100ms+'; not scanout latency')
    Write-Host "`n[USER ACTION REQUIRED] 请评价：重播/换背景/波形及时响应，白底暗化自然，文字/外阴影完整，拖动成对移动。"
    $visual=Read-Host '全部符合输入 PASS；有问题输入 FAIL；未观察完整输入 UNREVIEWED'
    if ($visual -cnotin @('PASS','FAIL','UNREVIEWED')) { $visual='UNREVIEWED' }
    SaveJson (Join-Path $session 'human-review.json') ([ordered]@{review_commit=$ExpectedCommit;production_source_commit=$base;report='explicit terminal response including control responsiveness and appearance';visual=$visual;manual_input_passed=$native.manual_input_passed;deployment_performed=$false})
    Write-Host ('[UPLOAD FILE] '+$reportPath); Write-Host ('[UPLOAD FILE] '+(Join-Path $reportDir 'events.log')); Write-Host ('[UPLOAD FILE] '+(Join-Path $session 'human-review.json'))
    Write-Host 'Native review finished; no product replacement, startup/settings change or microphone session.'
}
