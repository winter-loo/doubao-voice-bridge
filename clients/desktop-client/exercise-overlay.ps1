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
    $script:foundWindowArea = 0
    $callback = [OverlayExercise+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$parameter)
        $windowProcessId = 0
        [void][OverlayExercise]::GetWindowThreadProcessId($hwnd, [ref]$windowProcessId)
        if ($windowProcessId -eq $ProcessId) {
            $rect = New-Object OverlayExercise+RECT
            if ([OverlayExercise]::GetWindowRect($hwnd, [ref]$rect)) {
                $area = ($rect.Right - $rect.Left) * ($rect.Bottom - $rect.Top)
                if ($area -gt $script:foundWindowArea) {
                    $script:foundWindow = $hwnd
                    $script:foundWindowArea = $area
                }
            }
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

$process = Get-Process DoubaoVoiceClient -ErrorAction Stop
$hwnd = Find-ProcessWindow $process.Id
if ($hwnd -eq [IntPtr]::Zero) {
    throw "Could not find the prototype window"
}

$foregroundBefore = [OverlayExercise]::GetForegroundWindow()
$initiallyVisible = [OverlayExercise]::IsWindowVisible($hwnd)

if ($initiallyVisible) {
    Send-OverlayHotkey
    Start-Sleep -Milliseconds 2500
}

Send-OverlayHotkey
Start-Sleep -Milliseconds 1100
$visibleAfterHotkeyStart = [OverlayExercise]::IsWindowVisible($hwnd)
Send-OverlayHotkey
$visibleAfterHotkeyFinish = [OverlayExercise]::IsWindowVisible($hwnd)
Start-Sleep -Milliseconds 2500
$hiddenAfterHotkeyOptimizing = -not [OverlayExercise]::IsWindowVisible($hwnd)
$foregroundAfterHotkeys = [OverlayExercise]::GetForegroundWindow()

Send-OverlayHotkey
Start-Sleep -Milliseconds 1100

$result = [ordered]@{
    windowHandle = $hwnd.ToInt64()
    initiallyVisible = $initiallyVisible
    hotkeyStarted = $visibleAfterHotkeyStart
    hotkeyShowedOptimizing = $visibleAfterHotkeyFinish
    hiddenAfterHotkeyOptimizing = $hiddenAfterHotkeyOptimizing
    foregroundBefore = $foregroundBefore.ToInt64()
    foregroundAfterHotkeys = $foregroundAfterHotkeys.ToInt64()
    hotkeysPreservedFocus = $foregroundBefore -eq $foregroundAfterHotkeys
}

$outputDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$result | ConvertTo-Json | Set-Content "$outputDirectory/overlay-exercise.json" -Encoding utf8
