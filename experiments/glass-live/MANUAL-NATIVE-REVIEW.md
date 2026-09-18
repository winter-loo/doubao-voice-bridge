# Manual native-window review of the approved adaptive glass

Production source: `dbd4d4f0b054c8a462ecf2bb8ee7adc75ddc790c`.
Production EXE SHA256: `d0d88b08e20e4bcadb5250dad8cb9047b4bea7cb189cfaa56db66224120c26ab`.
This review branch is NOT a new product candidate. Do not advance the production
branch, replace its EXE or move the baseline tag to install a review harness.

## Reuse and isolation

`voice_window.rs` has only a test-gated module appended. `adaptive.rs` likewise
only appends a test-gated replay-reset module. Their original function bodies,
all shaders, `AdaptivePipeline`, `AdaptivePresenter`, input window procedure,
`create_pair`, `configure_pair`, `move_pair`, and content masks remain unchanged.
The launcher verifies the exact append-only differences against the production
commit before build and run. Neither helper is compiled into the formal product.
The test uses the production adapter enumeration and hardware device, logical
108x26 DIP body, independently padded canvas, DComp presenter, capture-exclusion
calls, actual mouse procedure and paired-window move/teardown path. It submits
continuously after ShowWindow rather than treating one hidden Present as proof.

The review paper belongs to a second thread in the same test process. Its
uniform white/gray/black/green source color is also uploaded as checked generated
BGRA bytes to the shader. There is NO Capture/Session construction, desktop
acquisition, screen-pixel readback, SendInput, SetCursorPos, hook, hotkey, network,
microphone, clipboard, installed-client launch or settings/startup write.
`choose_output` enumerates the display/adapter only; its legacy log text is not
a claim that this harness captures the display.

The normal hosted-CI native fixture is unchanged and must never be enabled on
the user's everyday desktop. The new entry rejects CI flags and needs its own
explicit consent. It does not call the old fixture or fake its PASS marker.

## Responsiveness correction

The first review harness (`3b32b42c`) recreated `Pipeline::new_voice` after every
control revision. That constructor compiles shaders and allocates GPU resources
on the presentation thread. The user reported a long wait after replay, background
and waveform clicks. Passing mouse-route counters did NOT validate response time.
No measured per-stage timing was collected in that first harness; do not invent it.

The corrected harness constructs the pipeline once before displaying controls.
It pre-rasterizes the two content masks, reuses GPU resources/presenter/windows,
uploads generated source bytes only when the source changes, and updates the
existing mask only when the content mode changes. Replay resets material history,
foreground/motion clocks and springs without compiling or allocating GPU objects.
Every action still replays the approved entrance, preserving the former review
semantics. Reduced-motion settings and the HWND binding survive the reset.

A no-window WARP regression compares reused output with independently constructed
fresh pipelines on 36 full-canvas samples, including white, green, listening,
dark theme and reduced motion. It also checks stable resource identities and
same-time rerendering. This is a pixel-preservation regression, not a latency test.

For actual manual controls, `events.log` records the request revision, skipped
superseded revisions, queue/paint waiting, mask update, source upload/prepare,
state reset, render submission and Present-call durations. The elapsed interval
starts on receipt of WM_LBUTTONUP and ends at Present return. It is NOT physical
click-to-photon latency, DWM scanout time or measured FPS. A 100ms flag is diagnostic
only; the timing sample count, maximum and flag count are separate from input
routing and human acceptance. Initial setup has its own duration and is not a
button-response sample. Observe actual responsiveness instead of trusting counters.

## Two phases

Use the GitHub-authored `manual-native-review.ps1` from a separate, clean,
exact-commit worktree. Do not hotfix source in the local worktree. Preserve
`glass-isolated/target`, acquire its existing pipeline.lock for compilation and
keep all output outside all registered worktrees.

1. `-Mode Build`: require exact review SHA, production EXE and approved candidate
   EXE paths. Build with locked/offline dependencies, release Windows target and
   `--lib --no-run`. Copy the test harness to a unique output run, verify its hash,
   list exact entries, then execute ONLY
   `gpu::retained::optical::adaptive::manual_replay_reset::reused_manual_replay_matches_fresh_pipeline`.
   Require one executed/passed test and the 36-comparison marker. No visible or
   hidden window is created by this regression. Retain compiler/regression logs
   and `build-manifest.json`; failed builds must not start a manual session.
2. `-Mode Review`: pass the returned manifest. Validate source, binary and replay
   regression identities, print `[USER ACTION REQUIRED]`, and wait for the user
   to type `REVIEW`. Only then launch the copied test binary with this ONE exact
   ignored test: `voice_window::manual_native_review::local_manual_native_review`.
   Never run the entire suite or a broad name filter with `--ignored`.

Windows PowerShell 5.1 launcher is UTF-8 with BOM for its Chinese instructions.
No bare host shell process is needed. Reuse the existing Environment v2 pane.
Do not synchronize over an active review or its pending terminal questionnaire.
Keep earlier build/session files unchanged; use a new build and new consent.

## User operation

End any active voice recording before confirming, without quitting or replacing
the installed client. The test paper is nonactivating and displays nine buttons:
white, gray, black, green, replay, simulated waveform, optimizing text, theme,
exit. Buttons affect ONLY the test. The waveform uses the existing twenty
cyan-to-blue strokes with generated amplitude, NOT live microphone samples.

Observe that replay/background/waveform respond promptly, and inspect the white
entrance and resting shadow. Click the logical capsule; click just below/outside
its body but within the shadow; drag it within the test paper; then click its
shadow again. Counters are shown along the bottom. Actual mouse messages to the
cross-thread paper are counted, not inferred from hit-test style flags. A center
click leaking to the paper is reported separately. Request/applied revision numbers
show which selection has actually reached a Present call.

The main test checks both HWND rectangles after presentation, including margin
and sizes. The maximum normal session is 180 seconds, or click Exit. On return,
the launcher asks for a separate PASS/FAIL/UNREVIEWED appearance/responsiveness
report. An incomplete gesture sequence cannot silently count as passed. A
300-second watchdog can stop ONLY the uniquely launched review test process if
it fails to return. It never kills all cargo processes or the installed client.

## Evidence boundaries

- `build-manifest.json`: review SHA, production source SHA, compiled test binary
  identity and no-window replay regression. NOT a formal client EXE build.
- `observations/events.log`: generated control selections, stage timings and
  owned-window events; contains no transcription or desktop pixels.
- `observations/native-session.json`: submission count, paired geometry checks,
  actual manual click/move counters, response samples, capture exclusion, teardown.
- `human-review.json`: explicit appearance/responsiveness report, separate from
  programmatic checks. Mouse-route PASS alone is never an overall native PASS.
- original stdout/stderr: retained even on failure. A successful libtest exit
  means the session ran and tore down, NOT that user acceptance is complete.

There is no automatic DWM-vs-GPU pixel comparison or screenshot, no measured
hardware FPS/latency, no complex-background live sampling and no voice-session
acceptance. The review-tool correction does not prove the formal client's cold
session startup is fast; that remains a distinct measurement. Generated review
does not replace a later separately authorized formal-client smoke test.
Capture exclusion remains enabled on the two glass HWNDs, so ordinary system
screenshots may omit the glass. Report what is actually seen instead of treating
an offscreen image as proof of DWM output.
