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
animation clock. The UI has three visible phases: a short stop hint, the animated
listening waveform, and `优化识别中` after input ends.

The listening capsule is intentionally compact at 188 x 34 logical px. The text
hint uses 250 x 34 logical px to keep its instruction on one line, and the
optimizing capsule is 108 x 30 logical px. Windows display scaling can make these
appear larger in screenshots.

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
