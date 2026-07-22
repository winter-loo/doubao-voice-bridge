param(
    [ValidateSet("Toggle", "SaveSettings")]
    [string]$Mode = "Toggle"
)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class DoubaoOverlayInput {
    public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extraInfo);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maximum);

    [DllImport("user32.dll")]
    public static extern IntPtr SendMessage(IntPtr hwnd, uint message, UIntPtr wparam, IntPtr lparam);
}
"@

function Send-Key([byte]$Key, [bool]$Down) {
    $flags = if ($Down) { 0 } else { 2 }
    [DoubaoOverlayInput]::keybd_event($Key, 0, $flags, [UIntPtr]::Zero)
}

if ($Mode -eq "SaveSettings") {
    $process = Get-Process DoubaoVoiceClient -ErrorAction Stop
    $script:settingsWindow = [IntPtr]::Zero
    $callback = [DoubaoOverlayInput+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$parameter)
        $processId = 0
        [void][DoubaoOverlayInput]::GetWindowThreadProcessId($hwnd, [ref]$processId)
        if ($processId -eq $process.Id) {
            $title = [Text.StringBuilder]::new(128)
            [void][DoubaoOverlayInput]::GetWindowText($hwnd, $title, $title.Capacity)
            if ($title.ToString() -eq "Doubao Voice Client") {
                $script:settingsWindow = $hwnd
            }
        }
        return $true
    }
    [void][DoubaoOverlayInput]::EnumWindows($callback, [IntPtr]::Zero)
    if ($script:settingsWindow -eq [IntPtr]::Zero) {
        throw "Could not find the settings window"
    }
    [void][DoubaoOverlayInput]::SendMessage(
        $script:settingsWindow,
        0x0111,
        [UIntPtr]1004,
        [IntPtr]::Zero
    )
    exit
}

Send-Key 0x11 $true
Send-Key 0x12 $true
Send-Key 0x20 $true
Send-Key 0x20 $false
Send-Key 0x12 $false
Send-Key 0x11 $false
