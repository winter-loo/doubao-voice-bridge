# Voice activation latency: build-time shader bytecode

Base product: `dbd4d4f0b054c8a462ecf2bb8ee7adc75ddc790c`.
The user reports roughly three seconds between requesting voice input and the
formal floating bar appearing; a short utterance can finish before feedback.
This blocks live-voice acceptance despite the earlier appearance/native-input
passes. The report is not a measured cold-start benchmark.

## Corrected production path

`voice_window::Session::new` previously traversed nine HLSL compilations while
constructing the configured, optical, voice and adaptive pipeline layers. Hidden
sessions release these resources, so this is not limited to the first recording.
The fallback window is hidden before that construction and glass is shown only
after a valid capture, first render and Present. No per-stage timing was collected
for the old deployed EXE, so the exact proportion of the reported delay is unknown.

`build.rs` now compiles the nine fixed shipped source/entry/target combinations
on the Windows build host, using the original `glass-live.hlsl` source name and
Flags1=32768, Flags2=0. The bytecode and exact source strings are generated only in
Cargo OUT_DIR and embedded in the program. A shader or material-config change
reruns the build step. Windows builds require a Windows host; unsupported cross
builds fail instead of silently reintroducing runtime compilation.

Runtime lookup matches full source, entry AND target. It copies embedded bytes
into the existing blob interface, preserving shader-creation and pipeline code.
The original compiler remains for independently generated fixture, reference and
ablation sources; those must not alias a shipped source just because the entry
name is equal. This is not a persistent disk cache or a warm-only optimization.

No HLSL, optical parameters, accepted white entrance, 20-stroke palette, masking,
window geometry, capture exclusion, voice shortcut, microphone or paste code is
changed. This change does not retain desktop capture in idle, enable recording,
prewarm through a fake voice session or suppress the approved entrance animation.
Device/pipeline creation and first captured frame still take real time; this patch
does not pretend that precompiled HLSL alone proves a latency target has been met.

## Focused no-window checks

Execute these exact library tests before a new formal candidate build:

- `gpu::retained::embedded_shaders::tests::source_entry_and_target_are_all_part_of_the_key`
- `gpu::retained::embedded_shaders::tests::embedded_bytecode_matches_original_runtime_compiler`
- `gpu::retained::embedded_shaders::tests::fresh_voice_constructors_never_compile_hlsl_at_runtime`

They check exact-source rejection, nine build/runtime bytecode comparisons, and
two independent voice constructors with 18 embedded loads and ZERO runtime HLSL
compilations. Constructor checks use WARP and do not show windows or capture the
desktop. They are structural/bytecode regressions, NOT hardware startup benchmarks.
Keep the existing actual-shader state, white entrance and palette self-tests.
Do not enable the hosted-CI mouse fixture on the local desktop.

## Actual-client observations

`[voice-glass-setup]` records generation, adapter enumeration, device creation,
pipeline creation, output duplication creation and total constructor durations.
`[voice-glass-activation]` records the first accepted visible request through the
first Present/ShowWindow return for that same generation. Later state changes do
not restart this timer, and a superseded generation cannot borrow a newer timer.
These logs contain timings and generation numbers, not screen/audio/text content.

The latter interval includes worker scheduling, construction and first-frame
waiting. It excludes physical-key-to-application processing and is NOT a DWM
scanout or click-to-photon measurement. Preserve the user's first/repeated ordinary
voice-session observations alongside these diagnostics; do not replace them with
manual-harness button timings or shader microbenchmarks.

Status at authoring: source change only; Windows build, focused checks, new formal
EXE self-test and actual-client activation measurements remain pending. The
currently installed EXE, local caches, baseline tag and saved settings are untouched.
