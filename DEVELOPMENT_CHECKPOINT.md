# Development checkpoint

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
- Next: run remaining production local gates, record results, verify final packaging,
  push once and open one review PR. Do not count the interrupted run as passed.
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
