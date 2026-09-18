# Optical liquid glass — replacing the rejected V2.3 material

The user rejected V2.3's near-solid white/gray appearance. Its readable text and passing tests did not constitute material acceptance. This document supersedes the earlier decision to freeze that appearance. Issue #11 remains open until the user reviews the result.

## Reference, not a claim to Apple's implementation

Reviewed primary sources:
- Apple WWDC25, **Meet Liquid Glass**, https://developer.apple.com/videos/play/wwdc2025/219/ — Dynamics (1:29), Adaptivity (6:00), Principles (10:31).
- Apple, **Apple introduces a delightful and elegant new software design**, 2025-06-09, https://www.apple.com/newsroom/2025/06/apple-introduces-a-delightful-and-elegant-new-software-design/.

Apple describes lensing/bending/concentrating light, transparent background context, geometry- and motion-dependent speculars, gel-like interaction, materialization, and adaptive tint/shadows/foreground contrast. Its talk explicitly distinguishes solid fills from material tinting. These sources do not disclose Apple's shader, coefficients, geometry or a cross-platform implementation. The equations below are our independent implementation choices, not extracted Apple code or a promise of pixel identity.

## What changed in the actual runtime

`main.rs` now loads `liquid_gpu.rs`. Its public Pipeline/Presenter are the optical versions in `liquid.rs`, used by the existing desktop host, review pair and `--self-test`. The former renderer remains under `gpu::retained` for independent historical comparisons. Merely creating unused material files is not the implementation.

| Visual behavior | Actual implementation |
| --- | --- |
| Transmitted backdrop, not a white/gray plate | Refracted linear desktop samples retain substantial luminance/color response. Removed V2.x's near-constant affine paint from the live material. Final alpha is coverage/shadow; background transmission is evaluated inside the shader. |
| Curved lensing | Analytic capsule field, curved 3D normal, refracted ray, thickness-dependent displacement and mild magnification. A no-lens ablation uses identical scattering/lighting/tint for comparison. |
| Different body and perimeter | Two-scale scattering, with less scattering in the transmitting curved perimeter. Minute bounded edge dispersion, not a rainbow outline. |
| Speculars and volume | Geometry-controlled Fresnel reflection, inner caustic, local ambient color and small adaptive shadow. Contact highlight follows the actual owned-window pointer. |
| Liquid interaction | Press flattening and drag shear driven by bounded springs; release settles, no perpetual fake wobble. Appearance changes shape and lens strength. |
| Adaptive legibility | GPU-only background statistics, hysteretic light/dark ink polarity, fine local support around text strokes rather than a capsule-wide veil. Text/waveform are drawn after material. Dark theme is a smoke transmission treatment, not guaranteed white ink on a fixed charcoal fill. |
| Reduced motion | Read-only Windows client-area animation preference disables elastic deformation; does not change the user's setting. |

Bulk/edge sigma are 0.115/0.018 times physical height (4.485/0.702px for the 39px canvas). Refraction and support are bounded inside the retained crop padding. Scene pixels are current-frame, not temporally blended; only adaptation/light statistics use history. Theme pair export uses exactly one input time/state and restores the selected theme.

The only added GPU resources are one immutable R8 local glyph-support texture, an interaction constant buffer, and two 1x1 FP16 adaptation targets. The added per-frame pass renders one pixel. No user-pixel readback is used to choose colors/ink.

## Evidence and limits

CI runs the real runtime HLSL on WARP. New checks reject the previous paint-like black-to-white response, verify lens-on/off displacement, opaque glyph contrast, motion with foreground disabled, settling and exact same-time re-rendering. Historical V2.x controls remain pinned and passing independently. Artifact data in `glass-material-v2-fixtures.liquid` includes old/new images with the actual Chinese label, a no-lens comparison, and synthetic input-motion frames. Consult the current CI result; adding a test does not establish that it passed.

Synthetic input frames are not recordings of final DWM presentation, real-device latency measurements or Apple screenshots. Static-color scenes alone cannot demonstrate lens displacement; use grids/letter edges and moving scene structure as well. A full opaque-foreground contrast check does not certify every antialiased glyph or all accessibility modes.

This change is the complete new material/input path in the standalone Windows host, not a silent deployment of the production voice client. Production GPUI/voice state wiring, expanded menus or joining multiple glass controls, exit/state-transition morphing, HDR/multimonitor/device recreation, and capture-compatible external screen recording are not implemented by this commit. No native fallback acceptance is claimed. They must not be marked complete because this renderer looks better.

The host still requires explicit capture consent, excludes only its own HWND from capture, saves only on an explicit snapshot request, and closes on unsupported display/capture loss. No microphone, private image upload to GitHub, driver or system setting changes. The existing review launcher continues to work and backs up the previous executable.
