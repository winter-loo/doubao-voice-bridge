# Liquid Glass playground

This is the Issue #13 **L1 development loop** for Liquid Glass material work. It
is a small Win32 + D3D11 executable. It does not depend on GPUI and does not use
the microphone, network, desktop capture, startup settings, the production
single-instance mutex, or production EXE replacement.

## Source-of-truth rules

The playground does not own a shader copy.

- Runtime HLSL always reads the production files in
  `experiments/glass-live/src/`, including the real
  `experiments/glass-live/src/adaptive.hlsl`.
- The visual reference always reads
  `docs/liquid-glass-reference/apple-liquid-glass-white-text-reference.webp`.
- The Apple image is decoded only for a GDI side panel. Its pixels are never
  uploaded into the D3D material pipeline and are never shader input.
- The Apple image is a visual benchmark for milkiness, background-glyph
  suppression, retained low-frequency environment, rim optics, shadow softness,
  and foreground dominance. Pixel-identical matching and Apple shader
  reproduction are non-goals.

## Fast DEV loop

Recommended on Windows PowerShell:

```powershell
.\experiments\glass-playground\glass-playground.ps1
```

The script reuses `.target\glass-dev` so ordinary iteration keeps Cargo
dependency/build caches. The equivalent direct command is:

```powershell
cargo run --locked --manifest-path experiments/glass-playground/Cargo.toml
```

The intended loop is:

```text
edit tuning.json or adaptive.hlsl
-> save
-> already-running preview updates
```

`adaptive.hlsl` is polled every 25 ms. On a save, both production adaptive
entry points are compiled at runtime before either shader is replaced. A compile
failure leaves the last-good shader active, shows the error in the window title
and console, and retries on the next save. The window title also reports the
last successful HLSL compile duration so the <500 ms Issue #13 target can be
measured on the actual Windows development machine instead of being assumed.

For the DEV-only compile path, the tracked production HLSL expresses loop hints
through `LIQUID_UNROLL`. Formal/build-time compilation maps that macro to
`[unroll]`; the playground defines it empty and uses
`D3DCOMPILE_SKIP_OPTIMIZATION`. The runtime still compiles the tracked production
shader sources directly: there is no copied shader and no runtime source-text
rewrite. On the current Windows development machine, eight consecutive
successful dual-entry hot reloads measured 251-307 ms (281 ms median), with the
initial runtime compile at 300 ms. `glass-check` compares the DEV-compiled shader
against the production embedded shader (alpha exact, max RGB byte error <= 1)
and verifies that a second-entry compile failure preserves the atomic last-good
shader pair.

`tuning.json` is also polled every 25 ms. Valid values update D3D constant
buffer `b4`; no HLSL compilation or Rust rebuild occurs. Invalid tuning keeps
the last-good values. Press `V` to persist the current keyboard-adjusted B
values back to that same dev file.

## Controls

| Key | Action |
| --- | --- |
| `1` | white background + deterministic dense black text |
| `2` | pure white |
| `3` | dark |
| `4` | green |
| `5` | colorful/high-chroma |
| `6` | Apple-optics stress fixture: oversized black text crosses both caps |
| `S` | steady material, no foreground |
| `Space` | deterministic listening waveform |
| `A` | activating |
| `T` | optimizing text |
| `Tab` | instant A/B: A production baseline / B candidate |
| `D` | light/dark theme |
| `Left/Right` | select runtime material parameter |
| `Up/Down` | change selected parameter for B on the next frame |
| `P` | reset B tuning to production defaults |
| `V` | persist current B values to `tuning.json` |
| `R` | show/hide canonical Apple reference side panel |
| `Esc` | close |

A and B are two production `AdaptivePipeline` instances on the same D3D
device/context. Fixture, geometry, state, deterministic input and frame time are
shared. Both histories are reset together when a fixture/state/theme or
successful runtime shader reload changes the comparison. A keeps the
build-time production shader/default tuning; B uses the runtime-compiled
production source and candidate tuning.

## Runtime tuning

`experiments/glass-playground/tuning.json` contains the production defaults.
The exposed values are intentionally limited to high-value visual iteration
axes: scene/local readability guard and veil, bright-neutral thresholds,
dark-detail and complexity thresholds, frost strength, milkiness, and protected
interior thresholds.

Default values preserve the pre-playground production material. The playground
does not expose every shader literal.

## Fast deterministic L2 check

Run:

```powershell
.\experiments\glass-playground\glass-check.ps1
```

or:

```powershell
cargo run --locked --manifest-path experiments/glass-playground/Cargo.toml -- --check
```

This reuses the existing strict production white+black-text WARP regression,
including its byte-validated fixture upload and readability thresholds. It does
not open a window or use desktop capture/audio/network. The output directory
contains:

```text
metrics.json
steady.png
waveform.png
text.png
contact-sheet.png
```

The original regression evidence files are retained as well. The contact sheet
is post-processing of those generated regression PNGs; it is not material input.

## Validation layers

**L1 — playground:** use for shader/tuning iteration, deterministic fixtures,
state switching, same-frame A/B, and Apple visual reference. No GPUI and no
desktop capture.

**L2 — production renderer fixtures:** run `glass-check.ps1` plus the existing
`glass-live`/voice renderer tests. This is where white-text readability,
entrance/recovery, black/green/colorful backgrounds, the original 20-bar
`#43DED2 -> #648DFF` palette, alpha/shadow/hidden state, reversal and held-scene
contracts remain regression gates.

**L3 — formal client/native window:** keep the existing locked release build,
`DoubaoVoiceClient.exe --liquid-glass-self-test`, real GPUI/native-window
presentation, DPI/DWM/focus/dragging/activation checks and controlled deployment
as a separate release gate. Do not run L3 for every material edit and do not
weaken its thresholds because L1 is faster.

## Safety boundary

Default playground operation uses generated local fixtures only. It never asks
for or grants desktop-capture consent. It has no microphone/network/clipboard
voice path, does not write user configuration or startup settings, does not
replace or start the production EXE, and does not acquire the production
single-instance mutex.

The canonical Apple image is display-only. Adding future live desktop capture
would require a new explicit opt-in consent boundary; it is intentionally absent
from this implementation.
