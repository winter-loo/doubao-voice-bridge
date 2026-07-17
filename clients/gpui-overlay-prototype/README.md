# GPUI Windows Overlay Prototype

> THROWAWAY PROTOTYPE: validates GPUI transparency, animation, always-on-top,
> no-activate behavior, and a global Windows hotkey. It does not contain the
> production voice client architecture.

Run from this directory:

```powershell
cargo run --release
```

The Windows release binary uses the GUI subsystem and does not create a console
window when launched directly. Debug builds retain their console for diagnostics.

The prototype opens a transparent voice capsule near the bottom of the primary
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

On Windows, hold right `Ctrl` for 420 ms to activate and release it to finish.
`Ctrl+Alt+Space` starts or finishes a session for remote testing, and clicking the
capsule also finishes. The previously focused application should retain keyboard
focus.

Most Windows keyboards handle `Fn` in firmware and do not expose a standard
virtual key, so right `Ctrl` is the prototype's configurable stand-in. A macOS
client can bind the same hold behavior to the observable Fn/Globe modifier.

Useful verification command while the prototype is running:

```powershell
Get-Process gpui-overlay-prototype | Select-Object Id, MainWindowHandle
```
