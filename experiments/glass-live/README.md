# Windows live custom-glass preview

> For ordinary material iteration, use `../glass-playground/README.md` instead. The playground uses generated fixtures, runtime production-HLSL reload and persistent build caches; this live-capture preview remains a later validation tool.

This is an independent executable in the **custom** worktree, not a deployment of
DoubaoVoiceClient. Its development no longer waits for native PR #9 acceptance.
The existing GPUI voice client, native worktree, global transparency settings,
drivers, clipboard, microphone and hotkeys are not changed by this executable.

## Run

From Windows PowerShell 5.1 or newer:

```powershell
& .\experiments\glass-live\run-preview.ps1
```

The launcher builds only this small crate (no GPUI dependency), runs the real HLSL
passes offscreen using WARP, and then **waits for explicit START confirmation**
before starting desktop capture or a visible window. Logs go to a unique TEMP
folder. Build failures stop before launch. `-BuildOnly` performs no live capture;
`-Compact` launches a 108 x 26 logical-pixel capsule instead of 180 x 40.

On the capsule: **left drag** moves, **short left click** switches light/dark,
**middle click** writes one small local PNG, **right click** closes. The illustrative
bars are NOT audio data; the label is a fixed `优化识别中` preview. No global Escape
hotkey is registered. The launcher gives it a 600-second maximum lifetime.

Direct commands, from this crate's directory:

```powershell
cargo build --release --locked
.\target\release\GlassLivePreview.exe --self-test
.\target\release\GlassLivePreview.exe --allow-desktop-capture --compact --seconds=600
```

No command without `--allow-desktop-capture` can enter live mode. `--help` and
`--self-test` do not capture. Self-test does not create visible/hidden windows.

## Implemented runtime

1. Enumerate the active primary output and create D3D11 on its associated adapter.
2. Acquire desktop frames through DXGI Desktop Duplication. Keep a GPU-only full
   frame cache so dragging can sample a different position even while the desktop
   is static. Copy only the padded lens neighborhood into processing textures.
3. Convert SDR sRGB capture pixels to linear FP16. Run separable Gaussian passes
   at two scales, with distinct input/output resources and explicit unbinding.
4. Use our capsule normal displacement and directional edge lighting. Interior
   color is already the processed background and has alpha=1; only the silhouette
   uses partial coverage, so sharp raw text is not alpha-composited back inside.
5. Apply a readability-first light/dark tone and THEN render sharp foreground.
   Text is rasterized once into an uploaded coverage mask, not blurred with the
   desktop. GDI is used for that own-text mask, not per-frame desktop capture.
6. Encode sRGB before premultiplication; present a small BGRA8 flip-sequential
   DirectComposition surface. No HostBackdrop/Acrylic APIs are used.

The original CPU reference in `../glass-material` is preserved. Runtime art
parameters intentionally differ (wider central blur, darker dark-theme response,
subtler refraction). CPU/GPU pixel parity with that reference is NOT claimed.

## Capture/privacy and snapshots

**Only this temporary preview window** receives `WDA_EXCLUDEFROMCAPTURE`, with
readback before capture/show. Failure is fatal. This prevents self-capture but
also hides the preview from many external system screenshot/recording tools.
It is NOT a per-capture-session exclusion mechanism, a DRM guarantee, or a
production recording-compatibility solution.

DXGI obtains the primary display, not only the ROI. The live loop does not map
screen textures to CPU, save screen frames or upload data. Middle-click snapshots
are enabled only when `--snapshot-dir=NEW_DIRECTORY` is supplied (the launcher
supplies a unique directory and explains this at its confirmation barrier).
A snapshot reads only the padded local crop plus final lens texture, and saves
one same-frame recomposition. It includes the user's local background pixels.
It is a **GPU-output review image, not proof of DWM's final screen presentation**.
No snapshots are automatically uploaded. Existing files are never overwritten;
snapshot count is bounded to 100 per launch.

Timeout with no new DXGI frame reuses the valid GPU cache (stationary desktop).
Capture access/device loss, format/display/DPI changes or other failures close
this experimental preview instead of silently retaining an invalid old frame.
Recovery and a production native/solid fallback are later integration work.

## Reference studied, not vendored

Reference: https://github.com/han1548772930/liquid-glass
Pinned study commit: `540a0639b62289a4d8e32f85cefa766226072f62`.

Studied its `src/main.rs` desktop-frame acquisition, GPU texture copy and capture
exclusion, and `examples/frosted_liquid/app.rs` crop/border/Gaussian processing.
Useful architectural reference: desktop GPU pixels -> blur/refraction -> window.
Its default fullscreen/two-window presentation, global Escape hotkey, assets and
error policy are NOT adopted here. This preview uses one small no-activate HWND,
our own shader and text, no global keys and explicit error/permission boundaries.

No explicit license file was present in the inspected complete tree, and the
inspected Cargo.toml contained no license declaration. No third-party source,
shader text or embedded displacement images are copied or redistributed here.
The original CPU material and this implementation are separate from that project.
Dependency versions/checksums in Cargo.lock are package metadata.

Primary API references:
- https://learn.microsoft.com/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput
- https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity
- https://learn.microsoft.com/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgifactory2-createswapchainforcomposition

## Tests and remaining acceptance

CI builds/links on Windows, executes the HLSL conversion/blur/material passes on
WARP, reads back synthetic fixtures and checks: background color separation,
opaque center/transparent corner, one-pixel stripe suppression, dark-tone bound.
PNG files are independently decoded with System.Drawing. Portable tests cover
capture opt-in, argument bounds and PNG checks; the previous CPU tests still run.
These tests do NOT establish real capture exclusion, live window presentation,
input/focus behavior, measured GPU performance, latency, or Apple visual parity.

Initial supported scope: **one unrotated SDR primary display**, DPI 96..288;
unsupported HDR/multiple outputs stop clearly without changing user settings.
No automatic monitor transition, device-recreation loop, capture-free recording
mode, GPUI shared-texture adapter, production state machine or native fallback
integration is claimed. The small standalone preview is the current deliverable.

Manual acceptance on the user's Windows machine: put real non-private Notepad
text behind it; drag/scroll/change backgrounds, switch both themes, observe crisp
foreground, absence of recursive trails and capsule-only drawing; middle-click
to save review crops. Do not force a pass by repainting another application's
window. Integrate with GPUI only after this live capture/material path is shown
working; keep the production voice client unchanged until explicitly accepted.
