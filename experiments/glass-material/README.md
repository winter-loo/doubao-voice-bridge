# Custom glass material: isolated fixture milestone

This experimental track starts alongside the native fallback in PR #9. It MUST
NOT replace the native deliverable before that deliverable passes real Windows
Notepad readability, clipping, focus and screenshot acceptance.

## Implemented now

- A dependency-free Rust CPU reference that samples supplied background pixels.
- Linear-light, separable Gaussian blur at two scales; bilinear displaced sampling
  along the capsule normal; separate light/dark tint and restrained edge lighting.
- Opaque lens interior with boundary coverage. The sharp original scene cannot
  reappear through a second low-alpha composition.
- Deterministic digit/text-like fixtures; P6 input for user-provided test backgrounds.
- Shader Model 5 HLSL equivalents for the blur and material passes. Compilation is
  checked separately; they are NOT yet wired into a GPU host or GPUI.

`cargo test --locked --manifest-path experiments/glass-material/Cargo.toml`

`cargo run --release --locked --manifest-path experiments/glass-material/Cargo.toml -- --output-dir=NEW_DIRECTORY`

Optionally add `--input=BACKGROUND.ppm` (8-bit P6, max 4,194,304 pixels). Outputs:
light/dark-background.ppm, light/dark-material.ppm and straight-alpha layer.pam.
Use a new output directory; existing files/directories are not overwritten. These
are real outputs of our reference algorithm, NOT Windows/iPhone screenshots.
The fixtures intentionally contain no foreground: production foreground is drawn
AFTER the material, and should not be baked into the backdrop source.

## Parameters / color contract

Geometry is in physical pixels, capsule inside supplied background. Blur sigma
and displacement scale with capsule height. Default bevel is 30% of radius,
refraction 10% of height, wide sigma 12% of height and narrow sigma 3.5% of height.
Sigma is limited to 20 pixels, matching the bounded shader radius. These are first
engineering parameters, NOT recovered Apple parameters or approved visual tuning.
Colors are processed in linear light. CPU PPM/PAM outputs are encoded as sRGB;
HLSL expects linear inputs and emits premultiplied linear RGBA. The GPU host must
respect this contract (including sRGB view formats and final output conversion).
GPU-vs-CPU rendered parity has NOT been measured; shader compilation is not parity.

## Next gate (after native acceptance)

Implement a GPU host/capture adapter, not a per-frame GDI readback. Separate live
capture, geometry and material resources. Crop with blur/displacement padding;
reject stale generations and maintain bounded resource lifetimes. Resolve actual
background capture permissions, self-exclusion and its impact on external screen
recording, HDR/color conversion, DPI, device loss and latency. No capture has been
enabled by this milestone, no display-affinity setting is changed, and no desktop
pixels are uploaded or stored by the running voice client.

Only then expose a production custom mode with runtime fallback to native / solid.
The production client in this branch still defaults to native and does NOT claim
that the fixture renderer is live GPU glass. Do not merge this experimental branch
merely because the native fallback branch passes its tests.
