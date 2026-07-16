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

**Pass.**

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

A macOS-side RustDesk capture of the live Windows console session confirmed that
the capsule is fully rendered with no clipping or overlap. A visual tuning pass
removed the native rectangular window frame, kept the secondary status on one
line, and replaced the placeholder pill with a recognizable microphone icon.

## Footprint

- Release executable: 10,947,072 bytes
- Working set during animation: 38,969,344-40,857,600 bytes (about 37-39 MB)
- Private memory: 18,194,432-18,321,408 bytes (about 17.4-17.5 MB)
- Process remained responsive

The original 11 independent repeating GPUI animations consumed roughly 1.58 CPU
seconds over a 7.7-second observation window on this host, or about 20% of one CPU
core. After moving all 11 bars to one animation clock and one Canvas paint pass,
the release build consumed 0.3438 and 0.3594 CPU seconds in two independent
10-second observation windows, or about 3.5% of one CPU core.

The tuned layout also passed a second automated interaction run after anchoring
the stop control to the capsule's right inset with space-between layout.

## Capture Limitation

Both GDI `CopyFromScreen` and FFmpeg Desktop Duplication captures returned black
frames from the interactive scheduled-task environment. The HWND, visibility,
styles, input behavior, process responsiveness, and animation CPU activity were
still observable. Capturing the same live desktop through the macOS RustDesk
window produced the expected pixels and was used for final visual verification.

Local macOS compilation was also not used as evidence because GPUI requires the
optional Xcode Metal Toolchain, which is not installed. The target Windows build is
the authoritative result for this prototype.

## Decision

GPUI is technically suitable for the Windows voice-input overlay. Keep the native
Win32 layer for window styles and global hotkeys. The single-Canvas waveform is
fast enough for this prototype, and the tuned styling passed direct RustDesk review.
