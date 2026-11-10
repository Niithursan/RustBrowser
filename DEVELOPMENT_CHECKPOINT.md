# Development checkpoint

## v0.15: `feat/v0.15-release-candidate-prep` (Claude Code, in progress)

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

Next: release criteria, survey, interrupted install, accessibility, owner
checklist, docs, archive of v0.10 to v0.13 checkpoint sections, full gates, draft PR.

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
