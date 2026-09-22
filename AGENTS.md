# Repository Agent Instructions

These instructions apply to the entire repository unless a more specific nested
`AGENTS.md` overrides them.

## Liquid Glass optical implementation rules

These rules are mandatory for work under `experiments/glass-live/`,
`experiments/glass-playground/`, and the Liquid Glass reference/docs.

### Hard prohibition: never hide backdrop glyphs by whitening the glass

When background text or glyphs remain too readable through Liquid Glass, **do not**
solve the problem by adding or increasing any white body layer, white veil, opaque
white tint, body-wide milk overlay, or `lerp(..., white, ...)` whose purpose is to
hide the background.

This prohibition includes repeatedly increasing existing `milkiness`,
`readability_veil`, `frost_milk`, body-to-white blends, or introducing a differently
named equivalent.

Existing white-mix paths are not precedent for adding more. When they are being
used to suppress backdrop glyphs, treat that responsibility as technical debt and
prefer to remove, reduce, or replace it with optical behavior.

A narrow physically motivated rim/specular highlight may approach white. That is
not permission to whiten the protected interior or turn the control into an opaque
white/gray card.

### Backdrop glyph suppression must come from optics

Unreadable backdrop glyphs should primarily result from these mechanisms:

1. **Continuous surface curvature and normals.** The straight body is relatively
   flat. Curvature must increase continuously through each shoulder into the
   circular caps; avoid a sudden body/cap transition.
2. **Normal/thickness-driven refraction.** Bend, compress, stretch, and locally
   magnify transmitted scene structure according to the surface field. Refraction
   should become much stronger near the perimeter and caps than at the straight
   center.
3. **Background-only low-frequency scattering/frost.** Remove high-frequency
   stroke identity while retaining broad light/dark scene structure. This must read
   as transmitted low-frequency environment, not uniform blur and not paint.
4. **Thick-edge optical behavior.** The perimeter should be formed by a thin outer
   reflection/highlight plus an inner refractive/caustic band and, where justified,
   a restrained transition trough caused by transmitted light.
5. **Independent foreground composition.** Voice waveform and labels remain crisp
   because they are rendered after the material. Do not whiten the glass beneath
   them as a readability hack.

### Apple reference interpretation

The canonical Apple image is **visual reference only**. Never use its pixels as
shader input, never claim Apple shader reproduction, and do not pursue pixel-identical
matching.

Always inspect magnified crops before changing the material. Prioritize these visual
features in this order:

- straight body -> shoulder -> circular-cap curvature transition;
- how backdrop strokes bend, compress, stretch, and lose high-frequency identity
  near the perimeter;
- the thin outer optical rim;
- the inner refractive/caustic band and apparent edge thickness;
- subtle background-only frosted scattering inside the body;
- retained large-scale backdrop light/dark structure;
- crisp independent foreground content.

Do **not** interpret a lighter Apple reference as an instruction to add white.
Before changing brightness, determine whether the difference is actually caused by
transmission, scattering, refraction, surface normals, optical thickness, Fresnel,
or specular response.

### Perimeter and shadow rule

Depth must come from the optical field first. Do not enlarge, darken, or reshape a
drop shadow to compensate for missing curvature, refraction, inner caustics, or
edge thickness.

When the perimeter looks wrong, inspect and fix:

- the normal-field slope through the shoulder;
- thickness-dependent ray displacement;
- cap-local radial compression/magnification;
- the outer reflection band;
- the inner refractive/caustic band;
- the transition between those bands and the straight body.

Shadow is secondary support only.

### Required visual inspection crops

A full 640px scene is not sufficient to judge Liquid Glass geometry. For every
meaningful optical iteration, inspect enlarged crops of at least:

- center straight body;
- left straight-body -> shoulder -> cap transition;
- right straight-body -> shoulder -> cap transition;
- left circular control;
- right circular control.

Judge the shape of refracted backdrop strokes at those boundaries, not only average
brightness or blur metrics.

### Iteration protocol

Before editing the shader, state the optical hypothesis being tested: curvature,
normal field, refraction displacement, scattering scale, optical edge thickness,
Fresnel/specular response, or another concrete light-transport mechanism.

Change one optical hypothesis at a time. Do not create candidate sequences whose
main axis is "more white", "more milkiness", or "stronger shadow".

For background-glyph readability work, changing a body-wide white blend is
forbidden unless the user explicitly asks to change tint/whiteness itself. Even in
that case, never justify the white blend as a glyph-hiding mechanism.

### Playground evidence rules

For visual A/B evidence:

- send keyboard commands to the real `DoubaoGlassPlaygroundHost` / process
  `MainWindowHandle`, not a material overlay HWND;
- confirm the title says `B candidate` for runtime-candidate captures;
- wait for `runtime shader ready` before editing runtime HLSL;
- confirm `adaptive.hlsl hot-reloaded` after each runtime shader edit;
- do not reuse or compare a stale `apple-optics-current.png`;
- Apple reference pixels remain display-only and never enter the D3D shader path.

### Regression and deployment gates

Do not relax deterministic thresholds to make a candidate pass.

After selecting a visual candidate, run the full `glass-check` suite and state
regressions. Preserve black/green entrance behavior, foreground contrast, waveform
palette, alpha/hidden-state contracts, shadow clearance, and reduced-motion/state
stability.

A passing test suite is not visual acceptance. Do not deploy or replace the formal
production executable until the user explicitly accepts the visual result.
