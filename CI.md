# Continuous integration

## Workflows

| Workflow | Triggers | Jobs |
| --- | --- | --- |
| `CI` (`ci.yml`) | pull requests, pushes to `main`, manual dispatch | **Windows quality and build** (fmt, strict Clippy, all regular tests, launcher/sampler/Python fixture tests, every native WebView2 suite in its own process, three independent daily-driver runs, debug build); **Windows release build and package** (release build, portable package checks, artifact upload) in parallel; **CI result**, which fails unless both jobs succeed |
| `Dependency security` (`security.yml`) | pull requests, pushes to `main`, weekly schedule, manual dispatch | `cargo audit --deny warnings` with pinned cargo-audit 0.22.2 |
| `Windows release` (`release.yml`) | version tags, manual dispatch | Clean, uncached build and packaging. Publication requires explicit approval and is unchanged. |

Feature-branch pushes no longer start a second full CI run next to the pull-request
run. Open a draft pull request to get CI for a branch. Superseded pull-request runs
are cancelled; every `main` commit is validated to completion. No branch protection
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
- Release publication (`release.yml`) is intentionally uncached.

## Measurements (2026-11-09, GitHub-hosted `windows-latest` / `ubuntu-latest`)

Wall time is attempt start to last job completion, including queueing. Runner time
is the sum of job durations (before per-job minute rounding). Baseline: the 8 most
recent successful runs of each old workflow (mixed push and pull-request events).
New: pull request #16. Cold is the first run with no cache. Warm is the next run plus
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
- The weekly scheduled audit may find its cache evicted (seven days without use)
  and rebuild cargo-audit. That is slower but still correct.
