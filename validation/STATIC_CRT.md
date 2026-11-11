# Windows static CRT investigation

Status: opt-in investigation after PR #19, not production adoption or RC approval.
Windows 11 x64 is the initial qualification priority. No Windows 10 pass, clean-
machine pass, missing-runtime pass, release tag or publication is claimed.

## Scope and source evidence

Baseline: `13ff603b84ba2b0c0429dc61bc338de738e9025b`, package 0.15.0.
Post-merge CI 42898120216 and security 42898120222 passed before branching.
Investigation branch: `feat/rc-static-crt-validation`. The pinned test build is
`da054bf2b390b4df6dfd609b543a3ce6ff218962`; its Rust sources, Cargo files, build
script and toolchain are unchanged from the baseline. Only validation tooling,
CI coverage and acceptance documentation changed. Production linking stays dynamic.

The [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html)
supports `-C target-feature=+crt-static` for MSVC and recommends inspecting the
result. We pass an explicit `x86_64-pc-windows-msvc` target so host proc macros and
build scripts do not inherit the target's CRT linkage. Bundled SQLite is rebuilt
in that target rather than linking to a separate installed SQLite DLL.

Microsoft documents [CRT library choices](https://learn.microsoft.com/en-us/cpp/c-runtime-library/crt-library-features?view=msvc-170)
and [cross-DLL CRT ownership hazards](https://learn.microsoft.com/en-us/cpp/c-runtime-library/potential-errors-passing-crt-objects-across-dll-boundaries?view=msvc-170).
Do not exchange CRT-owned file streams or allocate on one CRT heap and free on
another. The browser uses WebView2 through COM/windows bindings; the explicit
native download path frees COM-selected paths with `CoTaskMemFree`. No new FFI,
allocator, DLL interface or dependency was introduced. That inspection plus native
tests is not a formal audit of every upstream FFI boundary.

Static runtime security fixes require rebuilding and redistributing the executable.
An OS/shared-runtime update does not replace its embedded CRT. WebView2 remains an
external, separately serviced engine. Static CRT does not remove that requirement
or establish browser security, anonymity or arbitrary-site compatibility.

## Pinned artifacts

| Item | Value |
| --- | --- |
| Candidate source | `da054bf2b390b4df6dfd609b543a3ce6ff218962` |
| Toolchain/target | Rust 1.99.1, x64 MSVC, `-C target-feature=+crt-static` |
| Shipping executable SHA-256 | `bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58` |
| Executable size | 4,443,648 bytes |
| Direct/delay external CRT imports | None reported by MSVC dumpbin |
| Initial feasibility build | 39.44 s Cargo compile time; different target directory and artifact hash |
| Committed helper build | 45.25 s wall time; one development-machine sample, not a performance comparison |

The initial and pinned executables have different PE timestamps and CodeView GUIDs.
Equal sources did not produce equal bytes; reproducible executable builds are not
claimed. Native and packaging gates are run again against the pinned configuration.
`check-crt.ps1` inspects direct and delay imports with MSVC
[`dumpbin /IMPORTS`](https://learn.microsoft.com/en-us/cpp/build/reference/imports-dumpbin?view=msvc-170).
It rejects empty reports and unexpected CRT imports. It cannot prove the absence
of runtime `LoadLibrary` dependencies or inspect every WebView2/system DLL chain.
No installed runtime DLL was removed, replaced or hidden to simulate a clean host.

## Reproduce without changing production linking

Use the pinned Rust toolchain, MSVC Build Tools/Windows SDK and installed WebView2.
The import check also needs Visual Studio's `vswhere` and x64 `dumpbin`. Start from
a clean commit; keep output outside the repository and all browser profiles.

```powershell
$scratch = Join-Path $env:TEMP ('rb-crt-' + [guid]::NewGuid().ToString('N').Substring(0,8))
$candidate = .\validation\build-static-crt.ps1 -OutputRoot $scratch
Get-Content $candidate.manifest
.\validation\check-crt.ps1 -Executable $candidate.executable -Expected static
```

The helper restores the caller's Rust flags, writes a source/flags/hash manifest
and marks qualification/publication false. It does not alter `.cargo` configuration,
production workflows, Cargo.lock, system features or installed runtimes.

For static tests, use a separate PowerShell session and the candidate target:

```powershell
$env:CARGO_BUILD_TARGET = 'x86_64-pc-windows-msvc'
$env:CARGO_TARGET_DIR = Join-Path (Split-Path $candidate.manifest) 'target'
$env:CARGO_ENCODED_RUSTFLAGS = '-C' + [char]31 + 'target-feature=+crt-static'
$env:RUSTFLAGS = $null
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
```

Run each of the ten native commands listed in `ci.yml` in its own process, then
three independent `validation/windows-daily.ps1` runs. Repeat portable packaging,
`tests/windows_installer.ps1` with pinned v0.14, `tests/release_metadata.ps1`, and
`tests/owner_build.ps1 -ExpectedCrt static`. The production CI uses dynamic linking
and separately covers the owner helper with `-ExpectedCrt dynamic`.

The survey uses the same target/flags and `native_public_site_survey`; downloads
stay in its disposable profile and no credentials or permissions are supplied.
The release-only test probe runs the real session with 20 stable fixture tabs and
at most one scratch tab, churn every ten seconds, 1800 seconds soaking, and resource
sampling including descendants. See [benchmark procedure](../benchmarks/README.md).
The fixture explicitly enables automatic suspension after 15 seconds and disables
persistent history. Most background fixtures can therefore sleep; this is not a
20-fully-active-tab load or an extended media/download/session-history test.
Do not run builds or other validation workloads during resource measurement.
This is synthetic churn, not a human interactive session or proof of leak freedom.

## Acceptance bundle and remaining decisions

The fresh controlled soak completed 178 churn cycles with no navigation errors,
normal shutdown and observed child exit. See [validation](../VALIDATION.md) and
[sanitized numbers](../benchmarks/rc15-static-soak.json) for memory/CPU, incomplete
samples and the excluded interrupted run. Both static and default-production local
gates passed. This supports further acceptance work, not an RC declaration.

`prepare-owner-build.ps1` creates only a validation-identity Setup in its owner
bundle, a portable copy, exact hashes, `Launch.ps1` and `OWNER_BUILD.md`. The launch
checks binary hash and installed identity, selects the bundle's disposable profile,
and refuses foreign profiles, reparse paths and production directories. It never
deletes a profile. Do not launch the validation shortcuts directly.

Owner-process tests also cover pre-loader startup: the image path may initially be
unavailable. Verification waits within the same startup timeout and still rejects
wrong paths or exited processes. Test-only suspended creation uses an owned process
and disposable profile, with no privilege or sandbox change.

The installer and portable prerequisite disclosures remain conservative, including
Setup's existing VC Runtime check, even for the static experiment. Removing that
check or adopting static linking needs a separate reviewed change after a clean-
machine/runtime test. Human owner checks, risk dispositions, qualification status
and publication authorization remain open. Results are recorded in VALIDATION.md
and the review PR; this document does not itself approve an RC.
