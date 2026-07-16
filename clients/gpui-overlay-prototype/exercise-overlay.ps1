Add-Type @"
using System;
using System.Runtime.InteropServices;

public static class OverlayExercise {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr parameter);

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {
        public int X;
        public int Y;
    }

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

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out POINT point);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extraInfo);
}
"@

function Find-ProcessWindow([uint32]$ProcessId) {
    $script:foundWindow = [IntPtr]::Zero
    $callback = [OverlayExercise+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$parameter)
        $windowProcessId = 0
        [void][OverlayExercise]::GetWindowThreadProcessId($hwnd, [ref]$windowProcessId)
        if ($windowProcessId -eq $ProcessId) {
            $script:foundWindow = $hwnd
            return $false
        }
        return $true
    }
    [void][OverlayExercise]::EnumWindows($callback, [IntPtr]::Zero)
    return $script:foundWindow
}

function Send-OverlayHotkey {
    [OverlayExercise]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [OverlayExercise]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [OverlayExercise]::keybd_event(0x20, 0, 0, [UIntPtr]::Zero)
    [OverlayExercise]::keybd_event(0x20, 0, 2, [UIntPtr]::Zero)
    [OverlayExercise]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [OverlayExercise]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 400
}

$process = Get-Process gpui-overlay-prototype -ErrorAction Stop
$hwnd = Find-ProcessWindow $process.Id
if ($hwnd -eq [IntPtr]::Zero) {
    throw "Could not find the prototype window"
}

$foregroundBefore = [OverlayExercise]::GetForegroundWindow()
$initiallyVisible = [OverlayExercise]::IsWindowVisible($hwnd)

Send-OverlayHotkey
$visibleAfterFirstHotkey = [OverlayExercise]::IsWindowVisible($hwnd)
Send-OverlayHotkey
$visibleAfterSecondHotkey = [OverlayExercise]::IsWindowVisible($hwnd)
$foregroundAfterHotkeys = [OverlayExercise]::GetForegroundWindow()

if (-not $visibleAfterSecondHotkey) {
    Send-OverlayHotkey
}

$rect = New-Object OverlayExercise+RECT
$cursor = New-Object OverlayExercise+POINT
[void][OverlayExercise]::GetWindowRect($hwnd, [ref]$rect)
[void][OverlayExercise]::GetCursorPos([ref]$cursor)
[void][OverlayExercise]::SetCursorPos($rect.Right - 58, [int](($rect.Top + $rect.Bottom) / 2))
[OverlayExercise]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
[OverlayExercise]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 400
$visibleAfterStopClick = [OverlayExercise]::IsWindowVisible($hwnd)
$foregroundAfterStopClick = [OverlayExercise]::GetForegroundWindow()
[void][OverlayExercise]::SetCursorPos($cursor.X, $cursor.Y)

if (-not $visibleAfterStopClick) {
    Send-OverlayHotkey
}

$result = [ordered]@{
    windowHandle = $hwnd.ToInt64()
    initiallyVisible = $initiallyVisible
    visibleAfterFirstHotkey = $visibleAfterFirstHotkey
    visibleAfterSecondHotkey = $visibleAfterSecondHotkey
    hotkeyToggledTwice = $initiallyVisible -eq $visibleAfterSecondHotkey -and $initiallyVisible -ne $visibleAfterFirstHotkey
    visibleAfterStopClick = $visibleAfterStopClick
    stopClickHidWindow = -not $visibleAfterStopClick
    foregroundBefore = $foregroundBefore.ToInt64()
    foregroundAfterHotkeys = $foregroundAfterHotkeys.ToInt64()
    foregroundAfterStopClick = $foregroundAfterStopClick.ToInt64()
    hotkeysPreservedFocus = $foregroundBefore -eq $foregroundAfterHotkeys
    stopClickPreservedFocus = $foregroundBefore -eq $foregroundAfterStopClick
}

$outputDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$result | ConvertTo-Json | Set-Content "$outputDirectory/overlay-exercise.json" -Encoding utf8
