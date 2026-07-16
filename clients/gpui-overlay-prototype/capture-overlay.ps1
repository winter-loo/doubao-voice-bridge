Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class OverlayDiagnostics {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    public static extern IntPtr GetWindowLongPtr(IntPtr hwnd, int index);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
}
"@

$process = Get-Process gpui-overlay-prototype -ErrorAction Stop
$hwnd = $process.MainWindowHandle
$rect = New-Object OverlayDiagnostics+RECT
[void][OverlayDiagnostics]::GetWindowRect($hwnd, [ref]$rect)
$foreground = [OverlayDiagnostics]::GetForegroundWindow()
$extendedStyle = [OverlayDiagnostics]::GetWindowLongPtr($hwnd, -20).ToInt64()

$diagnostics = [ordered]@{
    processId = $process.Id
    windowHandle = $hwnd.ToInt64()
    foregroundWindowHandle = $foreground.ToInt64()
    overlayIsForeground = $hwnd -eq $foreground
    extendedStyle = ('0x{0:X8}' -f $extendedStyle)
    noActivate = ($extendedStyle -band 0x08000000) -ne 0
    toolWindow = ($extendedStyle -band 0x00000080) -ne 0
    topmost = ($extendedStyle -band 0x00000008) -ne 0
    bounds = [ordered]@{
        left = $rect.Left
        top = $rect.Top
        right = $rect.Right
        bottom = $rect.Bottom
    }
}

$outputDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$diagnostics | ConvertTo-Json -Depth 3 | Set-Content "$outputDirectory/overlay-diagnostics.json" -Encoding utf8

$padding = 24
$width = $rect.Right - $rect.Left + ($padding * 2)
$height = $rect.Bottom - $rect.Top + ($padding * 2)
$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen(
    $rect.Left - $padding,
    $rect.Top - $padding,
    0,
    0,
    $bitmap.Size,
    [System.Drawing.CopyPixelOperation]::SourceCopy
)
$bitmap.Save("$outputDirectory/overlay-screenshot.png", [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

$captureFilter = "ddagrab=output_idx=0:draw_mouse=0:video_size=${width}x${height}:offset_x=$($rect.Left - $padding):offset_y=$($rect.Top - $padding),hwdownload,format=bgra"
& ffmpeg -hide_banner -loglevel error -filter_complex $captureFilter -frames:v 1 -y "$outputDirectory/overlay-dda.png"
