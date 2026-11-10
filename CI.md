# Continuous integration

## Workflows

| Workflow | Triggers | Jobs |
| --- | --- | --- |
| `CI` (`ci.yml`) | pull requests, pushes to `main`, manual dispatch | **Windows quality and build** (fmt, strict Clippy, all regular tests, launcher/sampler/Python fixture tests, every native WebView2 suite, three daily-driver runs, debug build); parallel **Windows release build and package** (release, portable/installer/profile/metadata checks, review artifacts); **CI result** requires both jobs |
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
  starts fresh. Each job has its own cache (about 250 MiB quality, 205 MiB release).
- **cargo-audit:** `actions/cache` v6.1.0, pinned to commit
  `55cc8345863c7cc4c66a329aec7e433d2d1c52a9`, caches only the `cargo-audit` binary
  built from crates.io source on the runner. The key includes the pinned version, OS,
  architecture and toolchain. The workflow verifies the version before auditing.
  The advisory database is never cached and is fetched on every run.
- Pull-request runs save caches in their own pull-request scope, which `main` and other
  pull requests cannot read. After merge, `main` saves caches that new pull requests restore.
- Release application source/dependencies are intentionally uncached; only its
  pinned/version-checked cargo-audit 0.22.2 executable is cached. Advisories are fresh.

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

Final relocated-cache cold/warm timings are pending verification. These runs add
installation coverage and are not an application-performance benchmark or a proven
overall CI speedup. Release-workflow audit-cache savings are unmeasured: no tag,
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
