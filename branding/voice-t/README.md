# Voice T brand sources

These are the canonical editable sources imported from the approved Voice T
Scheme C v2.0 delivery dated 2026-08-27. Runtime builds do not depend on the
original delivery directory.

- `logo-mark.svg`: full-color mark on a transparent background.
- `logo-template.svg`: black alpha template for small system surfaces.
- `brand-colors.json`: sRGB color and gradient tokens.

Platform-ready derivatives live beside the build systems that consume them:

- macOS App Icon source: `packaging/macos/AppIcon.iconset/`
- macOS runtime logo and 20 pt menu-bar template:
  `Sources/DoubaoBridgeMac/Resources/Brand/`
- Windows seven-size ICO and Linux 1024 px desktop icon:
  `clients/desktop-client/assets/`
- Linux 24 px tray template: `clients/desktop-client/assets/`

Small menu-bar and tray images are individually pixel-aligned assets. Do not
replace them with mechanically scaled versions of the 1024 px mark.

`SHA256SUMS` pins every selected production asset to this delivery. Run
`scripts/verify-brand-assets.sh` after replacing any brand source or derivative;
the macOS test suite and release build run the same verification automatically.

Scheme C uses five top waveform elements and one isolated vertical stem. Keep
that gap intact in every derivative; it is part of the mark geometry.
