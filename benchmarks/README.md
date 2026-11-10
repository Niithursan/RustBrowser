# Windows performance procedure

Current [v0.14 matched results and soak](V014_RESULTS.md) compare 0.13.0 with 0.14.0.

Current [v0.11 matched results](V011_RESULTS.md) and
[sanitized per-run numbers](v011-corrected.json) compare unchanged production
0.8.0 with 0.11.0 under the same probe. [RESULTS.md](RESULTS.md) retains historical
v0.4/v0.5 findings. Fresh-profile startup is not an OS-cold measurement.

Use a current Windows x64 machine with MSVC, the pinned Rust toolchain and WebView2.
Run from the repository root. Close other Rust Browser test windows and avoid builds,
updates and other heavy work while measuring. Leave the test window unobstructed and
the machine on the same power plan. Never point these tools at your everyday profile.

## Build the probe

The ignored `windows_performance_probe` test uses the real application and native
WebView2 controllers. It is compiled only into the test executable. The shipping
browser contains no benchmark fixture, page instrumentation or metrics writer.

```powershell
$scratch = Join-Path $env:TEMP ('rust-browser-benchmark-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
$artifactLog = Join-Path $scratch 'build.jsonl'
cargo test --locked --release --bin rust-browser --no-run --message-format=json > $artifactLog
if ($LASTEXITCODE -ne 0) { throw 'Probe build failed' }
$artifact = Get-Content $artifactLog | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'rust-browser' -and $_.profile.test -and $_.executable } |
    Select-Object -Last 1
Copy-Item -LiteralPath $artifact.executable -Destination (Join-Path $scratch 'current.exe')
Get-FileHash (Join-Path $scratch 'current.exe') -Algorithm SHA256
.\benchmarks\windows.ps1 -ProbeExecutable (Join-Path $scratch 'current.exe') `
    -OutputRoot (Join-Path $scratch 'current-results') -Label v0.5.0 -Repeats 3
```

The default procedure measures 1, 5, 10 and 20 tabs, each with a fresh profile then
the same profile reopened, 30 seconds settling, 10 seconds idle sampling, 120 seconds
soak and 15 seconds recovery after closing all but one tab. It can take an hour with
three repeats. Short screening runs can use `-SettleSeconds 20 -SampleSeconds 8
-SoakSeconds 0 -RecoverySeconds 8`. Use a separate output root for each comparison.
Output directories must be empty or explicitly marked by the harness. The probe
also requires that marker; neither tool deletes arbitrary directories. Results,
synthetic profiles and logs stay outside version control. Close the test processes
before manually removing the disposable directory.

To compare v0.4 without changing `main` or the current working branch:

```powershell
$baseline = Join-Path $scratch 'baseline-source'
$archive = Join-Path $scratch 'baseline.zip'
git archive --format=zip --output=$archive 8e97713fa9c006586e250f07fdc4af12003b192d
if ($LASTEXITCODE -ne 0) { throw 'Baseline archive failed' }
Expand-Archive -LiteralPath $archive -DestinationPath $baseline
Copy-Item .\src\benchmark_probe.rs (Join-Path $baseline 'src\benchmark_probe.rs')
Add-Content -LiteralPath (Join-Path $baseline 'src\app.rs') -Encoding utf8 -Value "`n#[cfg(test)]`n#[path = `"benchmark_probe.rs`"]`nmod benchmark_probe;"
cargo test --manifest-path (Join-Path $baseline 'Cargo.toml') --locked --release `
    --bin rust-browser --no-run --message-format=json > $artifactLog
if ($LASTEXITCODE -ne 0) { throw 'Baseline probe build failed' }
$artifact = Get-Content $artifactLog | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'rust-browser' -and $_.profile.test -and $_.executable } |
    Select-Object -Last 1
Copy-Item -LiteralPath $artifact.executable -Destination (Join-Path $scratch 'baseline.exe')
.\benchmarks\windows.ps1 -ProbeExecutable (Join-Path $scratch 'baseline.exe') `
    -OutputRoot (Join-Path $scratch 'baseline-results') -Label v0.4.0 -Repeats 3
```

Only the identical test-only probe/module declaration is added to the archived v0.4
source. Its production behavior is unchanged. Alternate version/run order for stronger
comparisons; the recorded screening comparison ran baseline first.

## What is measured

- A virtual HTTPS host serves only a bundled synthetic HTML page. Each tab runs a
  250 ms timer with 4 ms of busy work and publishes its tick count in its title.
  Chromium normally throttles hidden timers already. There are no external requests,
  real browsing URLs, accounts or personal data in the fixture or result JSON.
- The same probe preloads a session with 1/5/10/20 tabs, requests a 15 second sleeping
  delay on v0.5, and preserves the same URLs/settings on the subsequent warm-profile
  run. v0.4 ignores the new performance settings. Product default remains 120 seconds.
- `construction_ms` starts just before event-loop creation (after fixture/session
  setup) and ends after native session construction.
  `first_usable_ms` waits for native security initialization, toolbar-ready and active
  page completion. `all_loaded_ms` waits for all fixture completions. These are
  **instrumented startup proxies**, excluding executable loader/test-runner startup
  and pixel presentation. Fresh profile is **not OS-cold startup**: OS/Runtime/disk
  caches are not flushed. Warm profile is the same profile reopened after child exit.
- PowerShell samples the root and recursively discovered WebView2 descendants about
  every 0.75 seconds plus WMI overhead. It records process count, summed working set,
  private bytes and cumulative CPU deltas. Summed working set double-counts shared
  pages; private bytes are committed address space, not physical RAM. Unavailable
  processes are counted; exited-between-sample CPU is not captured. CPU seconds per
  elapsed second is one-core utilization; divide by logical CPU count for machine
  utilization. Exclude startup/phase transitions when summarizing steady-state CPU.
- `tick_delta` measures per-tab title activity during the idle interval. Zero hidden
  ticks together with the separate native `IsSuspended` regression test are evidence
  of this workload sleeping; zero alone does not prove API state or complete inactivity.
- Each tab switch records synchronous command time and delay to a callback from the
  selected document. This proves native/JavaScript responsiveness, not paint latency.
- Closing all but one tab is followed by a separately labelled recovery sample.
  Normal shutdown must finish successfully. The harness waits up to 60 seconds for
  sampled engine children to exit before the next run; otherwise it stops.
- The test's 100 ms polling and Windows test runner introduce common overhead.
  Real sites, audio/capture/uploads/downloads, browser/runtime versions, DPI and
  shared process allocation can produce very different results. This is not a
  Firefox/Chrome comparison, a memory leak proof or an everyday browsing benchmark.

Record CPU/RAM/OS/Runtime/toolchain, scale factor, power conditions, source SHA and
probe hash alongside results. Report medians and min/max across independent repeats;
fresh and warm runs are different conditions, not repeated samples of one condition.
Keep outliers and failed runs documented. Do not publish raw user-profile data.

## Extended and interactive validation

Run a separate 20-tab soak with `-TabCounts 20 -SoakSeconds 1800 -FreshOnly -Churn`,
then perform the [interactive procedure](../validation/README.md). v0.6 raises the
automated cap to sixty minutes and adds tab creation/closure/switching every ten
seconds. This does not establish everyday real-site reliability. Revisit all tabs,
verify forms and in-page state, play audio, keep a slow download active, request a
permission without granting it, and check that protected tabs stay awake. Compare
timer activity with automation disabled, and use Keep awake for live-site work.

For **true cold startup**, use a dedicated machine/VM, reboot before each sample,
record process creation and first usable/presented window with ETW/WPA or equivalent,
and repeat independently. This has not been performed for the recorded screening run.
For visual switch latency, use frame/presentation traces rather than the JS proxy.
Validate 100/150/200% DPI and a screen reader independently; never infer them from
DOM attributes or scale-factor arithmetic alone.

Measured results and their specific limitations are recorded in [RESULTS.md](RESULTS.md).
