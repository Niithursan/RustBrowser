# Windows Servo source build

The v0.8 experiment starts from the pinned upstream `winit_minimal` example.
Rust Browser uses WebView2 for normal tabs. The Servo example uses the upstream
Cargo workspace and lockfile in a separate checkout.

## Source and prerequisites

| Item | Pin or local preflight |
| --- | --- |
| Servo | `65d058b9c6543840c1d986f7cae4520605bf5b5c` |
| Rust target | `1.99.1-x86_64-pc-windows-msvc`, rustc `58cf8f9c3` |
| Upstream lockfile | SHA-256 `eb0cc53c1a4044738088cd90825409034014ee65d6f51d6e00448617996e9031` on the Windows checkout |
| Local MSVC | VS 2022 Community 17.14.42, MSVC 14.44.35207, Windows SDK 10.0.22621.0 |
| Local LLVM | Swift Clang 21.1.6 with `clang-cl.exe`, `lld-link.exe`, and `libclang.dll` |
| Full servoshell prerequisites | ATL, uv, standalone CMake/Ninja and GStreamer were not available at the checked locations |

The full upstream Windows workflow uses LLVM 20.1 and `mach` bootstrap tooling.
This attempt selects the library's bundled resources and fonts with the default
Windows WGL surface. It omits the optional GStreamer and ANGLE features and
uses direct Cargo with the pinned MSVC Rust toolchain.
[Servo Windows guide](https://book.servo.org/building/windows.html) ·
[Pinned Windows workflow](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/.github/workflows/windows.yml) ·
[Pinned feature definitions](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/servo/Cargo.toml)

## Reproduce the compile

Use a dedicated Windows x64 development environment. Start from a short, fresh
scratch path outside the RustBrowser repository and its profiles. Run the following
in a dedicated PowerShell session. Install the pinned MSVC Rust toolchain and
provide LLVM with Clang, LLD and libclang before running the commands.
Run the commands from the Rust Browser checkout root.

```powershell
$repo = (Get-Location).Path
$revision = '65d058b9c6543840c1d986f7cae4520605bf5b5c'
$toolchain = '1.99.1-x86_64-pc-windows-msvc'
$scratch = Join-Path $env:TEMP ('servo-source-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch -ErrorAction Stop | Out-Null
$source = Join-Path $scratch 'src'

git init $source
if ($LASTEXITCODE -ne 0) { throw 'Servo checkout initialization failed' }
git -C $source remote add origin https://github.com/servo/servo.git
if ($LASTEXITCODE -ne 0) { throw 'Servo origin setup failed' }
git -C $source fetch --depth 1 origin $revision
if ($LASTEXITCODE -ne 0) { throw 'Pinned Servo fetch failed' }
git -C $source -c core.longpaths=true checkout --detach FETCH_HEAD
if ($LASTEXITCODE -ne 0 -or (git -C $source rev-parse HEAD) -ne $revision) {
    throw 'Pinned Servo checkout failed'
}
if (git -C $source -c core.longpaths=true status --porcelain=v1 -uno) {
    throw 'Servo checkout has missing or modified tracked files'
}

$rustc = rustup which --toolchain $toolchain rustc
if ($LASTEXITCODE -ne 0) { throw 'Install the pinned Rust toolchain first' }
$installedToolchain = Split-Path (Split-Path $rustc -Parent) -Parent
Copy-Item -LiteralPath $installedToolchain -Destination (Join-Path $scratch 'rust') -Recurse
$rustBin = Join-Path $scratch 'rust/bin'
$llvmBin = Split-Path (Get-Command clang-cl.exe -ErrorAction Stop).Source -Parent
foreach ($file in 'clang-cl.exe','lld-link.exe','libclang.dll') {
    if (-not (Test-Path -LiteralPath (Join-Path $llvmBin $file) -PathType Leaf)) {
        throw "Missing LLVM component $file"
    }
}

$env:PATH = "$rustBin;$llvmBin;$env:PATH"
$env:LIBCLANG_PATH = $llvmBin
$env:RUSTC = Join-Path $rustBin 'rustc.exe'
$env:CARGO_HOME = Join-Path $scratch 'cargo'
$env:CARGO_TARGET_DIR = Join-Path $scratch 'target'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_NET_GIT_FETCH_WITH_CLI = 'true'
$cargo = Join-Path $rustBin 'cargo.exe'

Push-Location $source
try {
    & $cargo build --locked --target x86_64-pc-windows-msvc -p servo --example winit_minimal --no-default-features --features bundled --timings --jobs 4
    if ($LASTEXITCODE -ne 0) { throw 'The pinned Servo example build failed' }
} finally {
    Pop-Location
}
```

The output target is `target/x86_64-pc-windows-msvc/debug/examples/winit_minimal.exe`
under the scratch directory. The first local checkout produced missing tracked WPT
files with `Filename too long`. A Git command with `core.longpaths=true` restored the
missing paths in the owned upstream checkout. No system path policy changed.

## Runtime gate

The pinned example starts at an upstream HTTPS demo. The Windows implementation
reports unsupported content-process sandboxing. Use a controlled fixture and a fresh
explicit config directory before running the example on Windows. The initial
compile uses the original example so that source-build failures can be distinguished
from fixture or host changes.
[Pinned minimal example](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/servo/examples/winit_minimal.rs) ·
[Pinned sandbox implementation](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/servo/servo.rs#L1516-L1526)

Apply the fixture patch in the upstream checkout after the original compile
succeeds. It replaces the demo URL and uses a fresh config directory with
temporary storage. Keep the Windows session interactive for both runtime checks.

```powershell
$patch = Join-Path $repo 'experiments/servo/winit-minimal-local.patch'
git -C $source apply --check $patch
if ($LASTEXITCODE -ne 0) { throw 'The fixture patch does not apply' }
git -C $source apply $patch
if ($LASTEXITCODE -ne 0) { throw 'The fixture patch failed' }

$target = 'x86_64-pc-windows-msvc'
$examples = Join-Path $env:CARGO_TARGET_DIR "$target/debug/examples"
$common = @('--locked', '--target', $target, '-p', 'servo',
    '--no-default-features', '--features', 'bundled', '--jobs', '4')
Push-Location $source
try {
    & $cargo build @common --example winit_minimal
    if ($LASTEXITCODE -ne 0) { throw 'The local fixture build failed' }
} finally {
    Pop-Location
}

. (Join-Path $repo 'experiments/servo/tools.ps1')
$run = New-ServoRun (Join-Path $scratch 'runs')
Copy-Item (Join-Path $repo 'experiments/servo/fixture.html') $run
Start-Process -FilePath (Join-Path $examples 'winit_minimal.exe') `
    -WorkingDirectory $run -WindowStyle Normal -Wait
```

Check for the CSS grid, animated canvas, and “Fixture ready” message. Close the
window before continuing. Keep run directories out of Git. Inspect retained
files only in your own scratch directory.

## Compile and run the Rust Browser embedder

Stage the host as an auto-discovered example in the same pinned checkout. The
host uses the upstream WGL and keyboard helpers and the existing Rust Browser
navigation command type. The upstream manifest and lockfile stay intact.

```powershell
$stage = Join-Path $source 'components/servo/examples/rust_browser_embedder'
New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
foreach ($name in 'main.rs', 'policy.rs', 'second.html') {
    Copy-Item (Join-Path $repo "experiments/servo/embedder/$name") $stage
}
Copy-Item (Join-Path $repo 'src/engine_api.rs') $stage
Copy-Item (Join-Path $repo 'experiments/servo/fixture.html') $stage

Push-Location $source
try {
    & $cargo build @common --example rust_browser_embedder
    if ($LASTEXITCODE -ne 0) { throw 'The embedder build failed' }
    & $cargo test @common --example rust_browser_embedder
    if ($LASTEXITCODE -ne 0) { throw 'The embedder policy tests failed' }
} finally {
    Pop-Location
}

$exe = Join-Path $examples 'rust_browser_embedder.exe'
& (Join-Path $repo 'experiments/servo/embedder/check-smoke.ps1') `
    -Executable $exe -OutputRoot (Join-Path $scratch 'smoke')
$run = New-ServoRun (Join-Path $scratch 'runs')
Start-Process -FilePath $exe -WorkingDirectory $run -WindowStyle Normal -Wait
```

On the first fixture, click the counter, type in the text field, and scroll to
the marker. F2 opens the second fixture with an external link and popup probe.
Alt+Left and Alt+Right traverse history. F5 or Ctrl+R reloads, and F1 opens the
first fixture. Resize the window, then close it. The smoke script checks three
fresh runs for a ready frame, teardown, and successful exit, then checks that
the executable refuses to reuse a run.

Keep both executables on the supplied local fixtures. See
[measured results and limits](EMBEDDER_RESULTS.md) before using the experiment.
