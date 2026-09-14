# Reduction experiment, not an automatic material acceptance test.
# Reuses the exact source observer/fixtures/metrics from verify-native-source.ps1.
# The minimal executable intentionally draws only the shared native brush: its
# colors are NOT comparable to the shaded product; compare startup vs warm within
# each executable. Nothing here changes or starts the production voice client.
[CmdletBinding()]
param([string]$OutputDirectory, [switch]$CompileOnly,
    [ValidateSet('compare','gpui','minimal')][string]$HostKind = 'compare')
$ErrorActionPreference = 'Stop'
$probe = Join-Path $PSScriptRoot 'verify-native-source.ps1'
if (-not (Test-Path -LiteralPath $probe -PathType Leaf)) { throw 'Shared source audit is missing.' }
& $probe -CompileOnly
if ($LASTEXITCODE -ne 0) { throw 'Shared C# source audit did not compile.' }
if (-not ('GlassSourceAudit19' -as [type])) { throw 'Unexpected shared source audit version.' }
if ($CompileOnly) {
    Write-Output 'Native-host comparison wrapper checked; no test window started.'
    $global:LASTEXITCODE = 0
    return
}
if (-not [Environment]::Is64BitProcess) { throw '64-bit Windows PowerShell is required.' }
$running = @(Get-Process | Where-Object { $_.ProcessName -in @('GlassPreview','GlassNativeMinimal') })
if ($running.Count -gt 0) { throw 'An existing diagnostic host is running; none was stopped.' }
$kinds = if ($HostKind -eq 'compare') { @('gpui','minimal') } else { @($HostKind) }
$executables = @{
    gpui = Join-Path $PSScriptRoot 'target\release\GlassPreview.exe'
    minimal = Join-Path $PSScriptRoot 'target\release\examples\GlassNativeMinimal.exe'
}
foreach ($kind in $kinds) {
    if (-not (Test-Path -LiteralPath $executables[$kind] -PathType Leaf)) {
        throw ('Built diagnostic host is missing: ' + $executables[$kind])
    }
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $env:TEMP ('doubao-host-comparison-' + [Guid]::NewGuid().ToString('N'))
}
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Refusing to overwrite an existing output directory.' }
[void](New-Item -ItemType Directory -Path $OutputDirectory)
$rows = @()
$hosts = @()
$failure = $null
try {
    foreach ($kind in $kinds) {
        $exe = $executables[$kind]
        $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        $directory = Join-Path $OutputDirectory $kind
        # Run the shared experiment without HWND-region removal, brush renewal,
        # or altered source repaint order. It repeats each host three times.
        $report = [GlassSourceAudit19]::Run($exe,$directory,'none','keep')
        $report.ExecutableSha256 = $hash
        if (Test-Path -LiteralPath $directory -PathType Container) {
            [IO.File]::WriteAllText((Join-Path $directory 'result.json'),($report | ConvertTo-Json -Depth 8 -Compress))
        }
        $hosts += [ordered]@{
            Kind=$kind; Executable=$exe; Sha256=$hash; Completed=$report.Completed
            FocusPreserved=$report.FocusPreserved; Error=$report.Error
            Appearance=$(if ($kind -eq 'minimal') { 'bare shared HostBackdrop; no product foreground/theme/animation' } else { 'GPUI product material; fixed dark text state' })
        }
        foreach ($case in $report.Cases) {
            $observed = @($case.Observations | Where-Object { $null -ne $_.SourceMismatches })
            $validSource = $observed.Count -gt 0 -and @($observed | Where-Object {
                $_.SourceMismatches -ne 0 -or $_.SourceOuterRowValid -ne $true
            }).Count -eq 0
            $warm = @($case.Observations | Where-Object { $_.Phase -eq 'after-source-repaint' })
            $left = @(); $right = @(); $colorOrder = $null
            if ($warm.Count -eq 1) {
                $left = @($warm[0].GlassLeft); $right = @($warm[0].GlassRight)
                if ($left.Count -eq 3 -and $right.Count -eq 3) {
                    $colorOrder = $left[0] -gt $left[2] -and $right[2] -gt $right[0]
                }
            }
            # A constant/blank output could have zero startup error. Preserve
            # actual warm RGB and expected red/blue ordering as additional
            # evidence; the ordering alone is NOT visual/blur acceptance.
            $rows += [ordered]@{
                Host=$kind; Trial=$case.Name; Error=$case.Error; Stopped=$case.Stopped
                BeforeToWarm=$case.BeforeToWarm; ObserverToWarm=$case.ObserverToWarm
                AfterWaitToWarm=$case.AfterOwnToWarm
                SourceProbesValid=$validSource; MaxOutsideChanged=$case.MaxOutsideChanged
                PaperPaintCounts=@($case.Observations | ForEach-Object { $_.PaperPaints })
                WarmLeftRgb=$left; WarmRightRgb=$right; WarmRedBlueOrder=$colorOrder
            }
            if ($kind -eq 'minimal') {
                if ($case.Log -notmatch '\[glass-minimal\] selected=Native;') {
                    throw 'Minimal host did not report its identity; do not confuse it with the GPUI product.'
                }
                if ($case.Log -match '\[glass-minimal\] failed:') {
                    throw ('Minimal host reported a runtime/cleanup failure; inspect ' + $directory)
                }
            }
        }
        if (-not $report.Completed) { throw ('Host experiment failed: ' + $report.Error) }
        if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $hash) { throw 'A diagnostic binary changed during the run.' }
    }
} catch {
    $failure = [string]$_.Exception.Message
    if ($failure.Length -gt 600) { $failure = $failure.Substring(0,600) }
}
$result = [ordered]@{
    Completed=($null -eq $failure); Error=$failure; Directory=$OutputDirectory
    Scope='Native-path reduction, not a proposed fix. Compare startup/warm within each host, not raw color errors across materials; an unchanged blank surface is not success. Same shared source audit; GDI captures are not presentation/FPS measurements.'
    Hosts=$hosts; Cases=$rows; VisualAcceptance='not_evaluated'
}
$json = $result | ConvertTo-Json -Depth 6 -Compress
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'comparison.json'),$json)
Write-Output $json
if ($null -ne $failure) { throw $failure }
$global:LASTEXITCODE = 0
