# Windows Overlay Technical Validation

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

The release PE header reports subsystem `2` (`Windows GUI`). A before/after
RustDesk review confirmed that launching the executable from the same interactive
scheduled task no longer creates a Command Prompt or Windows Terminal window.

Automated input exercised `Ctrl+Alt+Space`. All operations passed:

```json
{
  "hotkeyStarted": true,
  "hotkeyShowedOptimizing": true,
  "hiddenAfterHotkeyOptimizing": true,
  "hotkeysPreservedFocus": true
}
```

The foreground HWND was identical before and after the global-hotkey tests. This
validates the critical `WS_EX_NOACTIVATE` behavior.

A macOS-side RustDesk capture of the live Windows console session confirmed that
the listening and optimizing phases render with no clipping or overlap. The final
UI follows the supplied Doubao references: a compact dark bottom-center capsule,
20 cyan-to-blue waveform bars, and a shorter `优化识别中` capsule.

## Footprint

- Release executable: 10,830,848 bytes
- Working set during animation: 17,289,216 bytes (about 16.5 MB)
- Private memory: 18,845,696 bytes (about 18 MB)
- Process remained responsive

The original 11 independent repeating GPUI animations consumed roughly 1.58 CPU
seconds over a 7.7-second observation window on this host, or about 20% of one CPU
core. The final 20-bar Doubao-style version uses one state/animation clock and one
Canvas paint pass. It consumed 0.4844 CPU seconds over a 10-second observation
window, or about 4.8% of one CPU core.

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
Win32 layer for window styles and global hotkeys. The hidden/listening/optimizing
state machine and single-Canvas overlay are fast enough for this prototype, and
all visible states passed direct RustDesk review. Production should drive
`Optimizing` from recognition events instead of the prototype's fixed timeout.
