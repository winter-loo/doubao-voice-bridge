# Doubao Voice Bridge

Prototype for using Doubao IME on macOS as a remote speech input bridge for Windows/Linux.

For a complete first-time setup and daily-use walkthrough, see
[macOS and Windows Tutorial](MACOS_WINDOWS_TUTORIAL.md).

The current working path is:

```text
Linux/Windows client
  -> native client captures the microphone as 48 kHz PCM
  -> raw PCM over TCP to the Mac
  -> Mac bridge writes PCM to BlackHole through CoreAudio
  -> Doubao IME listens to BlackHole
  -> Doubao commits text into a dedicated capture NSTextView
  -> Mac bridge streams text/final events back to the client
```

## Mac Setup

1. Install `BlackHole 2ch` with `brew install blackhole-2ch`, then restart macOS.
2. Set Doubao microphone to `自动检测`.
3. Set Doubao voice input shortcut to long-press `fn`.
4. Grant Accessibility permission to the app or terminal running the bridge.
5. Make sure Doubao IME has microphone permission.

List CoreAudio devices and the current default input:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac --list-audio-devices
```

On the current Mac:

- BlackHole CoreAudio output is resolved by name because numeric identifiers
  change when audio devices are installed or removed.
- Doubao input source: `com.bytedance.inputmethod.doubaoime.pinyin`
- Doubao app bundle: `com.bytedance.inputmethod.doubaoime`

## Build

```bash
swift build -c release
scripts/build-mac-app.sh
```

The app build uses `DOUBAO_CODESIGN_IDENTITY` when set, otherwise it selects
the first valid local code-signing identity. Stable signing keeps the macOS
Accessibility grant valid across rebuilds. If identity signing is unavailable,
the script falls back to ad-hoc signing with a stable designated requirement
instead of binding the permission identity to the executable's changing hash.

Prefer the app bundle for IME testing. It gives macOS and Doubao a stable app identity:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac --help
```

For normal use, double-click `dist/DoubaoVoiceBridge.app`. The app runs in the
menu bar without a Terminal or Dock icon. On first launch, its setup assistant
checks Accessibility permission, Doubao, BlackHole, and the Windows
connection. Settings and diagnostics remain available from the menu-bar icon.

## Run Mac Bridge

The packaged app now uses the tested configuration by default:

- TCP control on `4387` and TCP audio on `5004`
- `BlackHole 2ch` for virtual audio and temporary default input
- long-press Fn for Doubao activation
- automatic physical-microphone restoration
- an off-screen capture window

The command below remains available for developer diagnostics and explicit
overrides:

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

With `--remote-input-device`, the bridge temporarily switches macOS default input to BlackHole when a session starts, then restores the previous input device after stop.

Before switching, the bridge atomically records the original and remote CoreAudio
device UIDs in:

```text
~/Library/Application Support/DoubaoVoiceBridge/default-input-recovery.json
```

If the bridge is interrupted before normal restoration, its next launch restores
the original input device before starting the UI or network listeners. If the
current input no longer matches the recorded remote device, the bridge assumes
the user already selected another microphone and only removes the stale record.
`--no-restore-default-input` disables both normal and crash recovery.

During an active remote recording, the bridge focuses its internal Voice Input
capture target. The target stays off-screen in normal mode. If another
application takes focus, the bridge immediately reactivates the capture target
so Doubao does not insert recognized text into the wrong application. This focus
enforcement stops when recording ends. Enable **Show developer capture window**
in Settings only when debugging.

## Run Linux Client

The GPUI Linux client now implements the native microphone-to-bridge path and
does not require Python or FFmpeg at runtime. From an interactive Linux desktop:

```bash
cd clients/gpui-overlay-prototype
cargo build --release
DOUBAO_BRIDGE_SERVER=MAC_IP:4387 ./target/release/DoubaoVoiceClient
```

It starts a one-shot recording session immediately. Click the overlay or press
`Ctrl+C` to stop, wait for the final text, and attempt to paste it into the
previously focused application. See
[`clients/gpui-overlay-prototype/README.md`](clients/gpui-overlay-prototype/README.md)
for Linux clipboard/input-tool requirements and current Wayland limitations.

The Python client remains useful for protocol diagnostics:

Linux PulseAudio/PipeWire default microphone:

```bash
python3 clients/doubao_remote.py \
  --server MAC_IP:4387 \
  --udp-host MAC_IP \
  --udp-port 5004 \
  --audio-transport tcp \
  record \
  --audio-start-delay 0.2 \
  --audio-stop-delay 0.5 \
  --recording-timeout 15 \
  --final-timeout 8
```

Without `--seconds`, the client records until you press `Ctrl+C`. To stop automatically after a fixed duration, add `--seconds 22`.

The client starts streaming audio early, but recording begins after the Mac bridge emits `phase=recording`. That phase is only emitted after the Mac confirms Doubao's voice UI is active. Pressing `Ctrl+C` stops the remote microphone, releases the Doubao voice shortcut, and waits for the final committed text.

The GPUI client mirrors that protocol state. It displays `激活中` for
the Mac bridge's `arming` and `voice_retry` phases, and only switches to the
audio-reactive waveform after `phase=recording`. This prevents early microphone
energy from looking as if Doubao is already recognizing speech.

When launching the client as a one-shot SSH command, allocate a pseudo-terminal so `Ctrl+C` reaches the remote Python process:

```bash
ssh -t USER@CLIENT_HOST 'cd ~/doubao-voice-bridge && python3 clients/doubao_remote.py ... record'
```

## Run Windows Client

The GPUI Windows release client is implemented entirely in Rust. It captures
the Windows default microphone through WASAPI and does not require Python or
FFmpeg at runtime:

```powershell
cd clients\gpui-overlay-prototype
cargo build --release
$env:DOUBAO_BRIDGE_SERVER = "MAC_IP:4387"
Start-Process .\target\release\DoubaoVoiceClient.exe
```

The Python commands below remain available for protocol diagnostics and fixture
replay.

On Windows, the client discovers DirectShow audio input devices automatically. If exactly one device is available, no input option is required:

```powershell
py -3 clients/doubao_remote.py `
  --server MAC_IP:4387 `
  --udp-host MAC_IP `
  --udp-port 5004 `
  --audio-transport tcp `
  record
```

If multiple devices are available, the client prints every device name and exits. Run it again with the desired name:

```powershell
py -3 clients/doubao_remote.py `
  --server MAC_IP:4387 `
  --udp-host MAC_IP `
  --audio-transport tcp `
  --input-device "Microphone Array" `
  record
```

Use `--input-args` only when a custom FFmpeg input configuration is needed. It cannot be combined with `--input-device` or `--input-file`.

## Fixture Replay

For reproducible tests, use a fixed WAV file instead of live microphone input:

```bash
python3 clients/doubao_remote.py \
  --server MAC_IP:4387 \
  --udp-host MAC_IP \
  --udp-port 5004 \
  --audio-transport tcp \
  --input-file /tmp/doubao-fixture-2-normalized.wav \
  record \
  --seconds 12 \
  --recording-timeout 15 \
  --final-timeout 8
```

The latest verified fixture test produced:

```text
这是豆包远程语音输入测试。这是豆包远程语音输入测试。12345678。这是豆包远程语音输
```

Fixture recordings are local generated artifacts and are intentionally ignored by git.

## Diagnostics

The bridge emits JSON-line events over the control TCP connection.

Important events:

```json
{"type":"status","phase":"arming","recording":true}
{"type":"status","phase":"voice_retry","attempt":1,"maxAttempts":2}
{"type":"status","phase":"recording","recording":true}
{"type":"voice_state","likelyVoiceUIActive":true,"keywordHits":["识别"]}
{"type":"audio_level","rmsDBFS":-18.2,"peakDBFS":-6.3,"bytes":129600}
{"type":"focus_state","focusedElement":{"ax":{"role":"AXTextArea","value":"..."}}}
{"type":"partial","text":"..."}
{"type":"text","text":"...","delta":"..."}
{"type":"final","text":"..."}
```

While Doubao is recognizing speech, the bridge sends revisable `partial` snapshots at up to 10 updates per second. Interactive clients redraw one terminal line for these previews. Committed `text` and `final` events remain stable output.

Manual diagnostic commands:

```bash
python3 clients/doubao_remote.py --server MAC_IP:4387 send voice-state
python3 clients/doubao_remote.py --server MAC_IP:4387 send focus-state
python3 clients/doubao_remote.py --server MAC_IP:4387 send diagnose
python3 clients/doubao_remote.py --server MAC_IP:4387 send test-hotkey
```

## Notes

- The production audio transport is TCP: the native Windows client sends 48 kHz mono PCM directly to the Mac's CoreAudio output.
- FFmpeg and `--python-tcp-audio` remain diagnostic paths and are not used by the normal Windows application.
- Doubao first inserts ASR text as marked text in the capture text view. Releasing `fn` commits it to normal text.
- If `voice_state` is active and `audio_level` is strong but no text appears, the failure is inside Doubao recognition/commit behavior, not network audio or focus routing.
