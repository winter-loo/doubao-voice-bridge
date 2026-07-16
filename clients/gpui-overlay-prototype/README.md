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
display. Its 11 waveform bars are drawn in one Canvas from one animation clock.
Press `Ctrl+Alt+Space` to hide or show it. Clicking the stop symbol also hides it.
The previously focused application should retain keyboard focus.

Useful verification command while the prototype is running:

```powershell
Get-Process gpui-overlay-prototype | Select-Object Id, MainWindowHandle
```
