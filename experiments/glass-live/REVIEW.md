# One-SAVE review session

Use this instead of the free-running launcher when collecting a light/dark pair:

```powershell
& .\experiments\glass-live\review-preview.ps1 -Compact
```

It builds only the standalone custom executable, keeps a backup of the prior EXE,
executes the no-capture WARP self-test, and waits for **START** before opening the
live preview or capturing the desktop. The production voice client and native
worktree are not touched. `-BuildOnly` never starts live capture. `-CompileOnly`
checks PowerShell syntax and compiles the control helper without invoking desktop
APIs, creating files or starting any process.

After START, left-drag the capsule over non-private Notepad text. Left-click can
still switch the on-screen theme. In this opt-in review mode, middle/right mouse
clicks neither save nor close. The normal launcher retains its existing controls.
The review controller owns only its newly started process; it will not close an
existing preview or restart a preview automatically.

Return to the unobstructing terminal and type **SAVE once**. A dedicated numeric
window message, not a synthetic mouse event, requests one pair. The controller
checks HWND/PID ownership; the preview accepts the one-shot request only in review
mode with capture consent and a snapshot directory. It renders both light and
dark from the same cached padded crop and animation phase, then restores the
selected on-screen theme. No new desktop acquisition occurs between these renders.

`snapshot-0001.png` is light, `snapshot-0002.png` is dark. `pair.json` is published
only after both PNGs have been written successfully; it records the PID, request,
frame counter, animation phase and geometry. Partial files are retained on an
error but never reported as a completed pair. The script independently decodes
the PNGs, checks their geometry, closes the owned preview and opens the folder.
No automatic uploads or ongoing frame recording. Cancellation also closes only
this owned preview. Maximum lifetime remains 1800 seconds.

These PNGs are **same-frame GPU composites, NOT screenshots of final DWM screen
presentation**. Capture exclusion affects other system screenshot/recording tools,
not just our acquisition. Real focus, dragging, exclusion, presentation and visual
quality still need local observation. The new mode changes interaction/reporting,
not the material shader or capture algorithm.

## Exit observability

The prior user logs reported clean `stopped` lines at 64.04 and 9.29 seconds,
without ERROR and well before configured limits. They did not distinguish a mouse
close, WM_CLOSE, WM_DESTROY or WM_QUIT, so the initiating cause cannot be recovered
from those logs. We do not infer that the user clicked the wrong button.

New logs distinguish first observed WM_RBUTTONDOWN (normal mode), WM_CLOSE,
SC_CLOSE, controller-close, WM_DESTROY, WM_ENDSESSION, WM_QUIT, lifetime-limit and
runtime-error. The controller also records the process exit code and any forced
cleanup. A message label identifies the observed path, not the sender of an
external message. System close/shutdown is not blocked; crashes or forced process
termination may prevent any final log. No automatic restart hides such failures.

## Regression coverage

Portable tests check opt-in prerequisites, one-shot request consumption and
preservation of the first exit reason. A Windows hidden-window test exercises
the actual window procedure: review mouse buttons do not destroy it, a repeated
pair request is ignored, and explicit controller close is traced. No desktop
capture or system mouse injection is used. WARP tests export a synthetic theme
pair, confirm raw background/output restoration, and check no-overwrite refusal.
These checks do not establish the original exit cause or certify local stability.
