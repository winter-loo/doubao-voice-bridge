# Doubao Voice Bridge

Prototype for using Doubao IME on macOS as a remote speech input bridge for Windows/Linux.

The current working path is:

```text
Linux/Windows client
  -> ffmpeg captures microphone or replays a fixture file
  -> raw PCM over TCP to the Mac
  -> Mac bridge plays PCM into Soundflower
  -> Doubao IME listens to Soundflower
  -> Doubao commits text into a dedicated capture NSTextView
  -> Mac bridge streams text/final events back to the client
```

## Mac Setup

1. Install a virtual audio device. The tested device is `Soundflower (2ch)`.
2. Set Doubao microphone to `自动检测`.
3. Set Doubao voice input shortcut to long-press `fn`.
4. Grant Accessibility permission to the app or terminal running the bridge.
5. Make sure Doubao IME has microphone permission.

List CoreAudio devices and the current default input:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac --list-audio-devices
```

On the current Mac:

- Soundflower AudioToolbox output index: `3` (FFmpeg 8.0.1)
- Doubao input source: `com.bytedance.inputmethod.doubaoime.pinyin`
- Doubao app bundle: `com.bytedance.inputmethod.doubaoime`

## Build

```bash
swift build -c release
scripts/build-mac-app.sh
```

Prefer the app bundle for IME testing. It gives macOS and Doubao a stable app identity:

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac --help
```

## Run Mac Bridge

```bash
dist/DoubaoVoiceBridge.app/Contents/MacOS/doubao-bridge-mac \
  --port 4387 \
  --udp-port 5004 \
  --audio-transport tcp \
  --audio-device-index 3 \
  --remote-input-device "Soundflower (2ch)" \
  --voice-shortcut fn \
  --voice-shortcut-mode hold \
  --startup-delay 0.3 \
  --voice-activation-check-delay 1.0 \
  --voice-activation-retries 2 \
  --voice-activation-retry-delay 0.25
```

With `--remote-input-device`, the bridge temporarily switches macOS default input to Soundflower when a session starts, then restores the previous input device after stop.

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

During an active remote recording, the bridge brings its Voice Input capture window to the foreground and keeps its text view as the first responder. If another application takes focus, the bridge immediately reactivates the capture window so Doubao does not insert recognized text into the wrong application. This focus enforcement stops when recording ends.

## Run Linux Client

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

The Windows GPUI client mirrors that protocol state. It displays `激活中` for
the Mac bridge's `arming` and `voice_retry` phases, and only switches to the
audio-reactive waveform after `phase=recording`. This prevents early microphone
energy from looking as if Doubao is already recognizing speech.

When launching the client as a one-shot SSH command, allocate a pseudo-terminal so `Ctrl+C` reaches the remote Python process:

```bash
ssh -t USER@CLIENT_HOST 'cd ~/doubao-voice-bridge && python3 clients/doubao_remote.py ... record'
```

## Run Windows Client

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

- The stable short-term audio transport is TCP with the remote `ffmpeg` process connecting directly to the Mac audio port.
- `--python-tcp-audio` exists as an experimental client-side proxy path, but it is not the default.
- Doubao first inserts ASR text as marked text in the capture text view. Releasing `fn` commits it to normal text.
- If `voice_state` is active and `audio_level` is strong but no text appears, the failure is inside Doubao recognition/commit behavior, not network audio or focus routing.
