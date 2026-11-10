# Continuous integration

v0.15 adds Actions cost controls without removing required coverage. A small Linux job,
**Change scope**, runs first and skips the two Windows jobs only when every file the
pull request changes is Markdown that no build, test or package reads. Dependabot
updates are grouped and checked monthly. v0.15 also adds two native steps
(toolbar engine recovery and keyboard/layout checks, about 5 and 14 seconds in
recorded local runs) and moves the
installer upgrade fixture to the pinned v0.14 commit `b4f0c23` (new immutable cache
key `installer-fixture-v4-...`, target `target-v014`). **The first CI run after this
change builds the fixture cold** (the previous fixture's cold build measured 154.5 s; see below). Details
under "v0.15: minute economy" below.

The public-site survey, the interrupted-installation test, the matched benchmark
campaign and the 30-minute soak are manual and are not part of any workflow: they
depend on the network, run Setup dozens of times or take about an hour.

## Workflows

| Workflow | Triggers | Jobs |
| --- | --- | --- |
| `CI` (`ci.yml`) | pull requests, pushes to `main`, manual dispatch | **Change scope** (Linux; decides whether the Windows jobs run); **Windows quality and build** (fmt, strict Clippy, all regular tests, launcher/sampler/Python fixture tests, every native WebView2 suite, three daily-driver runs, debug build); parallel **Windows release build and package** (release, portable/installer/profile/metadata checks, review artifacts); **CI result** requires the scope job, and both Windows jobs unless the change is documentation only |
| `Dependency security` (`security.yml`) | pull requests, pushes to `main`, weekly schedule, manual dispatch | `cargo audit --deny warnings` with pinned cargo-audit 0.22.2 |
| `Windows release` (`release.yml`) | version tags, manual dispatch | Clean/uncached application build, all script/native/audit gates, validated ZIP/setup/metadata. Only pinned audit tool cached. Existing-tag/manual publication approval and permissions unchanged. |

Feature-branch pushes no longer start a second full CI run next to the pull-request
run. Open a draft pull request to get CI for a branch. Superseded pull-request runs
are cancelled. Runs for `main` pushes, manual dispatches and the scheduled audit use
per-commit concurrency groups, because GitHub drops an already pending run when a
newer run queues in the same group. Each `main` commit therefore gets its own run. No branch protection
or required-check configuration exists (the private repository plan does not offer
it), so no required check names changed. `CI result` provides one stable name if
protection is enabled later.

## Caching

- **Rust dependencies:** `Swatinem/rust-cache` v2.9.3, pinned to commit
  `d1527c8f67ef9af357290475d4af4b0fab8f12e0`, after the toolchain install. It caches
  the Cargo registry and compiled **dependencies only**; workspace crates always
  rebuild. Its key covers runner OS/architecture, the rustc version, `Cargo.lock`,
  Cargo/Rust environment variables and the job, so a toolchain or lockfile change
  starts fresh. Each job has its own cache (v0.12: about 250 MiB quality, 205 MiB release).
- **cargo-audit:** `actions/cache` v6.1.0, pinned to commit
  `55cc8345863c7cc4c66a329aec7e433d2d1c52a9`, caches only the `cargo-audit` binary
  built from crates.io source on the runner. The key includes the pinned version, OS,
  architecture and toolchain. The workflow verifies the version before auditing.
  The advisory database is never cached and is fetched on every run.
- Pull-request runs save caches in their own pull-request scope, which `main` and other
  pull requests cannot read. After merge, `main` saves caches that new pull requests restore.
- The tag/manual publication workflow (`release.yml`) builds application
  source and dependencies **uncached**; only its pinned, version-checked cargo-audit
  0.22.2 executable is cached, and advisories are fetched fresh. The CI review-package
  job (`ci.yml`) does use the dependency cache; its artifacts are for review only.

## v0.13 installer coverage and measured overhead

Existing production dependency caches, parallel quality/release jobs, no duplicate
feature pushes, strict lint/native/audit gates and stable aggregate are retained.
The installer job builds pinned v0.12 source as an upgrade fixture, then tests
fresh/upgrade/removal/reinstall and data preservation under a GUID test identity.
That fixture has its own target/cache keyed by immutable commit, runner OS/arch
and Rust version. No downloaded historical binary is trusted as the fixture.

Initial PR #17 run [42724868424](https://github.com/Niithursan/RustBrowser/actions/runs/42724868424)
at `694f4de` passed. Quality: **220 s**; release/package: **462 s**; aggregate:
**2 s** (684 s summed runner time, about 466 s job wall span). The first immutable
baseline build took **123.519 s**; installer/fixture tests including compiler setup
and integration-test compilation took **225 s** total. Audit
[42724868239](https://github.com/Niithursan/RustBrowser/actions/runs/42724868239)
passed in **25 s**, restoring its pinned tool.

This exposed cache duplication: the old fixture target was nested in production
`target`, so the ~119 MB fixture cache was also included in the ~503 MB production
cache. Production post-cache cleanup/save took **106 s**. The fixture is now outside
the production target in runner temp, with an independent cache; a new production
namespace avoids restoring that contaminated tree. No checks were removed. A local
first clean fixture target took **37.834 s**, on different hardware and not comparable
to hosted time. The attempted shared-target build failed by reusing the wrong
workspace resource script; separate targets also fix that correctness issue.

### Relocated cache: cold and warm (PR #17 at `cb7a4b1`)

Run [42726540504](https://github.com/Niithursan/RustBrowser/actions/runs/42726540504):
attempt 1 was cold (new production namespace and new fixture key); attempts 2 and 3 are
reruns of the same commit. Security run [42726540444](https://github.com/Niithursan/RustBrowser/actions/runs/42726540444).

| Measure | v0.12 warm (PR #16) | v0.13 cold | v0.13 warm (2 attempts) |
| --- | ---: | ---: | ---: |
| CI wall time | 144–202 s | 566 s | 177, 184 s |
| CI runner time | 218–298 s | 737 s | 286, 320 s |
| Release job | 69–103 s | 558 s | 114, 174 s |
| Release compile | 23–45 s | 168 s | 21, 40 s |
| v0.12 upgrade-fixture build | — | 154.5 s (miss) | 0.35, 0.38 s (hit) |
| Installer lifecycle step (fixture included) | — | 287 s | 33, 51 s |
| Release cache restore / save | 40–54 s (both jobs) | 9 / 45 s | 25–36 / 0 s |
| Fixture cache save | — | 7 s | 0 s |
| Review artifact upload | <7 s | <2 s | 2 s |
| Security audit (wall) | 26–28 s | 27 s | 26, 31 s |

- v0.13 warm CI is **not faster** than v0.12 warm. It runs a full installer lifecycle
  every time, adding about 30–50 s of warm runner time. Wall time stays similar
  because that work sits in the parallel release job.
- The correction's measured effects: the fixture no longer rebuilds on warm runs
  (154.5 s to 0.35 s); the cold production cache save fell from 106 s to 45 s; the
  fixture is cached once (113 MiB) instead of inside the production cache.
- The production release cache is 330 MiB (v0.12: 206 MiB). The difference is not the
  fixture: the installer harness runs `cargo test --test installation_profile` in the
  release job, so debug dependencies are compiled and cached there. That is deliberate,
  because recompiling them every run would cost more than restoring them.
- On a fixture cache hit, cargo does not even relink (0.35 s): the freshly extracted
  pinned sources keep the commit's timestamps. This is not stale reuse, because the
  key includes the immutable fixture commit and the toolchain, so different fixture
  source cannot match it. The script still checks the fixture reports version 0.12.0.
- A stale 480 MiB cache from the pre-correction run (`694f4de`) stays in the PR #17
  scope until GitHub evicts it; it is never restored by the new namespace.
- Lower wall time does not mean lower billing: Windows runner minutes are summed per
  job and rounded up per job.

These runs add installation coverage and are not an application-performance
benchmark or an overall CI speedup. Release-workflow audit-cache savings are unmeasured: no tag,
manual release dispatch or publication was invoked to obtain timing numbers.

Review upload contains exactly the portable ZIP, setup EXE, their two checksums
and unsigned release JSON. It excludes validation installers, test profiles, keys,
logs and compiler payloads. The publication job re-verifies that exact set, hashes,
metadata and source commit. Download transport/checksums do not establish independent
publisher authenticity; see [update security](packaging/windows/UPDATE_SECURITY.md).

## Measurements (2026-11-09, GitHub-hosted `windows-latest` / `ubuntu-latest`)

Wall time is attempt start to last job completion, including queueing. Runner time
is the sum of job durations (before per-job minute rounding). Baseline: the 8 most
recent successful runs of each old workflow (mixed push and pull-request events).
New: pull request #16, measured on commit `95ea8d6` (CI changes only, before the
v0.12 UI, accessibility and compatibility phases were added). Cold is the first run with no cache. Warm is the next run plus
two reruns of the same commit. Hosted-runner variance is large (the old release build
alone ranged 99–205 s), so treat these as small samples, not precise speedups.

| Measure | Old (median, n=8) | New cold (n=1) | New warm (3 attempts) |
| --- | ---: | ---: | ---: |
| CI wall time | 458 s | 344 s | 144, 202, 202 s |
| CI runner time per run | ~454 s | 588 s | 218, 298, 264 s |
| Toolchain and checkout setup | ~20 s | 47 s (two jobs) | 41–53 s (two jobs) |
| Cache restore / save | none | 15 s / 82 s | 40–54 s / ~1 s |
| Clippy | 80 s | 87 s | 11–16 s |
| `cargo test` (compile and run) | 86 s | 88 s | 20–30 s |
| Native, daily and fixture tests | ~77 s | 76 s | 63–88 s |
| Release build | 164 s | 173 s | 23–45 s |
| Packaging and upload | 3.5 s | 6 s | 5–7 s |
| Security wall time | 342 s | 306 s* | 28, 26 s |
| cargo-audit install | 314 s | 283 s | 0 s (cache hit) |
| Audit itself | 5 s | 5 s | 4–6 s |

\*The first cold security run on the previous commit failed only at the new
version check (`cargo audit --version` prints `cargo-audit-audit 0.22.2`). The
check was corrected before the measured run.

Observations:

- Warm runs cut CI wall time from a median of 458 s to 144–202 s, and dependency
  security from 342 s to under 30 s. Native WebView2 test time is unchanged:
  coverage was not reduced.
- The **cold** run used **more** runner time than before (588 s vs ~454 s): the
  second Windows job has its own setup, and the first cache save took 82 s. Its
  wall time was still lower, because the release build runs in parallel.
- Before, each push to a branch with an open pull request ran CI twice (push and
  pull-request events), about 908 s of Windows runner time. A warm push now uses
  about 220–300 s.
- Windows runner minutes are billed at a higher multiplier than Linux on private
  repositories. Check the account's Actions billing before adding jobs.
- The final v0.12 head (`02e4221`, with the new native phases and the 0.12.0
  version bump) restored both dependency caches with a full key match. `rust-cache`
  excludes the workspace's own package version from its lockfile hash, so version
  bumps do not invalidate it. That run took 197 s wall time and 280 s runner time;
  dependency security took 24 s.
- The weekly scheduled audit may find its cache evicted (seven days without use)
  and rebuild cargo-audit. That is slower but still correct.

## v0.14 measurements (2026-11-09/10, GitHub-hosted runners)

Pull request #18. Wall time is run creation to last update; job times are start to
completion. One sample each; hosted-runner variance is large.

| Run | Head | Caches | CI wall | Quality job | Release job | Security |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 42758473146 / 42758473338 | `0a204aa` | Cold: `Cargo.lock` changed (version) and the upgrade fixture has a new key, so the pinned v0.13 build was compiled | 334 s | 229 s | 326 s | 25 s |

## v0.15: minute economy

### Where the minutes went (measured, 2026-11-07 to 2026-11-10)

The billing API (`/actions/runs/{id}/timing`) returns 0 billable milliseconds for
this repository, so these figures are **estimates** from job start and end times:
each job rounded up to whole minutes, Windows jobs counted twice (the 2x rate). The
script and raw table are in the v0.15 scratch folder (`ci_cost.py`,
`ci-cost-since-1106.tsv`).

| Source | Runs | Estimated billed minutes |
| --- | --- | --- |
| Feature branches and pull requests, CI | 90 | 1,092 |
| Dependabot pull requests, CI | 24 | 379 |
| `main` pushes, CI | 15 | 227 |
| Feature branches and pull requests, dependency audit | 49 | 200 |
| Dependabot pull requests, dependency audit | 24 | 110 |
| `main` pushes, dependency audit | 16 | 74 |
| Experimental Windows boundaries (10 runs) and Dependabot update jobs (25) | 35 | 47 |
| **Total** | 253 | **2,130** |

A full pull-request CI run cost an estimated 15 to 21 billed minutes in v0.14 (its
four runs: 21, 15, 15 and 17). Dependabot accounted for about a quarter of the total
(about 525 minutes): three open pull requests, each rerun every time Dependabot
rebased it after `main` changed (after PR #17 and after PR #18 merged), plus the
Windows boundaries workflow for the checkout update.

### Documentation-only changes skip the Windows jobs

`.github/scripts/ci_scope.py` (standard library only) decides. It classifies the
**whole pull request** (merge base to head), not only the latest push, so a
documentation commit on top of code still runs everything, and a head whose earlier
run was cancelled cannot report green without the Windows jobs. For a push to `main`
it classifies the pushed commits.

The Windows jobs are skipped only when every changed path ends in `.md` and is not
one of the five packaged documents (README, CHANGELOG, SECURITY, VALIDATION,
INSTALL_WINDOWS). Renames count as a deletion plus an addition, so moving code into a
Markdown file runs everything. Any other file, a manual dispatch, a missing or
all-zero base commit, an empty diff or any classification error runs everything.

`CI result` still reports on every run. It succeeds only if the scope job succeeded
and either both Windows jobs succeeded (full run) or both were skipped (documentation
only).

Evidence: 18 Python tests in `validation/test_ci_scope.py` (run in the CI quality job)
cover the rules, the verdict, real git diffs in a temporary repository, and that the
packaged list matches `package.ps1`, `installer.iss` and `windows_package.ps1`. Two
mutations (dropping `--no-renames`, ignoring packaged documents) were each caught.
**The skip path has not run on GitHub yet**: this milestone changes code, so its pull
request runs everything. The first documentation-only pull request will be the first
hosted evidence. No saving is claimed until then. Because most milestone
documentation changes touch the packaged README or CHANGELOG, the expected saving is
for checkpoint, CI, validation and planning documents.

### Dependabot

`.github/dependabot.yml` now groups updates into one pull request per ecosystem
(cargo, GitHub Actions), checks monthly instead of weekly, allows one open pull
request per ecosystem and disables automatic rebasing. Comment `@dependabot rebase`
on a Dependabot pull request when you are ready to review it; that starts one run.
Security updates are not affected by the schedule. Expected effect, not yet measured:
scheduled version updates are grouped by ecosystem at the monthly check, with at
most one open version-update pull request per ecosystem. Security updates and
requested rebases can add runs; this is not a guaranteed two-run monthly budget.

Review of the three open Dependabot pull requests (none merged):

| Pull request | Change | Recommendation |
| --- | --- | --- |
| #4 base64 0.22.1 to 0.23.1 | Direct dependency; another crate already pulls in 0.23.1, so the lock file currently carries both | **Merge.** Removes a duplicate crate. CI passed on its current head (run 42858162667). Only `STANDARD.encode` is used. |
| #2 actions/checkout 6.1.0 to 7.0.1 | v7 blocks checking out fork pull requests under `pull_request_target` and `workflow_run`, and moves to ESM | **Merge.** This repository uses neither trigger. CI passed (run 42858985681). `release.yml` also uses checkout and is only exercised on a tag. |
| #3 actions/upload-artifact 4.6.2 to 7.0.2 | v5 to v6 move to Node.js 24; v7 adds an opt-in `archive: false` | **Merge, low risk.** Defaults (zipped artifacts) are unchanged, which `release.yml`'s `gh run download --name` relies on; that publish path only runs on an approved release. Its CI run is older (42731132640). |

Cost note: merging this milestone changes `ci.yml` and `Cargo.lock`, so open
Dependabot pull requests would conflict, and with automatic rebasing still on in the
old configuration each would rebase and rerun. Cheapest order, for the owner to
choose: either merge #2 to #4 before this milestone (each merge then changes `main`
and the remaining ones may rebase), or close them and let the grouped configuration
open at most two new pull requests at its next monthly check.

### Why PR #18's last head looked unchecked

The brief for v0.15 said `36c8d2f` never received a CI run. It did: run
42857550749 (all jobs passed) and audit run 42857550411. What made it look
unchecked is timing:

| Event | Time (UTC, 2026-11-10) |
| --- | --- |
| `6311f88` committed and pushed | about 16:34 |
| CI run for `6311f88` created | 16:38:26 (about 4 minutes later) |
| `36c8d2f` committed and pushed | about 16:37 |
| PR #18 merged | 16:44:00 |
| CI run for `main` `b4f0c23` created | 16:44:03 |
| CI run for `36c8d2f` created | 16:44:10 (about 7 minutes after the push, 10 seconds after the merge) |

So at merge time the pull request page showed no checks for its head. Earlier heads
received their runs within seconds (for example `849711a`). The pull request's
mergeable state read `UNKNOWN` while waiting, which fits GitHub not yet having
computed the test merge commit that `pull_request` runs build, but the cause of the
delay is **not established**. Branch protection with a required `CI result` check
would prevent merging before the head's run finishes; this repository's plan does not
offer it (see above), so check `gh pr checks` on the exact head before merging.
