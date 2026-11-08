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

Baseline checkpoint commit is `64deb8093629c7859ff0e4b296d0d04c2351d1b1`.

## Next stages

1. Add read-only, fixed-target Windows diagnostics that compare standard
   AppContainer and LPAC without granting broad capabilities.
2. Investigate Winsock catalog/provider access and graphics dependencies.
   Record API return codes separately from causal hypotheses.
3. Prototype bounded, authenticated synthetic broker IPC and resource-limit
   tests if those can preserve the existing creation-time mitigations.
4. Assess a restricted rendering path against the threat model. Stop at a
   documented blocker if required boundaries cannot be verified.
5. Preserve input/fixture regressions, run production and dedicated native
   validation, and record reproducible measurements.
6. Update this checkpoint and the required documentation. Push verified
   commits, open a PR, and verify final CI. Leave review to the owner.

## Last successful checks

The baseline CI results above are verified. The native diagnostics compile
with /W4 /WX and pass standard AppContainer and LPAC collection, protocol
mutation tests, malformed/hang tests and broker-crash recovery locally.
LPAC's failed Winsock operation remains reported. Production and Servo
regression checks for the new branch are still pending.

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
