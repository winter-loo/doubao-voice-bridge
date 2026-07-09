# Doubao Voice Bridge Roadmap

## Long-term goal

Build a cross-platform voice input bridge that lets Windows and Linux users speak into their local microphone and use Doubao IME on a Mac as the recognition engine, with the recognized text inserted back into the active Windows/Linux application.

The intended architecture is:

```text
Windows/Linux microphone
  -> low-latency audio stream
  -> Mac virtual audio output device
  -> Doubao IME listens to that virtual device as microphone input
  -> Doubao types into a dedicated Mac capture text view
  -> Mac bridge streams recognized text back
  -> Windows/Linux client inserts text at the current cursor
```

## Non-goals

- Do not reverse engineer Doubao's private ASR protocol.
- Do not call Doubao's internal WebSocket/token APIs directly.
- Do not depend on private implementation details from Doubao binaries.

## Current MVP

The first implementation targets a push-to-talk style flow:

1. Run `doubao-bridge-mac` on the Mac.
2. Run `doubao_remote.py record --seconds N` on Windows/Linux.
3. The remote client streams raw PCM microphone audio to the Mac over TCP.
4. The Mac bridge plays that audio into a virtual audio device such as `Soundflower (2ch)` or `OrayVirtualAudioDevice`.
5. The Mac bridge switches to Doubao IME, focuses a capture text view, and holds the configured Doubao voice shortcut.
6. The Mac bridge verifies that Doubao's voice UI is active before marking the session as `recording`.
7. The Mac bridge sends recognized text events back over TCP.
8. The remote client prints the final text, and can optionally paste it into the active app.

## Implemented diagnostics

- `voice_state`: verifies that the selected input source is Doubao and that the Doubao main process exposes a voice recognition window such as `优化识别中`.
- `audio_level`: reports byte count, packet count, RMS dBFS, and peak dBFS for incoming TCP PCM.
- `focus_state`: reports the system focused app/UI element and capture text view state, including marked text.
- Fixture replay: `doubao_remote.py --input-file <wav>` replays a fixed WAV from Linux/Windows for reproducible tests.

## Current validated behavior

- TCP remote audio from Arch Linux to the Mac reaches Soundflower with strong audio levels.
- Doubao voice activation is now gated by the detected Doubao voice UI and retries automatically.
- A clear normalized fixture replayed from Arch was recognized by Doubao and committed into the capture text view.
- Doubao first inserts recognition as marked text; releasing `fn` commits it as normal text.

## Next milestones

- Add a real push-to-talk global hotkey on Windows and Linux.
- Replace raw PCM TCP with Opus/RTP, WebRTC, or QUIC for lower bandwidth and better jitter handling.
- Add VAD and user-visible feedback for "ready, speak now".
- Add robust session state recovery when Doubao voice mode gets out of sync.
- Add real-time provisional text mirroring with replacement/diff handling.
- Add per-platform text injection backends:
  - Windows: SendInput plus clipboard fallback.
  - Linux X11: xdotool/clipboard.
  - Linux Wayland: compositor-specific clipboard/input fallback.
- Package the Mac bridge as a launchable app or launch agent with clear permission onboarding.

## Local Mac facts observed on this machine

- Doubao IME app: `/Library/Input Methods/DoubaoIme.app`
- Doubao input source ID: `com.bytedance.inputmethod.doubaoime.pinyin`
- Doubao bundle ID: `com.bytedance.inputmethod.doubaoime`
- Doubao settings bundle ID: `com.bytedance.inputmethod.doubaoime.settings`
- Available virtual audio devices observed:
  - `OrayVirtualAudioDevice`
  - `Soundflower (2ch)`
- `ffmpeg` is available at `/usr/local/bin/ffmpeg`.
