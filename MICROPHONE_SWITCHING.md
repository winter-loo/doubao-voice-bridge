# Microphone Switching Investigation

## Result

The current Doubao IME build does not appear to store microphone selection in a directly editable plist/json/sqlite preference file.

Checked locations:

- `~/Library/Preferences/com.bytedance.inputmethod.doubaoime.plist`
- `~/Library/Preferences/com.bytedance.inputmethod.doubaoime.settings.plist`
- `~/Library/Application Support/DoubaoIme`
- `~/Library/Caches/com.bytedance.inputmethod.doubaoime`
- `~/Library/Caches/com.bytedance.inputmethod.doubaoime.settings`
- `~/Library/HTTPStorages/com.bytedance.inputmethod.doubaoime`

Observed facts:

- Doubao logs the microphone used during voice input. Example observed event:
  - `radio_name`: `MacBook Pro麦克风`
  - `radio_id`: `Digital Mic`
- The settings UI exposes microphone choices:
  - `自动检测`
  - `MacBook Pro麦克风`
  - `“ldd”的麦克风`
  - `OrayVirtualAudioDevice`
  - likely `Soundflower (2ch)` farther down the scroll list
- Temporarily selecting `OrayVirtualAudioDevice` and then restoring `MacBook Pro麦克风` changed only app log/activity files, not normal preferences or cache databases.
- The binary contains `MMKV` and `OimeSettings.UserSettings.selectedMicrophoneId`, but no obvious MMKV storage file was found on disk.
- The binary also contains `NSDistributedNotificationCenter` usage around `selectedMicrophoneId`, but opening the settings page did not emit an observable notification until a direct UI selection is made.

## Practical implication

For the bridge, do not assume we can safely switch Doubao's microphone by editing a preference file.

Current safe options:

1. Use UI automation to switch Doubao's microphone before/after a remote session.
2. Test whether Doubao follows the system default input device when set to `自动检测`.
3. Avoid switching Doubao's microphone and ask the user to keep a virtual microphone selected while using remote mode.

Option 2 is the best next experiment because it would avoid touching Doubao's private settings:

```text
Set Doubao microphone to 自动检测
Remote session start:
  set macOS default input device -> virtual audio device
Remote session stop:
  restore macOS default input device -> previous device
```

If Doubao honors that during ASR startup, bridge-level microphone switching can be implemented with public CoreAudio APIs instead of UI automation.

## Implemented bridge support

`doubao-bridge-mac` now supports this experiment:

```bash
.build/release/doubao-bridge-mac --list-audio-devices

.build/release/doubao-bridge-mac \
  --audio-device-index 4 \
  --remote-input-device "Soundflower (2ch)"
```

With `--remote-input-device`, the bridge saves the current macOS default input device, switches the default input to the named virtual device when a remote session starts, and restores the previous default input after the session stops.

This still depends on Doubao's `自动检测` behavior. If Doubao snapshots a different device or ignores the system default, use the UI automation fallback.

## UI automation fallback

The Doubao settings page is accessible enough for basic UI automation:

- Process: `DoubaoImeSettings`
- Voice page title: `语音输入`
- Microphone selector button appears after static text `麦克风选择`
- The selector opens a SwiftUI overlay titled `选择麦克风`

However, individual device rows are exposed as unnamed `AXButton` elements, so selection by device name is not stable through Accessibility alone. A reliable UI fallback would need one of:

- screenshot/OCR matching,
- fixed row order with validation,
- or deeper SwiftUI accessibility hooks if Doubao adds labels later.
