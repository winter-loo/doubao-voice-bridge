# Glass delivery tracks

## A. Native readability / fallback (first acceptance gate)

Branch: `feat/glass-native-fallback`. Base: PR #7 at `dfb2b6d`.

### What failed on the real desktop

The first implementation used GPUI 0.2.2's `Blurred` background. This enables an
HWND-wide Acrylic accent. On the tested Windows 11 system at 150% DPI it produced
a 214 x 76 gray rectangle, matching the entire HWND, outside the 162 x 39 capsule.

Read-back of the actual window region matched the desired capsule. Reapplying
that region with redraw and enabling WS_EX_LAYERED did not reduce the artifact:
the original, reclip and layered cases each changed 9,452 of 25,676 measured
outside pixels, excluding a 3px edge band. These counts do not assert that every
pixel/color was identical across frames. This Acrylic version failed acceptance.
Neither workaround is treated as a fix, and layered style is not enabled by the
production implementation.

### Current native candidate: clipped host-backdrop visual

`glass_host.rs` uses Windows.UI.Composition's HostBackdropBrush and a capsule
CompositionGeometricClip. It attaches to the HWND's lower composition target;
GPUI 0.2.2 continues to own the top target for foreground text/waveform. The brush
is clipped in its own visual tree, independently of the Win32 hit-test region.
No desktop capture, GPU-to-CPU readback, microphone or third-party window is used
by the native material implementation.

Windows 11's documented DWMWA_USE_HOSTBACKDROPBRUSH opt-in is required. The old
full-window Acrylic request is not used. Unsupported systems, an unavailable
lower target, initialization errors or a disabled GPUI composition path select
solid. A creation failure is latched for the current window to avoid retries
on every animation frame. Restart after fixing an unavailable runtime condition.
The custom-material branch still contains the earlier fallback revision until
this candidate passes acceptance; do not merge unverified changes into it.

The neutral content-protection layer remains beneath the procedural highlights;
foreground text and waveforms are rendered separately and are never blurred.

Select `--glass-backend=native` (default on Windows), `--glass-backend=solid`, or
`--glass-backend=transparent` (old appearance, diagnostic ONLY). The equivalent
process-local environment variable is `DOUBAO_GLASS_BACKEND`; CLI wins. Invalid
values fail closed to solid. High contrast, a remote session, or disabled Windows
transparency selects solid for native mode, checked at most once per second.
This is not a full accessibility policy: high-contrast foreground/system-color
mapping and battery-saver transitions still need review.

IMPORTANT: a successful native visual/brush initialization is NOT visual
acceptance. Verify actual background softening, shape and foreground readability.
A selected Native backend and passing CI do not prove the displayed result.
If a target device renders an unusable native material, explicitly select solid
until the problem is resolved. Never fall back automatically to clear glass.

### Safe visual preview and repeatable shape audit

`cargo build --release --locked --bins --manifest-path clients/desktop-client/Cargo.toml`

`cargo run --release --locked --manifest-path clients/desktop-client/Cargo.toml --bin GlassPreview -- --glass-backend=native --theme=light --content=cycle --seconds=60`

GlassPreview is a separate binary: no microphone, voice controller, paste,
clipboard, tray, global hotkey, production singleton or desktop capture. It exits
after its specified lifetime or a click. It shares the production material and
texture installation functions. Theme is fixed for reproducible comparisons.
The waveform is synthetic, not voice activity.

Run the persisted audit in 64-bit Windows PowerShell:

`powershell.exe -NoProfile -ExecutionPolicy Bypass -File clients/desktop-client/verify-native-glass.ps1`

The audit requires the release preview already built. It creates its own small
text background, tests Native/Solid in light/dark, captures only that test area,
compares pixels outside the capsule to a background-only sample, and closes its
test windows. It will not stop an already-running preview or the production
client. PNGs and full logs remain in a new temporary folder; terminal output is
bounded JSON, never image Base64. `Completed` means the audit ran, not that the
material passed. A Solid selection is not evidence that native blur worked.

Also use real Notepad dense text. Compare native / transparent / solid, both
themes, and wave / activating / optimizing. Verify softened background within
the capsule, untouched background outside, stable typing focus, screen recording,
and 100/125/150/200% DPI. Do not infer application FPS from GDI screenshot samples.

Cold start still uses the caller's fallback until async frames arrive; the
preview uses an opaque fallback. Production's caller fallback is unchanged and
must be checked during cold start and policy/DPI transitions before delivery.

## B. Custom material (separate experimental track)

Branch: `feat/glass-custom-material`. The existing offline reference and HLSL
milestone are isolated from this native change. First tune deterministic fixtures:
real input pixels, blur, capsule-normal displacement, contrast/tint, edge lighting,
and opaque output in the lens interior. Only the silhouette uses shape coverage;
do not leak the sharp original desktop through a second translucent blend.

Do not enable desktop capture in the deliverable until native acceptance passes.
Then validate a GPU-backed backdrop source, exclusion/self-feedback, capture
permissions and recording compatibility, HDR/color space, DPI, latency and device
loss. Native remains available; custom failures must not block voice input.
No promises of pixel-identical Apple behavior or unmeasured 60 fps.
