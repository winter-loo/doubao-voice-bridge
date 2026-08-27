# Voice T brand sources

These are the canonical editable sources imported from the approved Voice T
v1.0 delivery. Runtime builds do not depend on the original delivery directory.

- `logo-mark.svg`: full-color mark on a transparent background.
- `logo-template.svg`: black alpha template for small system surfaces.
- `brand-colors.json`: sRGB color and gradient tokens.

Platform-ready derivatives live beside the build systems that consume them:

- macOS app icon source: `packaging/macos/AppIcon.iconset/`
- macOS runtime images: `Sources/DoubaoBridgeMac/Resources/Brand/`
- Windows and Linux assets: `clients/desktop-client/assets/`

Small menu-bar and tray images are individually pixel-aligned assets. Do not
replace them with mechanically scaled versions of the 1024 px mark.
