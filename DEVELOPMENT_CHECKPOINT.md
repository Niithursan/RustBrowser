# v0.11 development checkpoint

## Claude Code continuation (2026-11-09) — supersedes all entries below

Took over from Codex at pushed `d77d9c9` (clean tree, branch = origin). No code
was changed; this session produced evidence and documentation only.

- **Not independently verified:** remote CI. `gh` is not installed and the repo
  is private; reading the Git Credential Manager token was (correctly) refused.
  The user reported Windows CI 42679482842 and security 42679483172 green at
  `d77d9c9`. Verify the final pushed head with `gh` before marking PR #14 ready.
- Local toolchain: Rust 1.99.1 lives in `~/.rustup/toolchains/1.99.1-x86_64-pc-windows-msvc/bin`
  and is not on PATH by default; prepend it.
- **Warm tab-close failure:** not reproduced. 5-tab fresh+warm ×6 on the current
  candidate (12/12 pass, no close diagnostic emitted) and ×6 on the old
  `28b292a` candidate without the focus fix (12/12 pass), plus 4/4 in the final
  campaign. Original failure coincided with a locked desktop and logged no native
  error. Characterized as unconfirmed root cause; test builds now log it.
- **Final campaign** (`work/v11/run-final.ps1`, hashes in `work/v11/final-binaries.json`):
  frozen baseline `7f54d2bb…` vs candidate `576c6702…` (built at `d77d9c9`),
  16 cases alternating order, all 32 runs done with zero navigation errors.
  Mixed results; no performance claim. Sanitized: `benchmarks/v011-corrected.json`
  (summarizer `work/v11/summarize-final.py`).
- **30-minute soak PASSED** (`work/v11/final-soak`): 1798.9 s, 178 cycles, zero
  navigation errors, normal exit, journal only start/started/clean_shutdown.
  Sanitized: `benchmarks/v011-soak.json` (summarizer `work/v11/summarize-soak-final.py`).
- Local gates at `d77d9c9` source (docs-only changes since): fmt, strict Clippy,
  60 regular tests, all 7 separate native tests, 3/3 independent daily native
  runs, debug and release builds passed. `cargo audit --deny warnings`
  (cargo-audit 0.22.2, 1430 advisories, 369 crates) passed.
  Note: running `windows-daily.ps1` with `*>&1` redirection in PowerShell 5.1
  makes cargo's stderr a terminating error; invoke it plainly, as CI does.
- Still unverified (unchanged): final manual GUI recheck of post-`58cdef9`
  production fixes, personal sign-in, physical IME/AltGr, mixed-DPI, DRM/devices,
  screen reader, system sleep/wake, clean-machine install, signing.
- Next session: confirm final-head CI with `gh`, update PR #14 body, and only then
  decide on marking it ready. Do not merge, tag or publish.

## Codex continuation — superseded by the section above

- PR #14 remains draft. Main, tags and releases are unchanged.
- Corrected campaign session 96778 exited with failure after 13 successful cases:
  candidate five-tab repeat-2 warm run reported `Tab closing failed`. Preserve
  partial evidence; no completed corrected matrix or clean 30-minute soak exists.
- Four separate diagnostic five-tab fresh/warm cases passed; this does not explain
  the failed case. A new deterministic minimized-window regression reproduced
  tab-close activation failing with WebView2 E_INVALIDARG, native Awake/not suspended.
  Defer focus while minimized and process it on restoration; the new native regression
  and strict Clippy pass. This is not proof of the original warm-case cause.
- At 7abdfa3, push CI run 42678244042 failed controlled phase-6 HTTP navigation
  with WebView2 status 0. Audit 42678256583 passed; PR Windows 42678256422 pending.
  Added test-only scalar native start/completion/status/HTTP trace, enabled only by
  the controlled TLS fixture environment, without URLs or production diagnostics.
  All assertions remain strict. Investigate subsequent hosted failures.
- Local daily native TLS test passed with scalar trace. Final GUI recheck remains
  blocked by desktop access denied; no user unlock reply yet.
- Before measurements, freeze new candidate production source including focus fix;
  do not silently combine old and new candidate measurements or retry away failures.

## Current baseline and scope

- Read the supplied v0.11 request and AGENTS.md.
- Verified PR #13 squash-merged at `729198d023384103da0464f1148031aa6e681566`.
- Merged [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42561572861)
  and [dependency audit](https://github.com/Niithursan/RustBrowser/actions/runs/42561572932)
  passed before branch creation.
- Created `feat/v0.11-daily-driver` from verified origin/main. Production
  starts at 0.8.0. Preserve existing profiles, storage formats and WebView2.
- Servo integration is outside this milestone; retain its fixture limits.

## Current assessment and plan

The event-loop application owns stable tabs/native views, a separately
trusted toolbar, background session/SQLite workers and native security,
download, popup and sleeping-tab services. Existing regressions cover
privacy, clearing, linked popups, uploads/media and renderer recovery.
There is no bounded local application diagnostic journal or installer.

1. Capture an unchanged baseline with the existing process-tree probe.
2. Add typed, bounded local diagnostics and inspect/clear controls. Never
   log URLs, titles, forms, arbitrary errors or private-window activity.
3. Reproduce/fix modifier/repeated-key handling, navigation cancellation,
   stale completion state and actionable navigation errors.
4. Improve light-theme, focus and loading feedback in the existing toolbar.
5. Extend controlled compatibility/native coverage and manual acceptance.
6. Evaluate packaging; improve verified portable metadata/distribution and
   document installer/signing/clean-machine limitations.
7. Run production/native checks, matched benchmarks and an extended
   synthetic workload using disposable profiles. Record actual GUI scope.
8. Commit and push checkpoints, open one PR into main and verify its final
   head. Do not merge, tag, publish, deploy or enable automatic updates.

## v0.11 verified progress

- Baseline checkpoint `1e06d9f5bb29d3add3b47e7168ee429fcd22256d` is pushed.
- Captured the unchanged optimized probe (SHA-256
  `e7a54c2b4920f1df31424c7325ab08c8ae6b0c6b5dd42f0c94e38c672ef873f0`).
  All 16 baseline cases passed: 1/5/10/20 tabs, two repeats, fresh/warm
  profiles, 15 s settle, 5 s sample and 5 s close-to-one recovery. Raw
  scratch output is outside Git under work/v11/baseline-results.
- Added the typed bounded local diagnostic worker, marker/unclean-exit
  detection, settings inspection/confirmed clearing and private exclusion.
  Five new integration tests passed, including owned-process termination,
  invalid/oversized data preservation, concurrency, I/O failure and bounds.
- Reproduced AltGr interception with a failing regression, then fixed
  Ctrl+Alt routing and native repeated tab actions. Both input tests pass.
- Diagnostics/input implementation is pushed at
  `06ba97e94d4bb25602fee720f5d809ff05c2bf3b`.
- Navigation now guards completion by the current native navigation ID,
  distinguishes cancellation from failure and provides status-specific
  advice. Strict Clippy, 56 regular tests and the four affected native
  sleep/private-newtab/popup-media/renderer-recovery checks passed.
- Navigation checkpoint `4323fc36f2acbcf1226577b2e369caf27cf32182` is pushed.
- Added a controlled native daily-driver fixture covering modules, fetch,
  workers, WebCrypto, IndexedDB, cookies/local storage, SPA Back, two-origin
  redirect/storage isolation, stable controllers, overlapping navigation,
  Stop, diagnostic confirmation, light theme, address drafts and compact UI.
  The native fixture passed. It reproduced WebView2 Stop returning status 9;
  only a recorded host-issued Stop now reclassifies that aborted result.
  Genuine network aborts and certificate errors remain failures.
- Actual untrusted TLS rejection passed using a fresh OpenSSL test key and
  Python loopback TLS server outside Git. No certificate/trust settings were
  changed. The first Schannel fixture could not acquire its in-memory key;
  replacing the fixture preserved the certificate-rejection assertion.
- UI/modern fixture checkpoint is pushed at
  `1ddeb1fdf40028b60ff2aa6b6a502cc836af9eb3` (58 regular tests passed).
- Verified user-facing changes justify the intentional package jump from
  0.8.0 to 0.11.0. Added Windows SDK version/manifest resources without a
  new Cargo dependency. Release build and strict Clippy passed; PE reports
  version 0.11.0 and NotSigned. Release uses GUI startup failure feedback;
  debug retains console feedback. Panic payloads are never printed.
- Metadata/startup checkpoint `58cdef9e9017e76a03c01baebbf9b44ad17dffcd`
  is pushed. Its Windows CI run 42566063522 passed.
- Final local fmt, strict Clippy, **58 regular tests**, debug/release builds
  and current production audit (369 dependencies, no denied warnings) passed.
  All **eight** separate native Windows cases passed, including daily TLS.
  Preserved launcher safety and two Python tests passed.
- Actual isolated release GUI on 2026-11-09: public HTTPS example.com and
  rust-lang.org redirect/render, loading feedback, Light dropdown selection,
  address focus, Ctrl+T/W/Tab/D/H/J/Shift+O/Shift+P, drag reorder, pin and
  bookmark/history/download panels. Normal close/restart retained two tabs,
  active selection, pin, Light and bookmark. No personal credentials, privacy
  settings changes, permission grants, uploads or data deletion were automated.
- All 16 matched candidate benchmark cases finished, using the fixed release
  test executable at 58cdef9 (SHA-256
  `5127be5cc284c351dfdd5dbd7120458f9e3bfccda9e31c5e5d11bf4df339bda6`).
  Two repeats each fresh/warm at 1/5/10/20 tabs. No broad speedup established.
- Portable producer/safety tests passed, including equal-input ZIP identity,
  hashes, x64/version metadata and unsafe/profile/reparse output refusal.
  Packaging/workflow/install guide checkpoint `dd257c7` is pushed; CI pending.
  Ancestor-profile refusal assertions were added and await final/CI verification.
  No publication was invoked.
- The original soak was interrupted between turns: last sample 1176.707 s,
  113 churn cycles, phase soak, no completion result or live probe on resume.
  Preserve work/v11/soak-results as partial evidence; do not count it as a pass
  or infer an application crash. No matching owned engine children remained.
- A fresh 1800-second retry is active in work/v11/soak-retry-results using the
  same fixed candidate (exec session 53583, started 2026-11-09 12:45 Toronto).
  Do not run competing local builds/native tests/GUI.
- UPDATE: that retry completed 1859 s / 178 cycles and normal child exit, but
  its diagnostic journal contains 178 NavigationFailed events. It is NOT an
  error-free browsing pass. A short controlled probe reproduced status 9 on
  replacing a freshly opened about:blank. Investigating proactive supersession
  before the next NavigationStarting callback; preserve genuine network errors.
- The short probe also exposed sampler PID reuse: stale ParentProcessId links
  attributed unrelated old processes (80 alive/170 unavailable) and delayed its
  child-exit wait despite the owned browser having exited. Its owned exec session
  7356 was cancelled. Fix creation-time identity checks and bounded real-time
  child waits; invalidate/repeat affected performance numbers rather than use
  contaminated counts. Test-only error capture is uncommitted in benchmark_probe.
- UPDATE: implemented birth-time validated process trees, PID-generation-safe
  cleanup, numeric export allowlist and sampler regressions. Short native churn
  now measures 15 processes for two tabs / 14 after close, exits normally, and
  has no navigation errors. The first proactive invalidation alone was insufficient:
  initial blank NavigationStarting arrives after host replacement. Awaiting-web
  state now rejects that obsolete blank start. Genuine status-9 failures remain
  errors; Stop remains independently guarded. 60 regular tests, strict Clippy,
  sampler tests and the stronger native daily case pass. Current changes not
  committed yet. Withdrawn old comparisons must be replaced using fixed sampling.
- NAVIGATION FIX `c3ed142` / SAMPLER FIX `c991377` / withdrawal checkpoint `28b292a`
  are pushed. All eight native tests, debug/release builds, strict lint, 60 regular
  tests, package/sampler tests, five Python tests and current audit (1430 records)
  pass locally. PR CI passed at 28b292a but independent push CI failed at phase 9
  with Modern compatibility failed; broader per-API failure diagnostics are being
  added without changing production code or the frozen performance probe.
- Corrected measurements are ACTIVE in exec session 96778. Orchestration script
  `<workspace>/work/v11/run-corrected.ps1` validates frozen binaries and preserves
  completed fresh/warm pairs. Progress: `work/v11/corrected-progress.json`.
  Results: `work/v11/corrected-screening/{baseline,candidate}/tabs-N-rR/results.json`.
  It alternates version order across repeats, then runs a strict 1800 s soak at
  `work/v11/corrected-soak`. Do not launch local builds, native/GUI tests during it.
  If interrupted, inspect process state first, then rerun that script; completed
  pairs are retained. A partial continuous soak must restart in a fresh case.
- Baseline original source 729198d plus identical test-only probe; candidate
  production source c3ed142 / measured commit 28b292a. Frozen hashes are in
  `work/v11/corrected-binaries.json`: baseline 7f54d2bb7d3e30451041073bdca61c382e998980ba020bcdb6ee8b7192909a37;
  candidate 9865290a859ad80b2cfc31c8683518b77809304d7f39ba0356b856ef67f33981.
- Final GUI recheck failed GetCursorPos access denied. Asked user to unlock via
  async input; no reply yet. Stopped only the identity-verified new disposable
  GUI process and waited for its matching children before measuring. No final GUI
  pass is claimed; earlier actual GUI results remain scoped to their source.
- Documentation and sanitized matched screening are prepared for checkpoint.
  Atomic TLS publication/empty-read polling checkpoint `dd00883` is pushed;
  three new Python tests pass locally. The modern native fixture and updated
  package/ancestor-output/runtime-manifest assertions pass locally too. An
  earlier CI run 42565551944 failed with InvokeMethodOnNull in the wrapper;
  an existing-but-empty buffered port file is a concrete race consistent with
  that error. A subsequent PR CI failed at modern fixture phase 2 while the
  same-head push passed. Added per-API fixture failure stages and timeout state,
  preserving all assertions. Fmt/strict Clippy/58 regular tests and the daily
  native test pass locally; hosted evidence is still needed. Do not dismiss it.
- PE import inspection found VCRUNTIME140.dll/VCRUNTIME140_1.dll. Installation
  docs and package prerequisites now disclose x64 Visual C++ v14 as well as
  WebView2. Neither runtime is bundled/installed; clean-machine testing remains open.
- Fixture-stage diagnostics `e26e617`, runtime prerequisites `9f85af4` and native
  loopback failure reporting `c83093a` are pushed. Windows PR run 42668712718
  and dependency run 42668712732 passed at c83093a. Earlier phase-2/phase-6
  hosted failures remain unexplained; no retry or assertion relaxation was added.
  CI now requires three independent fresh-profile daily runs, every one passing.
- Documentation/screening checkpoint `3c0c975e8debd04971f45473c22aaf35522cdef1`
  is pushed. [PR #14](https://github.com/Niithursan/RustBrowser/pull/14) is open
  **as a draft**, targeting unchanged main. Do not mark ready before final checks.
- Remaining: finish/inspect soak, record numeric findings/final acceptance,
  verify handshake/package edge assertions, review diff/links/workflow syntax,
  commit/push, and verify exact final head Windows PR/push, dependency security
  and secret checks. Then mark the PR ready for review. Do not merge.

Historical results below must not be presented as new v0.11 validation.

## Current continuation

Read this section, AGENTS.md and git status first. Preserve user changes.
Use fresh marked scratch profiles outside Git. Resume completed stages
from later checkpoint entries and commits; do not restart them.

Active soak command (already launched; first inspect output/process completion):

```powershell
.\benchmarks\windows.ps1 -ProbeExecutable <scratch>/work/v11/candidate.exe `
  -OutputRoot <scratch>/work/v11/soak-retry-results -Label v0.11-58cdef9-soak-retry `
  -TabCounts 20 -Repeats 1 -SettleSeconds 30 -SampleSeconds 10 `
  -SoakSeconds 1800 -RecoverySeconds 15 -FreshOnly -Churn
```

The full request remains in attachment c561271f-eb1c-400e-aef2-8f0b20a58f3a.
Test/build/user profiles and raw measurement paths must stay out of Git/PR uploads.

---

# Historical v0.10 checkpoint (completed; do not resume its delivery)

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

The first remote boundary run on `fe092fb` rejected an elevated-token
indication on the administrative Windows hosted runner before resuming
the worker, and cleaned its profile. The launcher now derives a restricted
LUA primary token, disables maximum privileges and the Administrators SID,
then applies the same lowbox/Job/mitigation/handle attributes with
CreateProcessAsUserW. Non-elevation and deny-only Administrators membership
are verified; there is no original-token fallback. Local standard and LPAC
boundary/IPC/lifecycle controls pass. This fix is committed at
`acbf21704b4a6295afd8db2fe29f6f402f32abcd`; the
[hosted boundary workflow](https://github.com/Niithursan/RustBrowser/actions/runs/42559913981)
passed on that commit. Local three-run samples in both modes were refreshed
after filtering; the original LPAC full suite still fails 10107 with cleanup.

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
