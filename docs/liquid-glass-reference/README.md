# Liquid Glass visual reference assets

These images are canonical visual-development references for GitHub Issue #13.

They are **not production inputs** and must never be fed into the shader.

## Assets

- `apple-liquid-glass-white-text-reference.jpg`
  - User-provided real iPhone Liquid Glass screenshot.
  - Visual reference only.
- `current-before-candidate-contact-sheet.jpg`
  - BEFORE/CANDIDATE white-text regression comparison produced during the readability work.
- `current-native-light.jpg`
- `current-native-dark.jpg`
  - Current implementation native-review same-cached-frame GPU composites.
  - These are not screenshots of final DWM presentation.

## What to compare

Use the reference set to judge observable material qualities:

- interior milkiness / neutral white veil
- high-frequency background glyph suppression
- retained low-frequency environment, refraction and depth
- rim clarity / edge optics
- shadow softness
- foreground text / waveform dominance
- avoidance of an opaque white-card appearance

## Current qualitative observation

The current implementation still reads as **too clean / too transparent** compared with the Apple reference.

The Apple material appears slightly **whiter, milkier and more frosted**, with stronger suppression of background glyph detail while preserving low-frequency scene structure and glass depth.

This is the visual gap the playground in Issue #13 should make cheap to iterate on.

## Constraints

- Do not claim to reproduce Apple's private shader or parameters.
- Do not use pixel-identical matching as an acceptance criterion.
- Do not feed the Apple reference into the shader.
- A/B comparisons should use the same fixture, geometry, animation phase and foreground state.
- Keep formal renderer / EXE / native-window validation separate from fast visual iteration.
