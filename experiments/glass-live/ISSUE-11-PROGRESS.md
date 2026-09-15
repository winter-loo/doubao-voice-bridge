# Issue #11 — V2.3 implementation checkpoints

Related: issue #11, draft PR #10. Code implementation is not user visual acceptance.

## Current candidate

Stages 2–4 are wired into `configured_material_ps`. The live `Pipeline::new`,
review preview and executable self-test all select `Profile::Candidate`.
`material_config.rs` provides four explicit compile-time profiles for comparisons:

1. `V22`: neutral configured reference.
2. `Veil`: V2.2 plus the soft content-local veil only.
3. `Rim`: veil plus thinner, softer paired rim highlights.
4. `Candidate`: the above plus theme-specific residual attenuation and saturation.

No new GPU pass, texture, capture area, raw-background alpha mixing, microphone,
network upload, temporal filter or per-frame configuration parsing is introduced.
The two blur sigmas, source padding, foreground colors/layout/animation and
existing V2.2 area-light reflection are retained.

### Stage 2 — soft content veil

The rounded soft support is centered at normalized `(0.47, 0.50)`, with full
extent `(0.84, 0.76)` relative to capsule width/height. The 84% width is deliberately
wider than the task's initial 70–78% discussion range to include the left waveform
and text together. It is the outer support, NOT a uniformly opaque patch.
Corner radius is `0.18 * height`; its inward soft transition is `0.10 * height`.
A separate inset gate leaves the outer `0.08 * height` completely untouched and
reaches full strength at `0.20 * height`.

The mask is fixed to content geometry, not inferred from background letters, so
this material introduces no background-dependent mask switching. It is applied
before rim/sheen and before sharp foreground rendering.

| Internal linear-light parameter | Light | Dark |
| --- | --- | --- |
| Veil RGB | `(1,1,1)` | `(0.060,0.060,0.060)` |
| Maximum blend weight | 0.10 | 0.08 |
| Maximum additive lift | 0.0010 | 0.0015 |

These are conservative linear-light defaults, not an unqualified copy of the
issue's unit-ambiguous lift ranges. GPU tests bound the added veil's RGB shift,
spatial support, smooth transition and foreground contrast. Actual Notepad
appearance still requires a local review.

### Stage 3 — dual rim

Rim positions/widths are specified at a 26px physical lens and scaled by
`height / 26`. Outer position stays 0.50, width narrows 0.35 → 0.30. Inner
position/width change from 1.15/0.38 to 1.00/0.34.

| Strength [inner shadow, outer primary, outer opposite, inner lift] | V2.2 | Candidate |
| --- | --- | --- |
| Light | `[0.05,0.70,0.025,0.006]` | `[0.035,0.48,0.025,0.007]` |
| Dark | `[0.08,0.17,0.008,0.004]` | `[0.060,0.14,0.010,0.005]` |

The existing directional arc and area-light sheen are reused, not stacked with
another gradient. The stage-3 ablation is required not to change the protected
interior. This is a restrained refinement, not a claim of Apple shader identity.

### Stage 4 — bounded residual attenuation

Both samples already exist: `wide` and `narrow` Gaussian textures. Start from the
existing bevel-weighted blend, then interpolate it toward `wide`. Compress its
luminance residual relative to that same wide sample, reduce chroma slightly,
and clamp to the valid linear RGB range before theme tone mapping.

| Parameter | Light | Dark |
| --- | --- | --- |
| Detail attenuation | 0.50 | 0.44 |
| Local luminance residual scale | 0.85 | 0.82 |
| Saturation multiplier | 0.97 | 0.94 |

This is reuse of the existing two scales, not a newly invented frequency detector.
It mainly suppresses residual structure near the edge; the protected center was
already using the wide sample. It does NOT reintroduce the narrow texture at the
center merely to remove it again. The content veil additionally contracts central
scene response. Large background color fields must remain distinguishable.

Foreground remains the original light `(0.012,0.020,0.035)` and dark
`(0.95,0.97,1.0)` in linear RGB. Changing already-readable glyphs was not necessary;
opaque foreground equality and synthetic contrast are checked instead.

## Stage 1 remains a fixed independent control

`src/glass.hlsl` is the literal V2.2 source from
`3974edb0c7fbd0585e5f2e383b6669c9a4835288`, Git blob
`7a8126afc379257a34d27a4dbff6ac9125267eda`. CI checks its identity. Do not tune it.
Its transfer, Gaussian-filter and capsule-distance helpers remain shared.
The neutral configured profile is compared with the independently compiled
original entry points; it is not mislabeled as the new runtime default.

| Shared geometry/filter parameter | Value |
| --- | --- |
| Wide sigma / physical height | 0.20, capped at 20px |
| Narrow sigma / physical height | 0.045, capped at 20px |
| Padding / physical height | 0.65, rounded upward |
| Compact 162×39px lens | sigma 7.8 / 1.755px, padding 26px |
| Final alpha | silhouette coverage only; processed interior is opaque |

Stage-1 checkpoint `7c1e25a` passed its CI. The 192-case matrix compares 2,201,472
pixels across three sizes, six generated background kinds, letter offsets,
themes, foreground on/off and phases. Its existing one-level RGB tolerance and
exact-alpha assertions remain unchanged. Earlier measured maximum error was 0;
consult the current run for current evidence.

## Automated evidence and artifacts

`material_tests.rs` explicitly retains historical V1/V2/V2.1/V2.2 contracts.
`issue11_tests.rs` retains neutral-profile refactor parity. Neither is substituted
for testing the candidate: `v23_tests.rs` constructs the real runtime default and
independently compares it with the stage profiles.

Candidate tests execute HLSL on WARP with generated input, no desktop capture:

- Soft-mask output against its specified geometry at 108×26, 162×39, 324×78.
- Current/staged materials on black, gray, white, checker/stripe, red-blue and
  synthetic letter backgrounds, both themes: 36 scene/size/theme cases.
- Exact silhouette, valid premultiplication and opaque foreground preservation.
- Minimum synthetic opaque-foreground contrast of 7:1 (a design bound, not a
  full accessibility certification or a test of every antialiased glyph edge).
- Bounded veil change and finite soft transitions, protected body preservation
  across the rim stage, nonconstant color transmission and edge-detail response.
- Identical repeated input/phase and generated A→B→A have identical output;
  this detects introduced history, NOT live smoothness, latency or displayed FPS.

The sibling artifact directory `glass-material-v2-fixtures.issue11-v23` contains
48 PNGs: six backgrounds × two themes × four stages. Within each directory:
`snapshot-0001.png` = V2.2, `0002` = veil, `0003` = rim, `0004` = candidate.
`metrics.json` and `parameters.txt` record measured results and active constants.
CI independently decodes PNGs and checks the declared counts/dimensions.
Always consult the completed run; adding these tests does not itself prove PASS.

## Status and manual gates

- [x] Stage 1 implemented; original successful CI recorded in issue #11.
- [x] Stage 2 veil implemented and stage-specific tests added.
- [x] Stage 3 existing rim refined; existing sheen intentionally retained.
- [x] Stage 4 residual attenuation wired; synthetic comparisons/tests added.
- [ ] Current candidate CI outcome and exported images reviewed (see issue comments).
- [ ] A: real Notepad text crossing the internal content region, both themes.
- [ ] B: real moving/changing large color blocks.
- [ ] C: ordinary light-gray UI, no hard patch or broad rim.
- [ ] D: scrolling/moving background; no new objectionable flicker/trailing.
- [ ] User visual acceptance; performance/final DWM presentation separately reviewed.

After CI, sync only the committed custom branch. Use `review-preview.ps1 -Compact`:
START before desktop capture, one SAVE for the same-frame light/dark pair. Clearly
announce any attachment of the selected PNGs and use short-path Agent upload
markers; never request local source scanning or treat internal composites as
screenshots of final DWM presentation.

Native PR #9 remains unaccepted; native and production code are unchanged. Do not
merge/deploy automatically, change drivers/settings, enable a microphone or claim
that capture-exclusion/recording compatibility, multi-monitor/HDR handling or GPUI
integration are solved by this material-only work.
