# Doubao Voice Desktop Client

The release executable contains the native Rust voice client for Windows and
Linux. It captures the selected microphone through CPAL, normalizes audio to
48 kHz mono s16le PCM, streams it to the Mac bridge, receives bridge events,
and delivers recognized text without launching Python or FFmpeg. Windows writes
live recognition into the focused application; Linux commits the final text into
the focused application through fcitx5.

## Linux

Text delivery goes through fcitx5. Wayland has no equivalent of the Windows
`SendInput` path, so the client hands the recognized text to the input method
that already owns the focus of every application on the desktop. Build and
install the addon from [`clients/fcitx5-addon`](../fcitx5-addon/README.md)
first:

```bash
cmake -S ../fcitx5-addon -B ../../build/fcitx5-addon -DCMAKE_INSTALL_PREFIX=/usr
cmake --build ../../build/fcitx5-addon
sudo cmake --install ../../build/fcitx5-addon
fcitx5-remote -r
```

Without the addon the client still records, and the session reports that the
text could not be committed.

Build and start the client from an interactive desktop terminal. Keep the
process running and press the configured global shortcut to start or finish
each voice session (`F13` by default):

```bash
cd clients/desktop-client
cargo build --release
DOUBAO_BRIDGE_SERVER=MAC_IP:4387 \
DOUBAO_VOICE_SHORTCUT='CTRL+ALT+v' \
  ./target/release/DoubaoVoiceClient
```

Set `DOUBAO_VOICE_SHORTCUT` to a shortcut written in the
[XDG Shortcuts syntax](https://specifications.freedesktop.org/shortcuts/latest/):
join zero or more `CTRL`, `ALT`, `SHIFT`, `NUM`, or `LOGO` modifiers and an XKB
key name with `+`. Examples include `F8`, `CTRL+ALT+v`, and
`CTRL+SHIFT+space`. Modifier names are case-insensitive. The key name follows
XKB naming, so names such as `Return`, `Page_Down`, and `XF86AudioPlay` are
also accepted when present in the primary keymap group's base layer. Express
shifted symbols with their base key plus `SHIFT` (for example,
`CTRL+SHIFT+equal`) instead of a shifted symbol name such as `plus`.

For a persistent setting, add `voice_shortcut` to
`~/.config/DoubaoVoiceBridge/client.json`:

```json
{
  "voice_shortcut": "CTRL+ALT+v"
}
```

The environment variable overrides the JSON setting. Invalid values disable
the keyboard shortcut and leave the tray available instead of silently
grabbing `F13`.

The first shortcut press starts recording and shows the compact overlay near
the bottom of the screen. The next shortcut press, an overlay click, or `Ctrl+C`
finishes the session. The client waits up to eight seconds for the final bridge
event and falls back to the latest committed text if needed. The overlay never
takes the input focus, so the application being dictated into keeps it for the
whole session. Launching the binary again while it is already running exits
without creating a second shortcut or recording session.

The Linux client adds a system tray icon with a live status row, a
**开始语音输入** / **结束语音输入** action, and **退出**. The tray action and
the configured shortcut use the same voice-session state machine, so either
control can finish a session started by the other. If global-shortcut
authorization is cancelled on Wayland, the client remains available through
the tray. If the shortcut is already claimed on X11, the tray becomes the
fallback control.

The tray uses the freedesktop StatusNotifierItem protocol. GNOME Shell
installations without a StatusNotifier host can still use the global shortcut,
but need a compatible shell extension if a tray icon is required.

On X11, the client resolves the configured XKB key from the primary keymap
group's base layer, maps logical modifiers from the active keymap, and registers
the resulting physical-key combination directly with XGrabKey. Switching an
XKB layout group therefore keeps the shortcut on the same physical key. Caps
Lock, and Num Lock when `NUM` is not explicitly configured, do not prevent the
shortcut from working. If the X11 keyboard map changes while the client is
running, restart the client to register against the new map; tray controls
remain available in the meantime. On GNOME Wayland, the client installs a
dedicated GNOME custom keybinding that forwards activation to the already
running process over a private socket in `XDG_RUNTIME_DIR`. This avoids the
GNOME 50 GlobalShortcuts provider crash when rebinding a previously saved
shortcut. The binding is stored under
`/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/doubao-voice-client/`.

On other Wayland desktops, the client submits the configured value as the
preferred trigger through the XDG Global Shortcuts Portal. The first launch may
open a desktop authorization dialog. Approve or change the shortcut there and
keep the client running; the desktop has the final say, and the accepted
binding is printed as `global shortcut ready: ...`.

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
client continues with tray controls. It exits with an error only if the tray is
also unavailable, and never starts the microphone unexpectedly.

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

When a session finishes, the client calls `CommitString` on the addon and
fcitx5 inserts the text into whatever application holds the input focus, the
same way it inserts what a user types. This reaches GTK and Qt applications
through their fcitx5 input-method modules, XWayland applications through XIM,
and native Wayland applications through the IBus channel fcitx5 serves. Nothing
touches the clipboard, and no input-injection permission is involved.

If no application holds the input focus when the session finishes, the text is
not committed anywhere and the session reports that. To see what fcitx5
considers focused:

```bash
busctl --user call org.fcitx.Fcitx5 /voicebridge \
  local.doubao.VoiceBridge1 FocusedProgram
```

The Linux client includes a small GPUI settings window. Open it from the
system-tray **打开设置** action, or launch `DoubaoVoiceClient --settings`.
It edits the server address, audio port, and voice shortcut in
`~/.config/DoubaoVoiceBridge/client.json`; environment variables still take
precedence. The shortcut registration is initialized at client startup, so
restart the client after changing the shortcut. The **测试连接** action checks
the bridge control endpoint without starting the microphone. Clicking the
shortcut field and pressing a key combination records it automatically (for
example `CTRL+ALT+v`).

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

Windows sends each live recognition snapshot directly to the application that
had focus when voice input started. Growing snapshots append only their new
suffix; when Doubao revises the end of a phrase, the client erases only that
changed tail and writes its replacement with Unicode keyboard input. There is no
separate transcript window and the clipboard is not overwritten for routine live
updates. Keep the caret in place while speaking. If another application takes
focus, injection pauses rather than writing into the wrong window; if the final
update still cannot be delivered, the completed text is left on the clipboard.
Windows controls disagree on whether one Backspace deletes a whole emoji or
combining sequence, so a revision that would need to erase one pauses before
changing the field; the completed final text is then left on the clipboard.

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
