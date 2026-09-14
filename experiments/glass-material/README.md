# Custom glass material: CPU reference

This is the offline reference in custom track PR #10. Native fallback PR #9 and
custom development are independent: native acceptance is NOT a prerequisite for
building or testing the custom renderer. Neither experimental track automatically
replaces the running production voice client.

The new standalone live Windows preview is in **`../glass-live`**. It implements
DXGI GPU capture, local GPU blur/material passes and an opt-in temporary window.
Read its README for capture-exclusion trade-offs, controls, tests and limitations.
It is not yet connected to GPUI or the production voice state machine. Its shader
uses revised readability-first art parameters; parity with this CPU reference is
not claimed.

## This crate implements

- Dependency-free Rust CPU reference on supplied background pixels.
- Linear-light separable Gaussian at two scales; bilinear displaced sampling along
  the capsule normal; light/dark tint and restrained directional highlights.
- Opaque lens interior with silhouette coverage: raw sharp background cannot be
  reintroduced by a second low-alpha composition.
- Deterministic digit fixtures; P6 input; PPM/PAM outputs with no overwrite.
- Original Shader Model 5 HLSL reference passes in `shaders/`.

```powershell
cargo test --locked --manifest-path experiments/glass-material/Cargo.toml
cargo run --release --locked --manifest-path experiments/glass-material/Cargo.toml -- --output-dir=NEW_DIRECTORY
```

Optionally supply `--input=BACKGROUND.ppm` (8-bit P6, max 4,194,304 pixels).
Outputs are light/dark-background.ppm, light/dark-material.ppm and straight-alpha
layer.pam. These are algorithm outputs, NOT Windows/iPhone screenshots. Foreground
is intentionally absent and should be drawn after the material.

## Reference parameters / color contract

Geometry uses physical pixels. Bevel=30% of radius, refraction=10% of height,
wide sigma=12% of height, narrow sigma=3.5% of height; sigma is bounded to 20 pixels.
These are artistic engineering values, not recovered Apple parameters.
CPU computation is linear-light; PPM/PAM encode sRGB. Reference HLSL expects linear
inputs and outputs premultiplied linear RGBA; the live host's separate shader
explicitly encodes sRGB before premultiplication for its UNORM composition surface.
Compilation alone does not establish CPU/GPU parity or visual acceptance.

## Production gates

Validate the standalone live preview with real Notepad text and movement, capture
exclusion, presentation, input/focus and hardware performance before GPUI texture
integration. Multimonitor/HDR, device recovery, normal recording behavior and
production custom/native/solid selection remain separate work. Native acceptance
is relevant to selecting a reliable fallback, not to whether custom work may start.
The production client has not been switched by these experiments.
