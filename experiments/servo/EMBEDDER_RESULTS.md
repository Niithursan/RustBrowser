# Windows Servo embedder results

Validated on 2026-11-08 on Windows 11 Pro 10.0.26200 x64, AMD Ryzen 5 5600
(12 logical processors), and 31.93 GiB RAM. The Servo revision, toolchain,
prerequisites, and commands are in the [source-build procedure](SOURCE_BUILD.md).

## Source build

| Windows x64 WGL build | Elapsed | Outcome |
| --- | ---: | --- |
| Original upstream example, clean target and Cargo cache | 401.22 s | Compiled, including dependency fetch |
| Upstream example with local fixture patch, warm dependencies | 6.45 s | Compiled and rendered the fixture |
| Rust Browser embedder, warm dependencies | 6.48 s | Compiled, two policy tests passed |

The original build used four Cargo jobs, an unoptimized dev profile without
debug info, and disabled incremental compilation. The build process tree
peaked at **6,994.3 MiB sampled private memory** across 184 samples. The
collector missed 59 individual process observations, so this is a sampled
peak. The scratch build directory occupied **6,936,629,841 logical bytes**
including the copied toolchain, Cargo cache, target, and logs. This excludes
the source checkout. The OS cache was left in its normal state.
The pinned library emitted minimal-feature warnings, and its binding
generator used the available Python fallback when `uv` was missing.
Strict Clippy for the experiment stopped in the pinned `servo` library
because its `GenericSender` import is unused with this feature set.
The production strict Clippy check passed.

The original example hardcodes a public HTTPS demo. The fixture patch was
applied only after that source compiled successfully. The patched upstream
example rendered the CSS grid, “Fixture ready” status, and animated canvas.
The window and root process were gone after Alt+F4. The observer expired
before closure, so no exact exit code was recorded for the upstream example.
Its profile retained two files.

## Native host checks

- One winit window rendered the CSS grid, JavaScript, and animated canvas.
  The fixture reported `window.ipc` absent. Pointer focus, button click,
  keyboard text entry, and wheel scroll updated the fixture visibly.
- F2 opened the second page. Back and Forward restored the respective
  fixtures and their state. Reload reset the second fixture's status text.
  The external link and popup probes were refused, and one window remained.
- Maximize resized the view and repainted the fixture at observed scale 1.0.
  The interactive run closed normally with exit code 0 and a teardown marker.
  Cached traversal exposed a stale refusal title during testing. The final
  build displayed “Fixture history restored” on Back and Forward, then closed
  normally with exit code 0. The observed console child exited as well.
- Three final-build smoke runs reached the first ready presentation in
  **612, 781, and 624 ms**. Each logged teardown and exited 0. A fourth
  launch refused to reuse the third run and preserved its fixture hash.
  The final executable SHA-256 was
  `fb7601c13eb1994d57657d70398c91e61010269c1ebb8874601322c56f1577b8`.

The smoke metric starts after constructing the winit event loop and ends at
the first host presentation after load completion with the expected title.
It does not measure OS launch time or the first visible pixel. Equivalent
WebView2 presentation timing and numeric input latency are unavailable.

## Process-tree resource sample

The [numeric collector](../../validation/sample-process.ps1) observed one
1200 × 820 logical fixture window per run. WebView2 ran first with
Runtime 157.0.4341.42, followed by the final Servo executable. Both used
fresh profiles and displayed the same fixture HTML and canvas workload.
The shared fixture SHA-256 was
`89e77a3810fcc78b4032f298beee38012a0174c7aca2c42094cfcd5c093a899f`.
Each sample began after visual confirmation and at least 20 seconds of
settling. The [v0.7 procedure](README.md#controlled-input-and-resource-comparison)
describes the loopback server and one-tab WebView2 setup.

| Shell | Samples / observed time | Mean private memory (range) | Mean summed working set | Mean process count | CPU per logical core |
| --- | ---: | ---: | ---: | ---: | ---: |
| v0.8 native Servo embedder | 34 / 30.646 s | 277.75 MiB (277.69–277.88) | 186.97 MiB | 2 | 6.14% |
| v0.8 Rust Browser with WebView2 | 31 / 30.028 s | 325.49 MiB (319.58–328.62) | 697.74 MiB | 14 | 21.57% |

The sampler includes the root and discovered descendants. It uses CPU time
between samples divided by the sampling interval. Summed working sets can
count shared pages more than once. Both applications closed normally with exit
code 0. The one observed Servo descendant (`conhost.exe`) and all 13 observed
WebView2 descendants exited.

Servo used a dev build and a local file with an injected CSP. WebView2 used a
release build, the normal browser chrome, and loopback HTTP. Its executable
SHA-256 was `d5b180125d66d0fe94872ed6760dd7b2a9d0ce560ceb7d49635971267b2ebed8`.
Each row represents one run. The different origin storage, content viewport,
process model, and uncontrolled system activity limit comparison to these
two runs. The [v0.7 observation](RESULTS.md) had a different CPU result.

## Boundaries and remaining checks

- The host uses the pinned single-process defaults. The Servo Windows
  content-process sandbox is unsupported. The process uses ordinary user
  privileges, and the fixture gates are application policy. The runtime
  was exercised only on bundled local pages. HTTPS, permissions that depend
  on a secure origin, and general download denial remain unverified.
  The source keeps TLS validation
  enabled and does not expose a bridge, file picker, or external launcher.
  [Pinned sandbox implementation](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/servo/servo.rs#L1516-L1526)
- Each completed host run retained five files in its disposable profile.
  They were `auth_cache.json`, `cookie_jar.json`, `hsts_list.json`, and two
  `reg.sqlite` files in separate subdirectories. Keep test data outside
  real profiles and Git. The host leaves its run directory for inspection.
- The separate upstream workspace lock audit reported `RUSTSEC-2023-0071`
  in `rsa 0.10.0-rc.19`, 11 unmaintained-package warnings, and one yanked
  package. The reverse dependency query for `rsa` found no match in the
  selected Windows Servo graph with bundled resources. This audit covers
  the upstream workspace lockfile. The separate production audit passed.
- IME composition, mixed-DPI transitions, compositor timing, and long-run
  reliability remain unverified. The manual Windows source-build procedure
  keeps the heavy experiment out of standard production CI.

The next milestone should verify a Windows process-isolation design and
add repeatable IME, scaling, and process-exit checks. Instrument the same
presentation and input endpoints in both shells before comparing latency.
