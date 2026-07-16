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
animation clock. Activation immediately shows the animated listening waveform,
followed by `优化识别中` after input ends.

The listening capsule is intentionally compact at 108 x 26 logical px: its width
is the exact 20-bar waveform width plus 30 logical px of total horizontal padding
(15 px per side). The optimizing phase reuses the same 108 x 26 logical px shell
with smaller text, so ending a recording does not resize the capsule. Windows
display scaling can make these appear larger in screenshots.

The shell uses a translucent Oklab gradient on a transparent Windows host. A
single GPUI Canvas adds a moving specular band, top lens highlight, cyan-blue
lower reflection, and soft waveform glow to approximate a compact Liquid Glass
control without exposing the rectangular Acrylic surface around a tiny window.

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
