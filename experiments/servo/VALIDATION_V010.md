# v0.10 investigation and validation

Validated locally on 2026-11-08. PR #12 was squash-merged at
`bba82c9b6a34c990f1e04f70c32abeda898f5e23` before the feature branch was
created. Merged [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42511501005)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42511500878)
passed. The matching v0.9 tree's
[experimental boundary check](https://github.com/Niithursan/RustBrowser/actions/runs/42508530702)
passed. Production remains version 0.8.0 and uses WebView2.

## Verification

| Check | Actual local result |
| --- | --- |
| Production `cargo fmt --all -- --check` | Passed |
| Production strict all-target/all-feature Clippy, locked | Passed |
| Production locked all-target/all-feature tests | 47 passed; 8 opt-in native/performance tests ignored by the regular command |
| Windows debug and release builds, locked | Passed |
| Production `cargo audit --deny warnings` | Passed with the unchanged, documented non-Windows exceptions |
| Existing native WebView2 regressions | All seven passed individually, including sleep/resume races, isolation/deletion, popups/uploads/media, renderer crash/reload and shared fixture |
| Launcher safety suite / Python fixture-server tests | Passed / 2 passed |
| Synthetic Windows compilation | `/W4 /WX /MT` passed |
| Fixed-frame mutation suite | Passed: twelve field mutations, run identity, replay sequence, message kind, exact size and invalid diagnostic opcode |
| Original standard AppContainer suite | 3 final repeated runs passed; failed startup, worker/job termination and broker recovery passed |
| Original LPAC suite | **Failed**, worker exit 70107 encodes WSAStartup 10107; owned-profile cleanup reported |
| New diagnostic collectors | 3 measured runs per mode passed collection, token/file/process/handle checks and resource controls; failed diagnostic operations remain reported |
| Actual malformed/hung worker | Rejected and terminated in standard AppContainer and LPAC; hang deadline 200 ms |
| Live-IPC broker failure | Broker deliberately exited 74; owned worker exited with job closure; exact profile recovered in both modes |
| Pinned Servo example unit tests | 7 passed |
| Changed example Rust formatting / host compiler warnings denied | Passed / passed |
| Strict Clippy for pinned Servo package | **Failed**, existing unused `GenericSender` import in upstream `components/servo/servo.rs:57` with minimal features |
| Separate upstream lockfile audit | **Failed**, `RUSTSEC-2023-0071` in rsa plus 11 unmaintained warnings; no new exception added |
| Servo smoke and profile-reuse refusal | 3 fresh runs painted and exited 0 with `teardown_complete`; reused run refused without changing its fixture |
| Shared four-scale synthetic fixture | 3 runs per engine, all passed; Servo DOM CompositionEvent constructor remains unavailable |

Production CI requirements remain intact. The experimental Windows workflow
now adds diagnostic collection for both modes after the unchanged baseline
suite. A green LPAC collector means its boundary assertions and collection
worked; it does not turn Winsock failure into success. The original explicit
LPAC full-suite command remains available and failing on this machine.
Final remote CI must be checked on the PR's exact head.

The first remote boundary run on `fe092fb` failed the non-elevation assertion
on the administrative hosted runner, before worker resume; profile cleanup
was reported. The fix derives a restricted LUA primary token, disables
maximum privileges and makes Administrators deny-only before lowbox
creation. The assertion remains mandatory. The updated original and
diagnostic suites passed on
[hosted Windows Server 2025](https://github.com/Niithursan/RustBrowser/actions/runs/42559913981)
at `acbf21704b4a6295afd8db2fe29f6f402f32abcd`, and locally on Windows 11.

The upstream lockfile is unchanged (SHA-256
`eb0cc53c1a4044738088cd90825409034014ee65d6f51d6e00448617996e9031`).
The existing selected-graph investigation in [v0.8 results](EMBEDDER_RESULTS.md)
found no rsa dependency in the selected bundled Windows graph. A workspace
audit failure and native dependencies still prevent any claim that Servo is
security-certified. Upstream warnings are visible; production checks were
not weakened to accommodate the separate experiment.

## Actual Windows GUI validation

The owned Servo executable displayed the bundled fixtures at device scale
1 on the interactive desktop. The second fixture's visible counters
observed native key down/up for `a`, pointer movement/down/up, focus/blur,
and a wheel event while the page scrolled. Tool-assisted text entry
displayed `a தமிழ்🙂` and preserved it through maximize/restore.
The viewport changed from 1200 x 820 to 1920 x 1009 and back; resize
counters advanced. This is a qualitative delivery check, not a latency
measurement. Text insertion through the UI tool does not prove hardware
Unicode key generation or Windows IME composition.

Alt+F4 closed the owned window. The retained process handle reported exit 0
and the host emitted `teardown_complete`. Five generated profile files
remained: auth_cache.json, cookie_jar.json, hsts_list.json, and two reg.sqlite
files in separate subdirectories. No erasure guarantee is made. The host
still uses ordinary user privileges and no normal profile was provided.

The installed language inventory listed US keyboard layouts for English
(US) and English (Canada), with no composing IME. One monitor was enumerated.
Native IME candidate selection/cancellation, OS autorepeat, physical
125/150/200% scaling and mixed-monitor transitions were **not verified**.
The synthetic fixture separately checks two keydowns (one repeat), keyup,
DOM composition/cancellation where available, transformed pointer hit
tests and engine device scales 1, 1.25, 1.5 and 2. Unit tests cover empty
preedit/commit and dismissal without recommitting prior text.

## Reproducible measurements

The sanitized [numeric samples](isolation/measurements-v010.json) contain
three sequential fresh-profile runs per worker mode, all sixteen raw echo
times per run, diagnostic return codes, three Servo smoke runs and three
four-scale reports per engine. Collection followed compilation, on an
uncontrolled interactive desktop, without a concurrent project build.
These are warm-binary/fresh-profile runs; OS cache and unrelated apps were
not controlled. Hardware: Ryzen 5 5600, 34,281,668,608 bytes RAM; Windows 11
Pro 10.0.26200; MSVC 19.44/SDK 22621; Rust 1.99.1. The Servo pin and build
configuration are recorded in that file and [SOURCE_BUILD.md](SOURCE_BUILD.md).

| Synthetic worker metric | AppContainer median (range), n=3 | LPAC median (range), n=3 |
| --- | ---: | ---: |
| Creation to first validated IPC reply, ms | 27.265 (21.122-27.933) | 18.715 (15.819-25.420) |
| Resume to observed process exit, ms | 323.685 (315.559-410.642) | 370.719 (328.815-383.719) |
| Process CPU time, ms | 0 (0-15.625) | 0 (0-0) |
| Peak private committed memory, MiB | 2.117 (2.113-2.121) | 2.070 (2.066-2.074) |
| Median of each run's 16 IPC echoes, ms | 15.776 (15.618-30.815) | 23.697 (15.647-23.806) |

Creation-to-reply includes primary-token filtering, process creation, token checks, endpoint cleanup,
resume and polling. Resume-to-exit includes all echoes and diagnostics; it
is not termination latency. CPU accounting is coarse (15.625 ms steps);
zero does not establish zero CPU use. Memory is Job peak private commit,
not working set or GPU memory. Descendants are prohibited and actual child
creation was denied, so there is no omitted permitted worker process tree.
IPC uses `Sleep(1)` polling; the observed 15-32 ms scheduling dominates many
samples. This protocol is a boundary test, not a low-latency transport.

Single fault samples measured Job termination to observed exit at 0.706 ms
(malformed) and 0.831 ms (hang) for AppContainer. LPAC-specific
fault cases measured 0.847 ms and 0.997 ms respectively. The hang's 200 ms
detection deadline precedes this shutdown interval. The supervisor waits
two seconds after intentional broker termination before checking child
identity; that recovery check does not measure precise exit latency.

Servo's three host-ready presentation samples were 778, 744 and 796 ms.
That endpoint starts after the winit event-loop construction and ends at
the host's ready presentation callback. It is not OS startup or first
visible pixel. Common script-to-requestAnimationFrame samples are retained
per engine and scale; they use synthetic DOM input. Composition is absent
on pinned Servo. Different hosts, storage origins and uncontrolled desktop
activity prevent speed rankings. Native input-to-display latency, GPU
utilization, restricted frame transfer and a long-duration soak are
unavailable. No Servo performance improvement is claimed.

From the repository root, after following the pinned source-build setup:

```powershell
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-ac') -Diagnostics -Repeats 3
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-lpac') -Diagnostics -Mode LPAC -Repeats 3
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-baseline') -Repeats 3
# Investigative full LPAC suite: currently fails with 10107; preserve the output.
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-lpac-full') -Mode LPAC
```

Run each smoke/validation script from SOURCE_BUILD.md with `-Repeats 3`,
and run `cargo test --locked --bin rust-browser native_servo_comparison_fixture
-- --ignored --test-threads=1 --nocapture` in three separate invocations.
Run collectors sequentially. Keep raw outputs/profiles outside Git. For
IPC summaries, take the median of sixteen echo values, then the median and
min/max of three per-run values; retain raw numeric samples to show spread.

## Remaining security work

The [rendering decision](SECURE_RENDERING_V010.md) documents the WGL/Win32k
blocker and the next acceptance gates. The exact internal Winsock failure,
general network policy, real engine containment, graphics transport,
longer stress, native IME and mixed-DPI behavior remain open. The next
milestone should verify a minimal supported graphics prerequisite under
the restrictions before adding Servo to a worker. Arbitrary content stays
blocked throughout that work.
