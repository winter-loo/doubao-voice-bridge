# Glass delivery tracks

## A. Native readability / fallback (first acceptance gate)

Branch: `feat/glass-native-fallback`. Base: PR #7 at `dfb2b6d`.

### Delivery status: native acceptance is still open

The requested first deliverable is working native background blur, and it remains
that deliverable. Solid is an interim readability baseline/emergency mode, NOT a
renaming of successful native blur. Keep this PR in draft until the native gate
passes. Do not silently redistribute its current native-default build as accepted.
The running original client is not replaced by any diagnostic.

### What failed on the real desktop

The first implementation used GPUI 0.2.2's `Blurred` background. This enables an
HWND-wide Acrylic accent. On the tested Windows 11 system at 150% DPI it produced
a 214 x 76 gray rectangle, matching the entire HWND, outside the 162 x 39 capsule.

Read-back of the actual window region matched the desired capsule. Reapplying
that region with redraw and enabling WS_EX_LAYERED did not reduce the artifact:
the original, reclip and layered cases each changed 9,452 of 25,676 measured
outside pixels, excluding a 3px edge band. These counts do not assert that every
pixel/color was identical across frames. Neither workaround is a production fix.

The clipped host-visual build at `de84613` (preview SHA-256
`66BD2FD4F5F280B17F322F57C855A8720F2BA405A21567B68536ECF2E9A459C9`)
removed the outside rectangle in the tested light/dark Native/Solid scenes.
Repeated warm background changes produced large coarse-color responses, small
same-pattern drift and very small stripe-correlated responses. This supports
native background filtering in those conditions, not general visual acceptance.

A remaining failure occurs when a new synthetic GDI background window appears
behind an already-present preview. In the failing runs, waiting from 800 to
3200 ms changed little; repainting the same background corrected a large error.
Background-first runs were much closer to the subsequent warm reference.
This is a reproducible ordering symptom, not yet a proven DWM/GPUI root cause.

The `verify-native-self-refresh.ps1` results did NOT establish a recovery:

- Light no-op/own-redraw/host-reassert retained approximately 52-54 mean RGB error
  against the warm reference. Dark host-reset retained approximately 41 error.
- Light host-reset started with only 0.96 error BEFORE the reset; dark own-redraw
  and host-reassert also started close to their reference. Those are trials in
  which the initial failure did not reproduce, not successful recoveries.
- Test-paper paint counts stayed at one before/after the candidate interventions,
  and no outside bleed was measured. These are useful controls, not proof of cure.

Do not add periodic DWM resets, blocking flushes, sleeps, or redraws of another
application to the product on the strength of these results.

### Current candidate: clean accent state plus clipped host-backdrop visual

`glass_host.rs` uses Windows.UI.Composition's HostBackdropBrush and a capsule
CompositionGeometricClip. It attaches to the HWND's lower composition target;
GPUI 0.2.2 continues to own the top target for foreground text/waveform. The brush
is clipped in its own visual tree, independently of the Win32 hit-test region.
No desktop capture, GPU-to-CPU readback, microphone or third-party window is used
by the native material implementation.

The candidate after `44ad881` removes another legacy setting during native
initialization: GPUI 0.2.2 Windows maps `Transparent` to accent state 2, not to
absence of an accent. In this pinned Windows implementation, `Opaque` sets
ACCENT_DISABLED (state 0) without replacing the DirectComposition renderer or
painting an opaque fill. Native now clears that legacy accent once BEFORE
creating/enabling the host visual. Existing host visuals are only resized; the
accent and host opt-in are not reset every frame or on every geometry change.
This is platform/version-specific and must be reviewed on a GPUI upgrade.

The presence of both mechanisms was confirmed in source; whether it caused the
ordering symptom is still a HYPOTHESIS. The new candidate needs fresh Windows
compilation and the unchanged shape/startup audits. Do not call it a confirmed fix
or hide first-presentation errors by extending warmup or repainting the test paper.

Windows 11's documented DWMWA_USE_HOSTBACKDROPBRUSH opt-in is required. Unsupported
systems, an unavailable lower target, initialization errors or a disabled GPUI
composition path select solid. A creation failure is latched for the current
window to avoid retries on every animation frame. Non-native rendering keeps its
previous configuration. No fallback to the known-broken full-window Acrylic is used.
The custom-material branch remains separate; do not merge unverified native code
into it or change the user's running original checkout.

The neutral content-protection layer remains beneath the procedural highlights;
foreground text and waveforms are rendered separately and are never blurred.

Select `--glass-backend=native` (current candidate default on Windows),
`--glass-backend=solid` (explicit interim readability mode), or
`--glass-backend=transparent` (old appearance, diagnostic ONLY). The equivalent
process-local environment variable is `DOUBAO_GLASS_BACKEND`; CLI wins. Invalid
values fail closed to solid. High contrast, a remote session, or disabled Windows
transparency selects solid for native mode, checked at most once per second.
This is not a full accessibility policy: high-contrast foreground/system-color
mapping and battery-saver transitions still need review.

IMPORTANT: a successful native visual/brush initialization is NOT visual
acceptance. Verify actual background softening, shape and foreground readability.
A selected Native backend and passing CI do not prove the displayed result.
Native source updates must not be treated as new Windows binaries until rebuilt.

### Safe visual preview and repeatable audits

`cargo build --release --locked --bins --manifest-path clients/desktop-client/Cargo.toml`

`cargo run --release --locked --manifest-path clients/desktop-client/Cargo.toml --bin GlassPreview -- --glass-backend=native --theme=light --content=cycle --seconds=60`

GlassPreview is a separate binary: no microphone, voice controller, paste,
clipboard, tray, global hotkey, production singleton or desktop capture. It exits
after its specified lifetime or a click. It shares the production material and
texture installation functions. Theme is fixed for reproducible comparisons.
The waveform is synthetic, not voice activity.

Run the persisted audits in 64-bit Windows PowerShell:

`powershell.exe -NoProfile -ExecutionPolicy Bypass -File clients/desktop-client/verify-native-glass.ps1`

`powershell.exe -NoProfile -ExecutionPolicy Bypass -File clients/desktop-client/verify-native-startup.ps1`

The first tests capsule-only clipping over light/dark text with Native/Solid.
The second retains both window-creation orders, early observations, same-content
repaint and repeated warm A/B phases separately. Reuse these tests for the new
candidate rather than changing their measurements to match a desired outcome.

The audits require the release preview already built. They capture only their
synthetic test areas and close their test windows. They do not stop an existing
preview or the production client. PNGs and full logs remain in new temporary
folders; terminal output is bounded JSON, never image Base64. `Completed` means
the audit ran, not that the material passed. A Solid selection is not evidence
that native blur worked. Trials that are already correct before an intervention
cannot count as successful repairs. Preserve failures as well as successful runs.

Also use real Notepad dense text. Compare native / transparent / solid, both
themes, and wave / activating / optimizing. Verify softened background within
the capsule, untouched background outside, stable typing focus, screen recording,
and 100/125/150/200% DPI. Do not infer application FPS from GDI screenshot samples.

Cold start still uses the caller's fallback until async frames arrive; the
preview uses an opaque fallback. Production's caller fallback is unchanged and
must be checked during cold start and policy/DPI transitions before delivery.
Solid's limited preview checks are not full production acceptance either.

## B. Custom material (separate experimental track)

Branch: `feat/glass-custom-material`. The existing offline reference and HLSL
milestone are isolated from this native change. First tune deterministic fixtures:
real input pixels, blur, capsule-normal displacement, contrast/tint, edge lighting,
and opaque output in the lens interior. Only the silhouette uses shape coverage;
do not leak the sharp original desktop through a second translucent blend.

Do not enable desktop capture in the deliverable until native acceptance passes.
Then validate a GPU-backed backdrop source, exclusion/self-feedback, capture
permissions and recording compatibility, HDR/color space, DPI, latency and device
loss. Native remains the intended fallback; custom failures must not block voice
input. Neither native nor custom is declared completed by this document.
No promises of pixel-identical Apple behavior or unmeasured 60 fps.
