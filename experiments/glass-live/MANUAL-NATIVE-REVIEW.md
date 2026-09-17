# Manual native-window review of the approved adaptive glass

Production source: `dbd4d4f0b054c8a462ecf2bb8ee7adc75ddc790c`.
Production EXE SHA256: `d0d88b08e20e4bcadb5250dad8cb9047b4bea7cb189cfaa56db66224120c26ab`.
This review branch is NOT a new product candidate. Do not advance the production
branch, replace its EXE or move the baseline tag to install a review harness.

## Reuse and isolation

`voice_window.rs` has only a test-gated module appended. Its original bytes,
all shaders, `AdaptivePipeline`, `AdaptivePresenter`, input window procedure,
`create_pair`, `configure_pair`, `move_pair`, and content masks remain unchanged.
The launcher verifies this against the production commit before build and run.
The formal product does not compile the manual-review module (`cfg(test)`).
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

## Two phases

Use the GitHub-authored `manual-native-review.ps1` from a separate, clean,
exact-commit worktree. Do not hotfix source in the local worktree. Preserve
`glass-isolated/target`, acquire its existing pipeline.lock for compilation and
keep all output outside all registered worktrees.

1. `-Mode Build`: require exact review SHA, production EXE and approved candidate
   EXE paths. Build with locked/offline dependencies, release Windows target and
   `--lib --no-run`. Copy the resulting test harness into a unique output run,
   verify its hash, and use `--list` only to resolve the exact entry. No test body
   or window runs here. Retain `build-manifest.json` and compiler logs.
2. `-Mode Review`: pass the returned manifest. Validate source and binary hashes,
   print `[USER ACTION REQUIRED]`, and wait for the user to type `REVIEW`. Only
   then launch the copied test binary with this ONE exact ignored test:
   `voice_window::manual_native_review::local_manual_native_review`.
   Never run the entire suite or a broad name filter with `--ignored`.

Windows PowerShell 5.1 launcher is UTF-8 with BOM for its Chinese instructions.
No bare host shell process is needed. Reuse the existing Environment v2 pane.

## User operation

End any active voice recording before confirming, without quitting or replacing
the installed client. The test paper is nonactivating and displays nine buttons:
white, gray, black, green, replay, simulated waveform, optimizing text, theme,
exit. Buttons affect ONLY the test. The waveform uses the existing twenty
cyan-to-blue strokes with generated amplitude, NOT live microphone samples.

Observe the white entrance and resting shadow. Click the logical capsule; click
just below/outside its body but within the shadow; drag it within the test paper;
then click its shadow again. Counters are shown along the bottom. Actual mouse
messages to the cross-thread paper are counted, not inferred from hit-test style
flags. A click in the center leaking to the paper is reported separately.

The main test checks both HWND rectangles after presentation, including margin
and sizes. The maximum normal session is 180 seconds, or click Exit. On return,
the launcher asks for a separate PASS/FAIL/UNREVIEWED visual report. An incomplete
manual gesture sequence cannot silently count as passed. A 300-second watchdog
can stop ONLY the uniquely launched review test process if it fails to return.
It never kills all cargo processes or touches the installed client.

## Evidence boundaries

- `build-manifest.json`: review SHA, production source SHA, compiled test binary
  identity and no-window build result. NOT a formal client EXE build.
- `observations/events.log`: generated control selections and owned-window events.
- `observations/native-session.json`: submission count, paired geometry checks,
  actual manual click/move counters, capture-exclusion verification and teardown.
- `human-review.json`: explicit visual report, separate from programmatic checks.
- original stdout/stderr: retained even on failure. A successful libtest exit
  means the session ran and tore down, NOT that visual or input review is complete.

There is no automatic DWM-vs-GPU pixel comparison or screenshot, no measured
hardware FPS/latency, no complex-background live sampling and no voice-session
acceptance. The generated review does not replace a later separately authorized
formal-client smoke test. Capture exclusion remains enabled on the two glass
HWNDs, so ordinary system screenshots may omit the glass. Report what is actually
seen instead of treating an offscreen image as proof of DWM output.
