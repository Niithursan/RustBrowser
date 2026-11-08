# v0.10 development checkpoint

## Completed

- Read the supplied v0.10 request and repository AGENTS.md.
- Verified PR #12 was squash-merged at
  `bba82c9b6a34c990f1e04f70c32abeda898f5e23`.
- Verified the merged tree matches the v0.9 PR head. Merged
  [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42511501005)
  and [dependency audit](https://github.com/Niithursan/RustBrowser/actions/runs/42511500878)
  passed. The matching v0.9 tree's
  [experimental boundary run](https://github.com/Niithursan/RustBrowser/actions/runs/42508530702)
  passed.
- Created `feat/v0.10-servo-secure-isolation` from verified origin/main.

## Current implementation

Production package version is 0.8.0. WebView2 handles production browsing.
The pinned Servo host accepts its owned fixtures. The v0.9 AppContainer
control passes, but the LPAC suite fails Winsock startup with error 10107.
There is no verified restricted Servo renderer.

The synthetic worker now has a fixed numeric diagnostic protocol through
two explicitly inherited pipe endpoints. Its token, Win32k and child-process
mitigations are retained. Diagnostics additionally enforce a 64 MiB
committed-memory cap, actual child-creation denial, file controls with IPC
enabled, malformed-reply rejection and a hung-worker deadline.

LPAC can read/load the provider DLL, but cannot open the Winsock catalog
keys and returns WSAStartup error 10107. This correlates with missing
catalog access and does not establish a unique root cause. Both workers
fail OpenGL loading with error 1114 under Win32k lockdown. See
`experiments/servo/isolation/DIAGNOSTICS_V010.md`.

Baseline checkpoint: `64deb8093629c7859ff0e4b296d0d04c2351d1b1`.
Initial diagnostic/IPC implementation: `de840e0cdb20528043e3a23e115ac157a55d6c63`.
Follow-up changes verify non-elevation, UIAccess, enabled privileges, live-IPC
broker termination, small-allocation controls and measured fault shutdown.
The protocol remains synthetic and grants no privileged browser operation.

## Next stages

1. Finish the rendering decision: pinned software/offscreen routes still
   need Surfman/WGL. Retained Win32k prevents OpenGL DLL initialization in
   synthetic workers. No real Servo renderer was contained; alternative
   graphics architectures remain unverified.
2. Repeat final native measurements sequentially, preserve the original
   LPAC failure, and run Servo smoke/scale regressions.
3. Update this checkpoint and the required documentation. Push verified
   commits, open a PR, and verify final CI. Leave review to the owner.

## Last successful checks

The baseline CI results above are verified. The native diagnostics compile
with /W4 /WX and pass standard AppContainer and LPAC collection, protocol
mutation tests, malformed/hang tests and broker-crash recovery locally.
LPAC's failed Winsock operation remains reported. Production formatting,
strict Clippy, regular tests, debug/release builds and audit passed locally.
All seven native WebView2 regressions passed, including the updated shared
fixture. Launcher safety checks and two Python fixture tests passed. The
separate Servo example's seven unit tests and host build with warnings
denied passed.

Actual Windows fixture input showed native key down/up, pointer clicks,
movement, wheel, focus, Unicode text entry, maximize/restore resizing and
preserved text at device scale 1. Alt+F4 closed the owned window with exit 0
and teardown_complete. Five owned run files remain for inspection. This
does not verify native IME, OS autorepeat or mixed-monitor transitions.

## Constraints and continuation

Read this file, AGENTS.md and `git status --short --branch` first.
Review `experiments/servo/isolation/`, `VALIDATION_V09.md` and the separate
source-build procedure. Keep all build output and profiles outside the repo.
Use the pinned Servo revision and lockfile when rebuilding the example.

The LPAC failure, inconclusive network timeouts, unsupported Windows Servo
sandbox, WGL/Win32k conflict, OS IME and physical-monitor validation remain
open. Do not enable arbitrary content, alter system security settings,
disable mitigations, or promote a child process as a verified sandbox.
Record any interrupted command as unfinished.
