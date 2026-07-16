# GPUI Windows Overlay Prototype Verdict

## Question

Can GPUI implement the Windows voice-input overlay while remaining transparent,
always on top, globally toggleable, clickable, and unable to steal focus from the
application receiving dictated text?

## Environment

- Host: `DESKTOP-VL6JO22`, interactive console session 1
- OS user: `ldd`
- Rust: `rustc 1.96.0`, `x86_64-pc-windows-msvc`
- GPUI: `0.2.2`, pinned in `Cargo.lock`
- Build: `cargo build --release`

## Result

**Pass, with a waveform performance caveat.**

The release executable compiled and ran in the logged-in Windows console session.
Runtime diagnostics reported:

```json
{
  "extendedStyle": "0x08200188",
  "noActivate": true,
  "toolWindow": true,
  "topmost": true,
  "overlayIsForeground": false
}
```

Automated input exercised `Ctrl+Alt+Space` twice and clicked the stop control. All
operations passed:

```json
{
  "hotkeyToggledTwice": true,
  "stopClickHidWindow": true,
  "hotkeysPreservedFocus": true,
  "stopClickPreservedFocus": true
}
```

The foreground HWND was identical before and after both keyboard and mouse tests.
This validates the critical `WS_EX_NOACTIVATE` behavior.

## Footprint

- Release executable: 10,793,984 bytes
- Working set during animation: about 41 MB
- Private memory: about 19 MB
- Process remained responsive

The current 11 independent repeating GPUI animations consumed roughly 1.58 CPU
seconds over a 7.7-second observation window on this host, or about 20% of one CPU
core. Production should use one animation clock and draw all waveform bars in one
Canvas pass.

## Capture Limitation

Both GDI `CopyFromScreen` and FFmpeg Desktop Duplication captures returned black
frames from the interactive scheduled-task environment. The HWND, visibility,
styles, input behavior, process responsiveness, and animation CPU activity were
still observable, but automated visual pixel verification remains inconclusive.
A direct console or RDP visual review is required before adopting the styling.

Local macOS compilation was also not used as evidence because GPUI requires the
optional Xcode Metal Toolchain, which is not installed. The target Windows build is
the authoritative result for this prototype.

## Decision

GPUI is technically suitable for the Windows voice-input overlay. Keep the native
Win32 layer for window styles and global hotkeys. Before integrating the voice
protocol, replace the prototype waveform with one Canvas animation and verify its
appearance directly on the Windows desktop.
