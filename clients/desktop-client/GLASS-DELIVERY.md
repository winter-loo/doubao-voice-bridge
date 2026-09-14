# Glass delivery tracks

## A. Native readability / fallback (first acceptance gate)

Branch: `feat/glass-native-fallback`. Base: PR #7 at `dfb2b6d`.
The existing production texture entrypoint now requests GPUI's native blurred
window background on Windows. Surface edges stay procedural and foreground is
still rendered separately. A neutral content-protection layer is baked below
surface highlights, rather than blurring the finished UI.

Select `--glass-backend=native` (default on Windows), `--glass-backend=solid`, or
`--glass-backend=transparent` (old appearance, diagnostic ONLY). The equivalent
process-local environment variable is `DOUBAO_GLASS_BACKEND`; CLI wins. Invalid
values fail closed to solid. High contrast, a remote session, or disabled Windows
transparency selects solid for native mode, checked at most once per second.
This does not implement the full WinUI accessibility/theme policy: high-contrast
foreground/system-color mapping and battery-saver transitions still need review.

IMPORTANT: GPUI 0.2.2's setter returns no native-effect status. Its return is NOT
an assertion that the background has blurred. If native mode is ineffective or
shows a rectangular artifact on a target system, use solid pending a platform
fix. Never automatically fall back to the old highly transparent material.

### Safe visual preview

`cargo run --release --locked --manifest-path clients/desktop-client/Cargo.toml --bin GlassPreview -- --glass-backend=native --theme=light --content=cycle --seconds=60`

GlassPreview is a separate binary: no microphone, voice controller, paste,
clipboard, tray, global hotkey, production single-instance mutex, or desktop
capture. It exits after the specified duration or a click. It uses the same
native material and texture installation functions as the production client.
Theme is fixed deliberately for reproducible light/dark comparisons. Waveform is
synthetic test content, not live voice activity. Position is near the screen bottom.

Use Notepad dense text behind it. Compare native / transparent / solid, both
themes, and wave / activating / optimizing text. Verify blur inside the capsule,
untouched background outside, no rectangular bleed, stable typing focus, normal
screen capture, and 100/125/150/200% DPI. Do NOT treat compiled code as visual
acceptance. Do NOT infer application FPS from slow GDI screenshot samples.

Cold start still uses the caller's fallback until the async frames arrive; the
preview uses an opaque fallback. Production's existing fallback is unchanged and
must be checked during cold start and a policy/DPI transition.

## B. Custom material (separate experimental track)

Branch: `feat/glass-custom-material`, developed separately and incorporating A.
First prove deterministic fixture rendering: real input pixels, blur, capsule
normal-driven displacement, contrast/tint, edge lighting, and opaque output in
the lens interior. Only the boundary uses shape coverage; do not blend the sharp
original desktop through a second time.

Do not enable desktop capture in the deliverable until native acceptance passes.
Then validate a GPU-backed background source, exclusion/self-feedback, capture
permissions and recording compatibility, HDR/color space, DPI, latency and device
loss. Native remains available; custom failures must not block voice input.
No promises of pixel-identical Apple behavior or unmeasured 60 fps.
