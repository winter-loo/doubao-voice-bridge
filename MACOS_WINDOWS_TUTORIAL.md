# macOS Bridge and Windows Client Tutorial

This tutorial explains how to use a Mac running Doubao IME as the speech
recognition server for a Windows computer. The Windows microphone is streamed to
the Mac, Doubao recognizes the speech, and live text is written into the Windows
application that had focus.

## How It Works

```text
Windows microphone
  -> native Rust Windows client using WASAPI
  -> TCP audio stream to the Mac on port 5004
  -> native CoreAudio output to BlackHole 2ch
  -> Doubao IME on the Mac
  -> Doubao Voice Bridge capture window
  -> recognized text over TCP port 4387
  -> live Unicode input into the focused Windows application
```

Both computers must be online and able to reach each other. A trusted local
network or Tailscale network is recommended.

## 1. Prepare the Mac

### Install the required software

Install these components on the Mac:

- Doubao IME
- BlackHole with the `BlackHole 2ch` device
- Xcode Command Line Tools

For example, install the command line tools and BlackHole with:

```bash
xcode-select --install
brew install blackhole-2ch
```

Restart macOS after installing BlackHole so CoreAudio loads the new device.

### Configure Doubao IME

Open Doubao IME settings and make these changes:

1. Set the microphone to `自动检测`.
2. Set voice input activation to long-press `Fn`.
3. Confirm that Doubao IME has macOS Microphone permission.

Do not permanently select BlackHole as the Mac's default input. The bridge
temporarily switches the default input to BlackHole when a remote session
starts and restores the previous physical microphone when the session ends.

### Build the Mac app

From the repository root on the Mac:

```bash
scripts/build-mac-app.sh
```

The resulting app is:

```text
dist/DoubaoVoiceBridge.app
```

The build script signs the app with a stable identity when possible. Keeping the
same identity prevents macOS from treating every rebuild as a different app for
Accessibility permission.

### Grant Accessibility permission

Open **System Settings > Privacy & Security > Accessibility**, add
`dist/DoubaoVoiceBridge.app`, and enable it. Doubao Voice Bridge needs this
permission to select the Doubao input source, synthesize the Fn shortcut, and
keep recognized text in its capture window.

If macOS already lists an older copy of the app but input activation fails,
remove that entry, add the newly built app, and enable it again.

### Find the Mac IP address

For Tailscale:

```bash
tailscale ip -4
```

For a local network, use the IP shown in **System Settings > Network**. In the
commands below, replace `<MAC_IP>` with this address.

### Verify BlackHole

List the audio devices seen by the bridge:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac \
  --list-audio-devices
```

The output must include `BlackHole 2ch`. Device identifiers can change, so the
normal launch command selects the device by name instead of by index.

## 2. Start the Mac Bridge

Double-click `dist/DoubaoVoiceBridge.app`. The bridge appears in the macOS menu
bar and does not require a Terminal window. Complete the setup assistant on the
first launch. A green waveform menu-bar icon means the service is ready.

The app starts with the TCP, BlackHole, and Fn settings automatically.
Open its menu-bar item to access Settings, Diagnostics, Start at Login, and the
setup assistant.

For developer diagnostics, the equivalent explicit command is:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac \
  --port 4387 \
  --udp-port 5004 \
  --audio-transport tcp \
  --audio-device-name "BlackHole 2ch" \
  --remote-input-device "BlackHole 2ch" \
  --voice-shortcut fn \
  --voice-shortcut-mode hold \
  --startup-delay 0.3 \
  --voice-activation-check-delay 1.0 \
  --voice-activation-retries 2 \
  --voice-activation-retry-delay 0.25
```

When launched from Terminal, a successful startup includes messages similar to:

```text
Doubao bridge listening on TCP 4387, audio 5004
Audio transport: tcp
CoreAudio output device: BlackHole 2ch
```

The app bundle should be used even when launching from Terminal because its
stable identity is associated with the Accessibility grant. Logs from a normal
Finder launch are written to:

```text
~/Library/Logs/DoubaoVoiceBridge/bridge.log
```

If the macOS firewall asks, allow incoming connections. Windows must be able to
reach TCP ports `4387` and `5004` on the Mac.

## 3. Prepare Windows

The recommended Windows UI is the native Rust GPUI client. A release executable
has no Python or FFmpeg runtime dependency. Install Rust and the Visual Studio
C++ build tools only when building the client from source.

Assume the repository is located at `D:\proj` for the remaining Windows
commands.

### Check network access

```powershell
Test-NetConnection <MAC_IP> -Port 4387
```

The check should report `TcpTestSucceeded : True` while the Mac bridge is
running. TCP port `5004` is opened only after a recording session starts, so it
is normal for an idle port check against `5004` to fail.

### Check the Windows microphone

The Python client automatically enumerates DirectShow audio inputs. You can see
the same list directly with:

```powershell
ffmpeg -hide_banner -list_devices true -f dshow -i dummy
```

The GPUI client uses native WASAPI capture through Rust. It selects the Windows
default recording device even when several microphones are installed. Python
and FFmpeg are not required by the Windows UI.

To override the default microphone, set `DOUBAO_VOICE_INPUT_DEVICE` to an exact
CPAL device name or stable device ID before launching the client.

## 4. Build and Start the Windows UI

Build the release executable once:

```powershell
cd D:\proj\doubao-voice-bridge\clients\desktop-client
cargo build --release
```

Set the Mac address and start the app:

```powershell
Start-Process .\target\release\DoubaoVoiceClient.exe
```

The release executable uses the Windows GUI subsystem, so it does not open a
CMD window. It is self-contained and does not depend on Python, FFmpeg, or the
source repository at runtime. On first launch, enter `<MAC_IP>:4387`, select the
microphone, and press **Save and connect**. Later changes are available from the
notification-area icon.

For developer automation, the environment variable overrides the saved server:

```powershell
[Environment]::SetEnvironmentVariable(
  "DOUBAO_BRIDGE_SERVER",
  "<MAC_IP>:4387",
  "User"
)
```

New processes receive the saved value. Restart Explorer or sign out and back in
before launching the executable directly from File Explorer.

## 5. Use Voice Input on Windows

1. Click the text field in the Windows application where the result should go.
2. Press **Ctrl+Alt+Space** once.
3. The compact overlay first displays `激活中` while the Mac switches to
   BlackHole, focuses the capture window, and activates Doubao voice input.
4. Wait until the overlay changes to the animated waveform, then speak. The
   recognized text appears and is revised in place at the current caret.
5. Press **Ctrl+Alt+Space** again to finish.
6. The overlay displays `优化识别中` while Doubao commits the final result.
7. The last live preview is replaced by the final result in the same input field.

Speech made while `激活中` is displayed is not recognized yet. Start speaking
only after the waveform appears.

Clicking the visible capsule also stops the session. Left Ctrl is not bound by
the Windows client.

## 6. Command-Line Client

The Python client is useful for selecting a microphone and for diagnostics. Run
it from `D:\proj`.

If Windows has exactly one recording device:

```powershell
py -3 clients\doubao_remote.py `
  --server <MAC_IP>:4387 `
  --udp-host <MAC_IP> `
  --udp-port 5004 `
  --audio-transport tcp `
  record `
  --recording-timeout 15 `
  --final-timeout 8 `
  --paste
```

If multiple devices exist, the client prints their names and exits. Repeat the
command with the exact desired name before the `record` subcommand:

```powershell
py -3 clients\doubao_remote.py `
  --server <MAC_IP>:4387 `
  --udp-host <MAC_IP> `
  --udp-port 5004 `
  --audio-transport tcp `
  --input-device "Microphone Array" `
  record `
  --recording-timeout 15 `
  --final-timeout 8 `
  --paste
```

Without `--seconds`, recording continues until you press **Ctrl+C**. Add, for
example, `--seconds 10` after `record` to stop automatically after ten seconds.

`--input-args` is not required for normal Windows use. It is only for a custom
FFmpeg input configuration and cannot be combined with `--input-device` or
`--input-file`.

## 7. Stop the Apps

Stop the Mac bridge with **Ctrl+C** in its Terminal window. The bridge restores
the physical Mac microphone after each normal recording session and also keeps a
recovery record for interrupted sessions.

Stop the Windows UI with:

```powershell
Stop-Process -Name DoubaoVoiceClient
```

## Troubleshooting

### The Windows UI stays on `激活中`

Check these items in order:

1. Confirm TCP port `4387` is reachable with `Test-NetConnection`. Port `5004`
   appears only while a session is active.
2. Confirm Doubao IME is running and its shortcut is long-press Fn.
3. Confirm Doubao Voice Bridge has Accessibility permission.
4. Confirm Doubao's microphone is set to `自动检测`.
5. Check the Windows client log:

```powershell
Get-Content "$env:TEMP\doubao-gpui-voice-client.log" -Tail 100
```

### The waveform appears, but no text is pasted

- Speak only after the waveform appears.
- Confirm the Mac default input changes to BlackHole during recording and
  returns to the physical microphone afterward.
- Confirm Doubao has Microphone permission.
- Make sure the original Windows text field still accepts keyboard input and
  clipboard paste.
- Check the Mac bridge log for CoreAudio errors and the Windows client log for
  WASAPI capture errors.

### Text appears on the Mac instead of Windows

Keep the Doubao Voice Bridge capture window open. During recording, the bridge
must keep its text view focused so Doubao commits text there. If this continues,
restart the Mac bridge and inspect the Windows log for focus recovery errors.

### The GPUI app starts but immediately stops recording

WASAPI could not open the selected microphone, or the saved device is no longer
present. Inspect the Windows log, open the tray Settings window, and select an
available microphone. The command-line client remains useful for device diagnostics.

### The Windows UI shows an old Mac address

Open **Settings** from the notification-area icon and update the Mac server.
`DOUBAO_BRIDGE_SERVER` can override the saved value for developer testing.

### Useful bridge diagnostics

From Windows, while the Mac bridge is running:

```powershell
py -3 clients\doubao_remote.py --server <MAC_IP>:4387 send status
py -3 clients\doubao_remote.py --server <MAC_IP>:4387 send voice-state
py -3 clients\doubao_remote.py --server <MAC_IP>:4387 send focus-state
py -3 clients\doubao_remote.py --server <MAC_IP>:4387 send diagnose
```

Use the Mac bridge's Terminal output together with
`%TEMP%\doubao-gpui-voice-client.log` to determine whether a failure is in the
network connection, microphone capture, Doubao activation, text capture, or the
final Windows paste.
