Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class DoubaoWindowCapture {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr parameter);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maximum);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hwnd, IntPtr deviceContext, uint flags);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hwnd, int command);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
}
"@

[void][DoubaoWindowCapture]::SetProcessDPIAware()

$process = Get-Process DoubaoVoiceClient -ErrorAction Stop
$windows = [System.Collections.Generic.List[object]]::new()
$callback = [DoubaoWindowCapture+EnumWindowsProc]{
    param([IntPtr]$hwnd, [IntPtr]$parameter)
    $processId = 0
    [void][DoubaoWindowCapture]::GetWindowThreadProcessId($hwnd, [ref]$processId)
    if ($processId -eq $process.Id) {
        $rect = New-Object DoubaoWindowCapture+RECT
        [void][DoubaoWindowCapture]::GetWindowRect($hwnd, [ref]$rect)
        $title = [Text.StringBuilder]::new(256)
        [void][DoubaoWindowCapture]::GetWindowText($hwnd, $title, $title.Capacity)
        $windows.Add([pscustomobject]@{
            handle = $hwnd.ToInt64()
            title = $title.ToString()
            visible = [DoubaoWindowCapture]::IsWindowVisible($hwnd)
            left = $rect.Left
            top = $rect.Top
            width = $rect.Right - $rect.Left
            height = $rect.Bottom - $rect.Top
        })
    }
    return $true
}
[void][DoubaoWindowCapture]::EnumWindows($callback, [IntPtr]::Zero)

$outputDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$windows | ConvertTo-Json | Set-Content "$outputDirectory/settings-windows.json" -Encoding utf8
$target = $windows | Where-Object title -eq "Doubao Voice Client" | Select-Object -First 1
if ($null -eq $target) {
    throw "No Doubao Voice Client settings window was found"
}
[void][DoubaoWindowCapture]::ShowWindow([IntPtr]$target.handle, 5)
[void][DoubaoWindowCapture]::SetForegroundWindow([IntPtr]$target.handle)
Start-Sleep -Milliseconds 300

$bitmap = New-Object System.Drawing.Bitmap $target.width, $target.height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($target.left, $target.top, 0, 0, $bitmap.Size)
$bitmap.Save("$outputDirectory/settings-screenshot.png", [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
