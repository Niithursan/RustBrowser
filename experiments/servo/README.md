# Opt-in Servo spike

This uses the **official servoshell executable**, not an in-process Rust Browser
Servo renderer. WebView2 remains the application's only working/default engine.
Read [research](RESEARCH.md), [architecture](ARCHITECTURE.md) and [results](RESULTS.md).
The pinned upstream Servo v0.7.0 has no supported Windows OS sandbox. Do not use this
prototype for accounts, real user files, device grants or everyday browsing.

## Prepare and run the pinned package

Windows x64; PowerShell 5.1 or later. From the repository root:

```powershell
$scratch = Join-Path $env:TEMP ('rust-browser-servo-' + [guid]::NewGuid().ToString('N'))
$package = .\experiments\servo\prepare.ps1 -OutputRoot $scratch
$run = .\experiments\servo\run.ps1 -ServoExecutable $package.executable -OutputRoot $scratch -Page Local
```

Preparation downloads a 103 MB archive from the fixed official GitHub release URL,
checks its SHA-256 from `pin.json`, extracts into a unique new directory and checks
the executable digest. To reuse an **archive**, add `-Archive C:\path\servo.zip` to
preparation; it is always verified again. No installer or system setting is changed.
Do not relocate the EXE away from its bundled DLLs/resources. The digest does not
protect against later local modification of those DLLs/resources; use a fresh verified
extraction if in doubt. No downloaded binary or archive belongs in Git.

Each launch allocates a separate profile, passes `--temporary-storage`, removes an
inherited Rust Browser profile override from the child and enables no debugging
server, certificate override or bridge. It accepts only these initial page modes:

- `Local`: the self-contained local fixture (no network needed).
- `Loopback`: `http://127.0.0.1:8770/`, or the explicit `-Port` value below.
- `Https`: the fixed `https://example.com/` page, with normal certificate validation.

The native browser window is visible even though its console is hidden. Close it
normally; add `-Wait` to make the script report a nonzero process exit. The script
does not delete retained run data or promise private-mode cleanup. It never reuses a
run profile. Review/discard only the owned scratch directory after all its processes
have exited. A fresh directory is **data separation**, not an OS sandbox.

## Controlled input and resource comparison

For the common HTTP fixture, start a loopback-only server in a separate terminal:

```powershell
python .\experiments\servo\fixture_server.py --port 8770
```

Then launch with `-Page Loopback`. The server serves only `/`, rejects other paths
and POSTs, has no host IPC/file API, records no requests, and sends a CSP denying
network callbacks/frames/external resources. Click Count, type synthetic text,
scroll, resize and check the animation and visible status. On loopback the synthetic
local-storage round trip should succeed; a file origin can report storage unavailable.
Do not act on device/security prompts. HTTPS is a separate minimal navigation check,
not a compatibility benchmark or TLS-failure certification.

Use the existing read-only process-tree collector for numeric evidence:

```powershell
.\validation\sample-process.ps1 -RootProcessId $run.process_id -OutputRoot (Join-Path $scratch 'measurements') -Seconds 30
```

For WebView2, build the application in release mode and seed a newly allocated
disposable profile's `session.json` with `{"version":1,"tabs":["http://127.0.0.1:8770/"],"active":0}`
using UTF-8 without a BOM. Set `RUST_BROWSER_PROFILE` only around `Start-Process` to
that profile and launch **without URL arguments**. A URL argument adds a second tab
beside Home in an unseeded profile, which is not the same workload. Restore the parent
environment immediately afterwards. Both shells start at 1200×820 logical client
size, though their toolbar/content allocations differ. Observe page completion,
leave it idle for another twenty seconds, and run the same collector. Close one shell and wait
for its descendants before starting the next measurement. Record executable hashes,
source revisions, hardware/Runtime, window size, foreground state, settling/sample
times and other desktop activity. Do not build during resource sampling.

The shells have different UI and viewport overhead. CPU/memory observations are not
an engine-only comparison. Startup or input latency needs equivalent instrumented
endpoints; report it unavailable until those are implemented. Do not compare an
upstream whole-process duration with an application-internal startup proxy.

## Tests and production CI

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
.\tests\servo_tools.ps1 -TestRoot (Join-Path $env:TEMP 'rust-browser-servo-tests')
python -m unittest discover -s experiments/servo -p 'test_*.py'
```

The ordinary Windows job preserves all six native v0.6 tests and debug/release builds.
New tooling/fixture tests execute no downloaded engine. Default Cargo.lock has no
Servo dependencies. Actual servoshell rendering/input/shutdown is validated separately;
a full Servo source build is not silently skipped or treated as passed in application CI.

## Separate source-build procedure (not performed locally)

Provision the official [Windows prerequisites](https://book.servo.org/building/windows.html)
in a dedicated development environment: x64 SDK/MSVC + ATL, uv/rustup and Servo's
native tools. The pinned source specifies Rust 1.97.1 and additional components;
do not alter Rust Browser's pinned 1.99.1 toolchain or its dependency graph.

```powershell
$source = Join-Path $scratch 'source'
git init $source
git -C $source remote add origin https://github.com/servo/servo.git
git -C $source fetch --depth 1 origin aac43a3f31a259f04a574f5ec4e959c943ad7cc7
git -C $source checkout --detach FETCH_HEAD
Set-Location $source
.\mach bootstrap
.\mach build --locked --profile release
```

Check each command's exit status. Bootstrap can install native tools; this procedure
is for the dedicated environment, not an automatic action of the browser/launcher.
Use the matching `ports/servoshell` and embedding example sources; capture build
timings/resource use before choosing a source-build CI budget. Preserve compiler
errors as evidence. Once a build succeeds, use its own help/resources and a new
controlled profile; the package launcher intentionally refuses a different EXE hash.
Do not weaken the pinned-package checks to accept unverified builds automatically.
