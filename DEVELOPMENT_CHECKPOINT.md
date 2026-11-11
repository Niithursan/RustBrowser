# Development checkpoint

## v0.18: multiple normal windows (Claude Code, in progress)

Branch `feat/v0.18-multiple-windows` from verified origin/main `7a61d97` (PR #23;
CI 43063434943 and security 43063435068 passed). Scratch `<workspace>/work/v18`;
gate script `work/v18/gates.ps1`. Owner decisions: session option B, two-step
refactor, v0.17 interpretations approved, "new window" opens a real window.

Done:
- `231c9bf` recovered the unpushed v0.17 checkpoint note; `c1b6dcb` owner v0.17
  results (steps 12 to 16, bundle o-4349a481) in VALIDATION and RELEASE_READINESS.
- Step one, behaviour-neutral split: `5c88b6e` (WindowState per window, Session
  profile services, Deref to the current window, process-wide tab IDs via
  `Tabs::with_ids`, Event::WindowCommand for toolbar IPC, routing by window, tab
  and toolbar source) and `10e653a` (two fixtures' event probes recognise the
  routed command form; no assertion changed). The first gate run failed in
  `native_content_find_zoom_print_boundary` step 46 for that reason. All 33 local
  gates then passed on `10e653a`.

Draft PR #24. Step one CI green on `eb80224`: CI 43066449889, security
43066449940. Then `65d7c50` weekly Dependabot, `df67871` README official-build
statement and SECURITY reporting wording (private vulnerability reporting was
still disabled when checked: `enabled: false`).

Step two, multiple windows:
- `57d73a6` format option B (`storage::WindowSnapshot`, `other_windows`,
  `MAX_WINDOWS` 8, `storage::fits`), 7 tests in `tests/session_windows.rs`.
- `11dda94` windows: open (Ctrl+N, More > New window, hand-off), limit notice,
  close one window (retired after `RETIRE_DELAY`), Reopen closed window, Exit,
  save and restore, engine restart and clearing across windows, per-window
  toolbar checks, permissions only for the focused window.
- `ee0b8a4` two defects found by the new native suites, reproduced first:
  (1) the Focused handler moved page focus, so three windows traded focus
  forever and the loop never idled (pages stuck on "Connecting"); (2) restored
  windows' toolbars took focus as they loaded (wry `with_focused` default), so the
  last restored window became "most recent". Fixes: focus events only claim
  permissions; restored windows are created inactive with unfocused toolbars;
  page focus is never moved inside a background window. Adds
  `src/windows_validation.rs` (3 native suites).
- `46a9304` `tests/single_instance.rs`: new-window hand-off opens a window;
  killed-browser restore of two windows. Only "Window Class" windows are counted
  (winit's event target window reports visible).
- Fixture lesson: creating windows from `about_to_wait` wedges winit's loop; the
  fixtures send window requests through the proxy, as production does.
- Installer upgrade fixture now pins v0.17 `7a61d97` (cache key
  installer-fixture-v6, `target-v017`); 0.17 Setup already registers, so the
  fresh-install check expects registration.

- `81e3a54` engine suite strengthened after two mutation survivors; all 10
  mutation checks caught (`work/v18/mutate.py`). `7076c01` CI and release run
  the window suites; `99a0bb7` installer upgrade from 0.17; `4044820` 0.18.0;
  `1aba4dc` docs; VALIDATION in the following docs commits.
- All 36 local gates passed on `1aba4dc` (`work/v18/production-gates.json`); the
  commits after it change documentation only.

- CI 43085823286 on `10a51d8` failed the restore suite (security
  43085823200 passed): Exit saved the window Windows activated while hiding the
  others. Reproduced locally by delivering that focus event, fixed in `335ba12`
  (focus ignored during shutdown); all 36 local gates passed on `335ba12`.

Next: CI on the pushed head (results in PR #24 comments, not here, to avoid
another run), owner bundle from that head, owner checklist steps 12 to 21.
Stop at the draft PR: do not merge, tag or mark ready without the owner.

## v0.17: link handling (Claude Code, in progress, owner away)

Branch `feat/v0.17-link-handling` from verified origin/main `96a70a2` (PR #22; CI
42945393777 and security 42945393781 passed). Scratch `<workspace>/work/v17`.
No PR yet: branch pushes start no workflow; open the draft PR once, then push
once more for the final head. Do not rerun or dispatch workflows.

Plan and order (advisor-reviewed): 1 command line, 2 hand-off, 3 installer
registration, 5 address-field menu, then 4 multiple windows (may not land; if
its refactor is not green, deliver the design and session-format proposal).

Done:
- `a2aa270` item 1. Reproduced first: invalid UTF-16 argument panicked (exit 101)
  and unknown options were ignored. Pure parser `src/launch.rs` over the raw
  GetCommandLineW text; `--single-argument` takes the rest verbatim as one link;
  `navigation::external` requires explicit http(s). Decision: local .htm/.html
  are NOT accepted (typed file: is refused today), so registration is http/https
  only. Refusals exit 2 with a message. Profile lock now taken before the event
  loop (`storage::lock`, `Store::open_locked`, `App.start`). Tests: 6 unit tests,
  `tests/command_line.rs` (spawned exe, raw_arg).

- `e58d507` item 2 hand-off (`src/handoff.rs` pure format/limits/trust,
  `src/native_handoff.rs` pipe server/client, UI answers via `Event::HandOff`,
  `main.rs` `start_or_hand_off` retries 6 s and explains every outcome, exit 3).
  8 transport tests + 5 mutants caught; `4bdd7a2` two-process suite
  `tests/single_instance.rs` (hand-off, new window, refused link, crash takeover;
  2 mutants caught; drop guard stops leaked test browsers). SECURITY.md threat
  model written.
- Item 3 committed (`b19fc27` registration and button, `61302ef` lifecycle and
  real-defaults guard, `6ed244b` 0.17.0, `0a8df20` v0.16 fixture, cache key
  installer-fixture-v5, first CI run cold). Lifecycle passed from a real 0.16
  install (baseline built with the pinned 0.16 installer.iss); interrupted sweep
  16/16; real UserChoice and RegisteredApplications unchanged; 1 mutant caught (its
  leftover validation value was removed by hand afterwards).
- `d145a28` item 5 address-field menu (`src/native_clipboard.rs`, test build uses
  an in-memory clipboard; `native_address_field_edit_menu`, 3 mutants caught).
- Item 4 NOT implemented: evidence and design in `MULTI_WINDOW_PROPOSAL.md`
  (options A/B/C, B recommended). Ctrl+N stays unbound; a "new window" hand-off
  opens a tab. Owner approval needed before any refactor.
- Docs: README, INSTALL_WINDOWS, CHANGELOG, SECURITY (threat model), VALIDATION,
  OWNER_CHECKLIST steps 12 to 16; owner bundle Setup now records its profile.
- Gate script: `work/v17/gates.ps1` (adds edit-menu and single-instance suites,
  v0.16 baseline in `work/v17/install/baseline`).

Final state (2026-11-11): draft PR #23, final head `25a94d0`. CI 42962367821 and
security 42962367796 passed on it. The earlier head `ab72a65` failed CI 42961341354
in the DACL test (hosted runner is the built-in Administrator, written as SDDL
alias LA); fixed in the test only. All 33 local gates passed on `25a94d0`
(`work/v17/production-gates.json`). This checkpoint note is a local commit only,
not pushed, to avoid another full CI run; the same status is in the PR comments.
Stop at the draft PR: do not merge, tag or mark ready without the owner.

Decisions recorded for the owner:
- Extra features of the existing `windows` crate enabled (no new crate,
  Cargo.lock unchanged).
- Hand-off reply is one status byte; read as "returns no data".
- "New window" request opens a new tab while multiple windows are not shipped.
- A second launch during the first window's startup is answered "busy" and the
  launcher retries; after 6 s it explains instead.
- Two different copies (portable and installed) on one profile do not hand off to
  each other (executable path must match); the user is told.

## v0.16: everyday browsing and toolbar follow-ups

Active branch `feat/v0.16-everyday-basics`, created from verified origin/main
`cb3d81dc4e5f7fca883fc561ff6216523a21dd2d`. Baseline Windows CI 42931608985
and dependency security 42931608910 passed. PR #21 merged at 6e83426; PRs #20
(1101e0a) and #4 (cb3d81d) had also already merged before this branch. Preserve
them; no further dependency change is authorized. Git uses the GitHub noreply
address. Main, real profiles/installations, Windows settings, tags and releases
are untouched. Static CRT remains opt-in. RC and acceptance criteria are unapproved.

Implemented and pushed once for draft PR #22:
- Monotonic toolbar source IDs, stale-failure rejection, a once-per-window terminal
  alert, and five-second health grace for reported Unresponsive. Real hang detection
  was not observed in 30 seconds; the bounded recovery test injects the report into
  a genuinely hung fixture. Responsive probes do not spend the restart budget.
- Content-only native Find/print and per-controller zoom, live percentage/reset,
  keyboard and More commands. Local panels reject these commands; trusted chrome
  disables native browser accelerators, context menus and zoom. Ctrl+Shift+P and
  AltGr remain. Package 0.16.0, no persistence format or dependency version change.
- Reproduced print-from-More focus failure by GUI and native assertion. Transferring
  focus to content fixes the native assertion. A 0.16 GUI recheck passed More and
  Ctrl+P preview/Escape, keyboard zoom, More reset and disposable-session restoration.
- Native content-tools suite passes; the same workload passes in private mode.
  Added normal/private suites to CI and release validation, each in its own process.
  Eight compiled mutants were rejected; raw pointer `work/v16/mutations.json`.
- Recorded the owner's eight prior passes exactly for bundle o-d8ee96f5, source
  062c93dc7688ef296345f20ac7b1e39fd7c06fae, static EXE bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58.
  These do not transfer to modified v0.16 bytes or approve an RC.
- #20 review recommends retaining its already merged immutable pins. New artifact
  direct-upload mode is not enabled. #4's deferred status in older notes is historical.
  Whole-PR classification means this code PR cannot verify the docs-only hosted skip.

Initial qualified head ed90da2a06711a9adeebb40c36350e28e69c33bd passed all 31
local gates: fmt, strict Clippy, 81 regular tests, debug/release, 12 separate native
suites, three daily runs, fixtures/Python, imports, portable/installer/metadata,
owner and audit. Audit: 368 dependencies / 1,446 advisories, same exceptions.
First artifact SHA-256 1036cb448c83ca7144966f0faba02d9698a2cfa741e7dcba6f8d09b34ea94597.
Raw pointer `work/v16/production-gates.json`, first-qualified.json and gates-first logs.
The full runner uses `work/v16/dynamic`; earlier focused work uses `work/v16/target`.
Both are isolated; do not overwrite target/release or use the real profile.

Draft https://github.com/Niithursan/RustBrowser/pull/22 is open. Initial head CI
42941163150 was running and security 42941163324 passed when this final evidence
update was prepared. These are initial-head results, not a final CI claim.
Final qualification (Claude Code, 2026-11-10): repeating the gates after d6df29f
failed once in native_toolbar_keyboard_reachability_and_scaled_layout ("Control list
unreadable"): a late focus-probe response was parsed as the next query. afa8fb4
fixes the fixtures only (one outstanding query, ordered responses, fixed 120 ms
focus-response delay; ticketed responses in the page-tools fixture). After it:
keyboard 5/5, normal page tools 3/3, private page tools 3/3, then all 31 local gates
passed on afa8fb4508098ca9ee74de716baaa13ed22286eb (81 regular tests, 12 native
suites, three daily runs, package/installer/metadata/owner/audit). Release EXE
SHA-256 3db516901dd655c0621bd4e53f099ece6d2744072df83fc1aa9bae3bdd1a6941. The
following commit changes only this unpackaged file, which no build or test reads.
Final hosted results are recorded in PR #22, not here.
No merge/tag/publication or RC declaration is authorized.
See validation/V016.md for baseline, reproduction, API research and evidence limits.

Focused local commits before full qualification: 71038eb (toolbar recovery),
8f17e0a (content tools/native regressions), c0504a5 (separate CI processes),
4137f8f (version). All use 129026028+Niithursan@users.noreply.github.com.
An additional private-workload mutation also rejected disabled zoom, making nine
compiled mutation runs across the three workloads. Sources were restored.

## Historical v0.15 checkpoint

## Post-merge v0.15: static CRT and owner acceptance investigation

Owner direction after PR #19: continue qualification without declaring or publishing
an RC. Verified merge `13ff603b84ba2b0c0429dc61bc338de738e9025b`; main CI
42898120216 and security 42898120222 passed. Created
`feat/rc-static-crt-validation` from that verified main. Read AGENTS.md and this file.
Scratch: `<workspace>/work/rc15`. Do not touch the real profile/install, change
system features, merge, tag or publish. Windows 11 x64 is the initial priority;
Windows 10 is untested. Acceptance criteria remain proposed, publication separate.

- `1c642ec`: opt-in static build/import inspection, safe owner bundle and production
  CI coverage. `da054bf`: separate technical acceptance from publication authorization.
- Pinned shipping build source `da054bf2b390b4df6dfd609b543a3ce6ff218962`, hash
  `bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58`.
  No Rust/Cargo/toolchain change from main, no default static linking or new dependency.
  Import inspection: no direct/delay external CRT, 16 system imports, 4,443,648 bytes.
  Initial feasibility artifact hash differs because PE timestamp/CodeView GUID differ;
  pinned configuration was tested again, not assumed reproducible.
- Pinned static format/strict Clippy/79 regular tests/builds/10 separate native suites/
  three daily runs/package/installer/metadata passed. Owner bundle GUI and refusal
  tests passed. Setup's VC Runtime check remains; missing-runtime host not tested.
- Owner acceptance bundle pointer: `work/rc15/owner-bundle.json`. Prepared, not installed
  for the owner. Follow its OWNER_BUILD.md and Launch.ps1; shortcuts alone do not
  isolate profiles. Human checklist remains open.
- PR #4 reviewed separately: scalar API remains, SIMD default feature adds separate
  unsafe engines, Windows graph currently excludes 0.23.1. Offline scalar/SIMD parity
  probe passed 4097 lengths on AVX2; no fallback/ARM audit claim. Defer change while
  qualifying. No dependency file or PR changed; closed #2/#3 left alone.
- Fresh survey completed: 41 visits, zero engine/renderer exits, dumps or timeouts;
  three intentional DNS/TLS failures. Nine-site/20-filler burst also clean. HTTP 429
  from OpenStreetMap remains a site response; one contained download start observed.
- First soak stopped after 667.684 seconds / 62 cycles when the owned process and
  exec session disappeared; cause unknown, no final shutdown result. Excluded from
  a pass. Raw partial run retained in `work/rc15/soak` and `soak-interrupted.json`.
- Fresh full soak passed in `work/rc15/soak2`, 17:22 to 17:53 Toronto on 2026-11-10.
  Pointer `work/rc15/fresh-soak-status.json`: 1800 s configured, 1798.489 s sample
  span, 1792 samples/24 incomplete, 178 cycles, zero navigation errors, final switch
  responses 20.645 to 28.607 ms, clean root/observed-child exit. Journal only
  starting/started/clean_shutdown; running marker absent. Sanitized numerical
  evidence in `benchmarks/rc15-static-soak.json`. Suspension 15 s/history off;
  finite run, not a dynamic comparison, leak proof or human/media/device test.
- `85d5e65` covers failed-build flag restoration and exact installed identity;
  `f10f2d5` moves protected-path negative tests into synthetic LOCALAPPDATA so even
  a regression stays in owned scratch. Focused owner tests passed after this change.
- `84c81ad` records the completed soak; `eec7b45` corrects the inherited Python
  count attribution: 13 scope + 3 TLS + 2 icon = 18 production tests.
- All 29 production local gates passed, including 79 regular tests, ten native
  suites, three daily runs, scripts/Python, builds, imports, packages/installer,
  metadata, new helper tests and audit (369 crates/1,446 advisories). The owner test
  initially queried Process.Path before loader initialization; controlled suspended
  startup reproduced null then correct path. `9dcff76` adds a bounded readiness
  wait and cold-start/wrong-path/exit regression. Fixed static/dynamic tests passed.
  Prior 27 gates retained on unchanged application code; owner/audit follow-up passed.
- Next: verify final packaging and pinned-byte identity, refresh the safe owner
  bundle, push once and open one review PR. Verify final-head checks with gh pr checks.
  Do not count the interrupted run as passed. No PR is open for this branch yet.
- RC status stays unapproved. Final hosted results belong in the PR/final report to
  avoid a packaged-document push after CI. No merge/tag/publication is authorized.

## v0.15: `feat/v0.15-release-candidate-prep`

Historical working notes from before PR #19 delivery. That PR is now merged at
`13ff603`; the active post-merge investigation is recorded above.

Branch from verified `origin/main` `b4f0c23` (PR #18; CI 42857529540 and security
42857529076 passed). Scratch: `<workspace>/work/v15`. No PR is open yet: pushing the
branch costs no Actions minutes (workflows run only for pull requests and main), so
open the draft PR once, on the final head.

Done (committed and pushed to the branch):
- `37abe46` docs-only CI skip (`.github/scripts/ci_scope.py`, tests in
  `validation/test_ci_scope.py`, 2 mutants caught). `adaf6ba` grouped monthly
  Dependabot with `rebase-strategy: disabled`.
- `94f4d69` fixed a parallel temp-folder collision in `tests/diagnostics.rs`.
- `cf12a1d` toolbar engine recovery. Reproduced first: ending the toolbar's own
  browser process left the toolbar dead (0x8007139F) and ending its renderer left it
  silent; pages kept working; nothing noticed. Now rebuilt (about 350 ms and 110 ms),
  notice shown, regression `native_toolbar_engine_failure_is_recovered` (4/4 runs,
  5 mutants caught), added to ci.yml and release.yml.

Findings: PR #18 head `36c8d2f` did get CI run 42857550749 (passed), created 10 s
after the merge at 16:44:00Z and about 7 minutes after the push. Windows Sandbox is
not enabled (no WindowsSandbox.exe); the session is not elevated.

Further committed work, verified against the handoff evidence:
- `224f58b` public-site survey/download containment: 41 addresses, no engine/renderer
  exits, crash dumps or timeouts; two nine-site/20-filler burst runs passed. Three
  intentional DNS/TLS failures diagnosed; OpenStreetMap HTTP 429 remains unexplained.
- `54b19e0` moved v0.10 to v0.13 checkpoints into the archive without losing text.
- `ca1e9e2` and `bb86517` installer interruption fixes and manual sweep. Two final
  32-case fresh/v0.14-upgrade sweeps repaired all cases with no leftovers and preserved
  synthetic profile bytes/semantics. Completed-process controls are included; 24 and
  26 cases respectively recorded killed owned processes. Real-profile read-only
  metadata fingerprint unchanged. Reserved Setup filenames are documented.
- `4d4900c` version 0.15.0; `dc8ec72` pinned v0.14 upgrade fixture/cache.
- `0e57adb` native keyboard/layout regression: 3 passing runs, 4 caught compiled
  product mutants. DPR/CSS renderer emulation is not physical Windows DPI. Optional
  foreground-guarded UIA walks passed 3 times (24/33 controls, 29 distinct reached).
- `7e68a66` reviewed and committed CI minute-economy docs, proposed release criteria
  and owner checklist. Checklist now requires a disposable profile and validation
  installer; it does not imply ordinary browsing leaves data unchanged. Owner scope,
  static CRT, dependency-PR order and acceptance checks remain decisions, not approvals.

Current continuation:
- Read AGENTS.md, this file, readiness/CI/proposal/checklist and the supplied handoff.
  Verified clean base ancestry, PR #18 merge at `b4f0c23`, baseline CI/security and
  absence of a v0.15 PR. Preserved the three pending documents before further edits.
- README/CHANGELOG/VALIDATION, readiness, compatibility, accessibility and installation
  docs now describe v0.15, provenance and limits. No new production code/dependency,
  signing, updater, CRT mode, Windows feature, hand-off or profile-format change.
- `25bfda8` records v0.15 evidence/limits; `5c8822e` formats the inherited keyboard
  test after the first fmt check failed. All 26 final local gates passed on that
  source: 79 regular tests, 10 separate native suites, 3 daily runs, both builds,
  PowerShell fixtures, 2 experimental and 18 production Python tests, packaging,
  v0.14 installer lifecycle, metadata and audit (369 crates / 1,446 advisories).
  A live release EXE lock was avoided with isolated build output; a 273-character
  Inno scratch path was shortened. No user browser was stopped or assertion weakened.
  Raw result pointer: `work/v15/current-final-gates.json` (outside Git).
- Final documentation also requires owner launches through an isolated-profile
  command: validation installer identity alone does not isolate the browser profile.
- Next: recheck packaging after the results-only documentation commit, push once
  and open exactly one draft PR. The whole branch includes code/packaged docs, so
  Windows CI must run.
  Verify exact final head with `gh pr checks`; no rerun/dispatch unless a hosted
  failure is reproduced and fixed locally. Mark ready only after required validation.
- Hosted final run IDs/results will be recorded in the PR and final report, avoiding
  a post-CI packaged-document push that spends another full Windows run. No remote
  v0.15 result is claimed in this pre-PR checkpoint. Do not merge, tag, publish or deploy.

## v0.14 — `feat/v0.14-daily-driver-reliability` (Claude Code, 2026-11-09)

Branch created from verified `origin/main` `ca7ac14` (PR #17 squash merge, 0.13.0;
main CI 42730890112 and security 42730890455 passed). Scratch: `<workspace>/work/v14`.

Done and committed (all local gates passed at each commit):
- `c03eeab` engine reliability. Found with the new manual survey probe
  (`validation/site-survey.ps1`, `src/survey_validation.rs`):
  - Closing a tab or the window destroyed a visible WebView2 view at once. With the
    Microsoft sign-in page this crashes the WebView2 **browser** process (0xC0000005):
    6/8 tab closes, 8/8 window exits (crash dump in the profile). No app hook causes
    it (bisected). Fix: hide now, destroy after 1 s (`RETIRE_DELAY`); shutdown does
    the same. After: 0/12 closes, 0/8 exits. 300 ms was the shortest safe delay.
  - Engine-exit recovery (`Session::recover_engine`), `ProcessFailed` kinds classified.
  - HTTP 4xx/5xx reported as the site's answer; engine follow-up navigation after a
    connection failure no longer overwrites the reason with status 9.
- `130579c` downloads/blocked app links no longer mark the page failed.
- `486fb1a` Ctrl+Shift+T, Ctrl+1..9, Ctrl+PageUp/PageDown, Ctrl+F4, Alt+Home;
  "already running" startup message.
- `e3180e3` version 0.14.0.
- New native regression: `native_failure_reporting_tab_retirement_and_engine_restart`
  (mutation-checked: 7 mutants all caught). 79 regular tests.

Also done:
- `7c1ebb2` installer upgrade fixture moved to pinned v0.13 `ca7ac14`
  (`tests/build_install_baseline.ps1`, cache key `installer-fixture-v3-…`); new native
  step in `ci.yml` and `release.yml`; 41 packaged-doc links made absolute and checked
  by `tests/windows_package.ps1`.
- `260ab24` accessibility announcement assertions; benchmark/soak results
  (`benchmarks/V014_RESULTS.md`, `v014-screening.json`, `v014-soak.json`).
- `0a204aa` VALIDATION.md, `validation/RELEASE_READINESS.md`.
- Campaign complete: 32 matched runs (no difference established), 30-minute soak
  passed (1,799 s, 178 cycles, 0 navigation errors, clean exit). Raw data in
  `work/v14/{screening,soak}`; summarizers `work/v14/summarize-{screening,soak}.py`.
- Final local gates all passed (`work/v14/gates-final.log`): fmt, Clippy, 79 regular
  tests, 8 native suites, 3 daily runs, builds, audit, scripts/Python, portable
  package, installer lifecycle 0.13 → 0.14, metadata. Release GUI check passed
  (`work/v14/gui-check.ps1`).
- Draft [PR #18](https://github.com/Niithursan/RustBrowser/pull/18) is open against `main`.

Findings worth knowing before changing this code:
- A download request arrives in a later event-loop turn than the "aborted"
  completion of the navigation it replaced, so failures are recorded to diagnostics
  after `FAILURE_RECORD_DELAY` (2 s) and a download claims only the newest one.
- WebView2 follows a DNS/connection failure with a second navigation to the same
  address (new ID, no ContentLoading, ends status 9). Certificate and HTTP errors
  do not get one.
- `$PSScriptRoot` is empty inside a `param()` default when a script is run with
  `powershell -File`; resolve it in the body.

Not done / not possible here: Windows 10, clean VM, standard-user, missing-runtime
and interactive-wizard tests (no VM); screen reader, contrast theme, DPI, IME and
multi-monitor checks; toolbar engine-process recovery; single-instance hand-off
(needs approval for a new local interface).

Burst-close follow-up (Claude Code, 2026-11-10):
- New survey option `-BurstCloseFillers n` closes the visited tab and `n` blank tabs in
  one event-loop turn. It showed the retirement fix was incomplete: `MAX_RETIRED` was
  16, so a 17th close within 1 s destroyed the oldest hidden view at once. Sign-in
  page crashed in 10/12 bursts over the cap, 0/8 at or under it; example.com 0/3;
  window exit with 19 tabs 0/2. Fix: `MAX_RETIRED = MAX_TABS`; 0/13 bursts after.
- Native regression now closes 62 tabs in one turn; mutation (cap 16) caught.
- Clearing data retires every tab at once, so it also hit the old cap with >16 tabs
  (inferred from the code, not measured). Ctrl+W key repeat is inferred too; the
  measured case is the probe's one-turn burst. Commit `f2fa2fb`'s message says
  "10 of 11"; the correct count is 10 of 12 (history not rewritten).
- Gates passed: fmt, Clippy, 79 tests, debug and release builds
  (`work/v14/gates-burst.log`). Raw runs: `work/v14/burst-rate/`.

Next: re-verify CI on whatever head is current with `gh pr checks 18`. Await the
user's review. Do not merge, tag or publish without explicit approval.

## Earlier milestones

v0.10 to v0.13 checkpoints are in [DEVELOPMENT_CHECKPOINT_ARCHIVE.md](DEVELOPMENT_CHECKPOINT_ARCHIVE.md).
