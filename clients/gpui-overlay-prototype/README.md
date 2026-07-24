# GPUI Voice Client

The release executable contains the native Rust voice client for Windows and
Linux. It captures the selected microphone through CPAL, normalizes audio to
48 kHz mono s16le PCM, streams it to the Mac bridge, receives bridge events,
and pastes the final text without launching Python or FFmpeg.

## Linux

Install an X11 clipboard tool and input injector. On Arch Linux with GNOME
Wayland/XWayland:

```bash
sudo pacman -S xclip xdotool
```

Build and start the client from an interactive desktop terminal. Keep the
process running and press `F13` to start or finish each voice session:

```bash
cd clients/gpui-overlay-prototype
cargo build --release
DOUBAO_BRIDGE_SERVER=MAC_IP:4387 ./target/release/DoubaoVoiceClient
```

The first press starts recording and opens the overlay near the bottom of the
screen; the next press, an overlay click, or `Ctrl+C` finishes the session. The
client waits up to eight seconds for the final bridge event, falls back to the
latest committed text if needed, attempts to paste the result into the
previously focused app, and returns to the background for the next session.

On X11, the client registers `F13` directly with XGrabKey. On Wayland and
XWayland, it requests `F13` through the XDG Global Shortcuts Portal. The first
Wayland launch may open a desktop authorization dialog. Approve the shortcut
and keep the client running; some desktops may assign a different binding, and
the accepted binding is printed as `global shortcut ready: ...`.

Wayland support requires `xdg-desktop-portal` plus a desktop portal backend that
implements `org.freedesktop.portal.GlobalShortcuts`. To check the active portal:

```bash
gdbus introspect --session \
  --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop |
  grep GlobalShortcuts
```

If the interface is absent, install or update the portal backend for the
desktop. If authorization is cancelled or the portal is unavailable, the
client exits with an error instead of starting the microphone unexpectedly.

For a stable host application identity, install the release binary and desktop
entry under the same application ID:

```bash
install -Dm755 target/release/DoubaoVoiceClient \
  ~/.local/bin/DoubaoVoiceClient
install -Dm644 ../../packaging/local.doubao.voicebridge.desktop \
  ~/.local/share/applications/local.doubao.voicebridge.desktop
```

Ensure `~/.local/bin` is in `PATH`. The desktop entry basename intentionally
matches the client's `local.doubao.voicebridge` application ID.

The default PipeWire/PulseAudio microphone is selected automatically. Set
`DOUBAO_VOICE_INPUT_DEVICE` to an exact CPAL device name to override it. The
audio port can be changed with `DOUBAO_BRIDGE_AUDIO_PORT`.

On XWayland, the client writes the result with `xclip` (or `xsel`) and injects
`Ctrl+V` with `xdotool`. For native Wayland applications, install and configure
`wl-clipboard` plus `ydotool`; compositor security rules may still require
explicit input-device permissions. If injection is unavailable, the recognized
text remains in the clipboard for manual paste.

The Linux slice does not yet include a tray menu or settings window.

## Windows

Build from this directory:

```powershell
cargo build --release
Start-Process .\target\release\DoubaoVoiceClient.exe
```

The Windows release binary uses the GUI subsystem and does not create a console
window when launched directly. Debug builds retain their console for diagnostics.

On first launch, a small setup window asks for the Mac server address and the
microphone to use. Saving performs a connection check, stores the choices in
`%LOCALAPPDATA%\DoubaoVoiceBridge\client.json`, and can register the client to
start when the user signs in. After setup, the client lives in the notification
area. Its menu provides **Settings** and **Quit**; launching the executable again
opens the existing instance instead of starting a second client.

The default Windows microphone is selected automatically. To select a stable
CPAL device ID or exact device name explicitly, set `DOUBAO_VOICE_INPUT_DEVICE`.
The bridge address remains configurable through `DOUBAO_BRIDGE_SERVER`; the
audio port can be overridden with `DOUBAO_BRIDGE_AUDIO_PORT`.

The client opens a transparent voice capsule near the bottom of the primary
display. Its 20 cyan-to-blue waveform bars are drawn in one Canvas from one
animation clock. Activation first shows `激活中` while the Mac bridge opens and
verifies the Doubao voice input UI. The animated listening waveform appears only
after the bridge emits `phase=recording`, followed by `优化识别中` after input
ends. Speech made during `激活中` is not yet recognized and does not animate the
waveform.

The listening capsule is intentionally compact at 108 x 26 logical px: its width
is the exact 20-bar waveform width plus 30 logical px of total horizontal padding
(15 px per side). The optimizing phase reuses the same 108 x 26 logical px shell
with smaller text, so ending a recording does not resize the capsule. Windows
display scaling can make these appear larger in screenshots.

The Windows shell places a rounded native backdrop window below the transparent
GPUI overlay. DWM maintains a live thumbnail relationship with the window beneath
the capsule, so scrolling, animation, and color changes continue to update while
the voice UI is visible. Switching applications rebinds the thumbnail instead of
retaining captured pixels. A slightly smaller source rectangle is stretched into
the capsule for a refraction cue; the GPUI canvas adds the translucent tint,
moving specular band, edge caustics, lower reflection, and waveform glow.

On Windows, `Ctrl+Alt+Space` starts or finishes a session, and clicking the
capsule also finishes. The previously focused application should retain keyboard
focus. Left `Ctrl` is intentionally unbound so normal modifier-key use cannot
start voice input.

Useful verification command while the prototype is running:

```powershell
Get-Process DoubaoVoiceClient | Select-Object Id, MainWindowHandle
```
