# Issue #11 — V2.3 implementation checkpoints

Related: issue #11, draft PR #10. This document tracks implementation, not visual acceptance.

## Stage 1: parameter structure and fixed V2.2 baseline

The retained `src/glass.hlsl` is the exact V2.2 source at commit
`3974edb0c7fbd0585e5f2e383b6669c9a4835288` (Git blob
`7a8126afc379257a34d27a4dbff6ac9125267eda`). CI checks that blob identity.
Its `material_ps` remains the independent comparison entry point. Do not tune it.

The runtime now uses `configured_material_ps` in `src/configured_material.hlsl`.
It shares only the original transfer, Gaussian-filter and capsule-distance helpers.
Its art parameters come from typed `BLUR`, `OPTICS`, `LIGHT` and `DARK` constants in
`src/material_config.rs`. A validated HLSL header is generated at initialization;
there is no new per-frame parsing, GPU pass, buffer or capture area.

This checkpoint intentionally preserves the existing picture. It is not a visual
V2.3 candidate and does not implement additional frosting by itself.

### Parameter semantics

| Group | Unit / meaning | Stage-one value or policy |
| --- | --- | --- |
| Center / edge blur | Sigma = fraction * physical capsule height, capped in physical px | 0.20 / 0.045; maximum 20px |
| Capture padding | Fraction of physical capsule height, rounded up | 0.65; validate three-sigma support for heights 20..120 |
| Compact example | Actual 162x39px lens at the user's last reviewed scale | center sigma 7.8px, edge sigma 1.755px, padding 26px |
| Tone/tint | Linear-light affine luminance mapping, edge and protected-body endpoints | Light and dark retain separate V2.2 coefficients |
| Chroma | Gain on scene minus linear luminance | Light edge/body 0.40/0.24; dark 0.11/0.09 |
| Tint bias | Small signed linear RGB offset | Retained; NOT sRGB 0..255 values |
| Bevel/refraction | Fractions of physical capsule height | 0.12/0.065; narrow-source mix 0.52 |
| Rim dimensions | Position/width at 26px lens, scaled by height/26 | Outer 0.50/0.35, inner 1.15/0.38 |
| Rim/surface strengths | Material-color lighting, not output/window transparency | Independent theme strengths; retained |
| Foreground | Linear RGB, drawn last | Light (0.012,0.020,0.035); dark (0.95,0.97,1.0) |
| Readability candidates | Material-internal mix, lift, saturation, local contrast and detail suppression | Reserved neutral values; stages 2/4 must wire and test them before claiming an effect |

The task's discussion ranges are candidates, not a replacement specification.
In particular, replacing the current 7.8px center sigma with an unspecified
5–7 value would not be a behavior-preserving refactor. Existing affine tone
mapping already performs tint/contrast protection; a second full-opacity tint
must not be stacked on it without a measured reason.

All theme values are checked for finiteness and documented bounds. Validation
rejects NaN/infinity and invalid weights before shader compilation. Output alpha
remains coverage only; there is no unprocessed-background alpha leak.

### Automated evidence

`src/issue11_tests.rs` compares the configured runtime to separately compiled
V2.2 entry points, on WARP without windows or desktop capture. It pins the
historical CPU sigma fractions as well as the original shader.

The matrix has 192 cases: three lens sizes (108x26, 162x39, 324x78), black/gray/
white/stripes/red-blue/synthetic letter structures, three letter offsets, both
themes, foreground on/off, and two animation phases. It compares 2,201,472 output
pixels. Acceptance is identical coverage, valid premultiplication and at most
one 8-bit level of RGB difference. These are refactor bounds, not similarity
scores, OCR accuracy or measured frame rate.

CI also runs the existing material/rim/reflection/control/pair-export tests,
portable configuration tests, release build, real-shader self-test and independent
PNG decoding. Consult the checks for the current commit; this text does not
substitute for a passing run.

Fixtures are written to the **sibling** `glass-material-v2-fixtures.issue11`
directory, avoiding a parallel creation race with the historical material suite.
Each compact text/color theme directory contains `snapshot-0001.png` (V2.2) and
`snapshot-0002.png` (configured). Metrics record maximum error and coverage.

## Remaining checkpoints — not completed by stage 1

- [ ] Stage 2: content-aware soft veil, with no needless increase over an already clean center.
- [ ] Stage 3: tune the existing two-part rim/surface reflection without restoring the broad lip.
- [ ] Stage 4: bounded detail attenuation and independent theme/foreground review with fixed scenes.
- [ ] Real Notepad / color blocks / light-gray UI / scrolling review of the actual V2.3 candidate.
- [ ] User visual acceptance; performance and final desktop presentation remain separately unverified.

The native branch and production client are unchanged. Native PR #9 is still
unaccepted. This issue does not claim a deliverable native fallback, deploy the
preview, enable a microphone, change a driver, or solve capture-exclusion/recording
compatibility, multiple monitors, HDR or GPUI integration.
