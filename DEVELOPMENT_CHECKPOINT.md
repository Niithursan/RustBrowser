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
IPC recovery/input controls: `deb5eb1e7f4f298e0a2cbcb386966d052a3719a1`.
LPAC-specific fault cases: `195c7774967fe98aef0fec4a58ad5148d4a3cd1d`.
Documented decision and numeric evidence: `ec0f627b9c6da864ea565e7edb3ab7ba10457545`.
Follow-up changes verify non-elevation, UIAccess, enabled privileges, live-IPC
broker termination, small-allocation controls and measured fault shutdown.
The protocol remains synthetic and grants no privileged browser operation.

## Completed investigation and delivery stage

The rendering decision is in `experiments/servo/SECURE_RENDERING_V010.md`.
The pinned software/offscreen routes still need Surfman/WGL. No real Servo
renderer was contained and alternative graphics paths remain unverified.
Three sequential diagnostic samples per mode, the preserved original LPAC
failure, Servo smoke and shared scale regressions are documented in
`experiments/servo/VALIDATION_V010.md` and the numeric measurement file.

The branch is pushed and [PR #13](https://github.com/Niithursan/RustBrowser/pull/13)
is open into main. Baseline remains `bba82c9`; no merge, tag or publication
was performed. Continue this PR rather than creating another one.

The PR check suite is the live remote CI record. On continuation, fetch the
PR's current head and check its Windows, dependency, synthetic-boundary and
security results before reporting success. The final response/PR validation
summary must identify the exact tested head. This checkpoint records local
results; it does not substitute for checking a newer remote commit.

## Last successful checks

The baseline CI results above are verified. The native diagnostics compile
with /W4 /WX and pass standard AppContainer and LPAC collection, protocol
mutation tests, malformed/hang tests and broker-crash recovery locally.
LPAC's failed Winsock operation remains reported. Production formatting,
strict Clippy, 47 regular tests, debug/release builds and audit passed locally.
All seven native WebView2 regressions passed, including the updated shared
fixture. Launcher safety checks and two Python fixture tests passed. The
separate Servo example's seven unit tests and host build with warnings
denied passed. Three fresh Servo smoke runs and profile-reuse refusal
passed. The four-scale shared fixture passed in three runs per engine.
Original standard AppContainer boundaries passed three final runs.
Both diagnostic modes passed token/file/process/resource controls, protocol
fault termination and live-IPC broker/profile recovery. The source package's
strict Clippy still fails on upstream GenericSender, and its lock audit
still reports rsa RUSTSEC-2023-0071 plus 11 unmaintained warnings.

Actual Windows fixture input showed native key down/up, pointer clicks,
movement, wheel, focus, Unicode text entry, maximize/restore resizing and
preserved text at device scale 1. Alt+F4 closed the owned window with exit 0
and teardown_complete. Five owned profile files remain for inspection. This
does not verify native IME, OS autorepeat or mixed-monitor transitions.

## Constraints and continuation

Read this file, AGENTS.md and `git status --short --branch` first.
Review `experiments/servo/isolation/`, `VALIDATION_V010.md` and the separate
source-build procedure. Keep all build output and profiles outside the repo.
Use the pinned Servo revision and lockfile when rebuilding the example.

The LPAC failure, inconclusive network timeouts, unsupported Windows Servo
sandbox, WGL/Win32k conflict, OS IME and physical-monitor validation remain
open. Do not enable arbitrary content, alter system security settings,
disable mitigations, or promote a child process as a verified sandbox.
Record any interrupted command as unfinished.
