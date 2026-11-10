# Compatibility and reliability validation

The current [v0.11 acceptance record](DAILY_DRIVER.md) separates unit/native/GUI
results and remaining daily-driver gaps. CI runs eight native WebView2 cases,
including the controlled scale/composition comparison fixture and the modern
daily suite below. Older manual procedures/results retain their original scope.

For modern modules/fetch/crypto/IndexedDB/worker/SPA/cross-origin navigation,
diagnostics/Stop/address/theme checks and actual untrusted-certificate rejection:

```powershell
.\validation\windows-daily.ps1 -OutputRoot (Join-Path $env:TEMP ('rust-browser-daily-'+[guid]::NewGuid().ToString('N')))
```

Python 3 and OpenSSL (PATH, Git for Windows, or `-OpenSsl`) are fixture-only
prerequisites. They are not production dependencies. Generated test keys and
profiles stay outside Git; no trust-store modification is made. Run each native
test in a separate process and keep production audit/Clippy gates intact.

Use isolated disposable profiles and synthetic data. No account credentials, device
permissions, external authentication provider or real file upload is required.
Record the exact source commit, executable hash, Runtime/Rust versions, hardware,
display scaling, scenario and observed failures. Keep profiles, downloads and logs
outside Git. [COMPATIBILITY.md](COMPATIBILITY.md) records the tested subset and gaps.

## Public-website survey (manual, v0.14)

```powershell
.\validation\site-survey.ps1 -OutputRoot (Join-Path $env:TEMP 'rust-browser-survey')
```

This drives the production session against the addresses in
`site-survey-sites.txt` (or `-Urls`), one new tab per address, and writes a JSON
report: result, status text, reported failures, engine and renderer exits, and the
number of WebView2 crash dumps left in the disposable profile. `-KeepTabsOpen`
leaves every tab open and exits with the last one visible. `-BurstCloseFillers n`
opens `n` blank tabs first and, after the visits, closes every visited and blank tab
in one event-loop turn, then waits 4 s for engine exits. It needs a network
connection, enters no credentials and grants no permission. It is not a CI gate and
its results describe third-party sites on the day it ran. Do not add addresses that
require an account.

## Native regressions

Run each separately: winit permits one event loop per process. These six historical
cases remain required; CI also runs the comparison fixture and daily suite.

```powershell
cargo test --locked --bin rust-browser native_private_storage_and_confirmed_engine_deletion -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_application_clear -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_tab_sleep_resume_and_local_page_security -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_private_newtab_settings_are_ephemeral -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_popup_opener_upload_audio_fullscreen_and_redirects -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_renderer_crash_is_reported_and_reload_recovers -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_failure_reporting_tab_retirement_and_engine_restart -- --ignored --test-threads=1
```

The last one (v0.14) checks HTTP error pages, a refused connection and its Reload,
a blocked app link, a download started from the address bar, deferred destruction
of a closed tab, restart after the fixture ends its **own** WebView2 browser
process, reopen-closed-tab, select-by-position and the delayed shutdown.

The compatibility test binds a random loopback port, generates quiet PCM audio, and
uses disposable folders for synthetic HTTP downloads. Test-only CDP user gestures
exercise popup/fullscreen/media APIs; production exposes no CDP or application IPC
to websites. A page-originated timer independently tests runtime popup blocking.
The renderer crash test targets only its disposable fixture renderer. Regular tests
kill an owned child holding a session lock and uncommitted SQLite transaction, then
check recovery and five repeated store-open/close cycles. They do not kill a user's
browser or fill a real disk to simulate ENOSPC.

## Interactive loopback fixture

Python 3 is optional for this manual fixture; it is not a browser/runtime dependency.
Start it from the repository root:

```powershell
python .\validation\fixture_server.py --port 8769
```

In a separate PowerShell terminal:

```powershell
cargo build --locked --release
$run = Join-Path $env:TEMP ('rust-browser-v06-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run | Out-Null
$previousProfile = $env:RUST_BROWSER_PROFILE
try {
    $env:RUST_BROWSER_PROFILE = Join-Path $run 'profile'
    $browser = Start-Process .\target\release\rust-browser.exe -ArgumentList 'http://127.0.0.1:8769/' -PassThru -WindowStyle Hidden
} finally {
    $env:RUST_BROWSER_PROFILE = $previousProfile
}
.\validation\sample-process.ps1 -RootProcessId $browser.Id -OutputRoot (Join-Path $run 'measurements') -Minutes 30
```

The console can be hidden while the native browser window remains visible. Child
private windows inherit the isolated profile root from the browser process. The
sampler writes counts/CPU/memory CSV plus hardware/method context; it never captures
URLs, page content, filenames or command lines, and never closes the browser.
`-Seconds 5` is available only for a collector smoke check. Use `-Minutes 60` for a
longer interactive run. Closing the browser ends sampling early and records that fact.

For a repeatable **30-minute interactive session** (repeat minutes 5–25 once for an
approximately 50-minute session):

| Minutes | Controlled workload and acceptance checks |
| --- | --- |
| 0–5 | Open the fixture and several tabs. Complete its synthetic popup; verify message/cookie status and return to the opener. Open a `noopener` tab and follow the redirect. Test back/forward/reload and address editing. |
| 5–10 | Choose **only `validation/upload-fixture.txt`** in the native file picker and submit to loopback. Verify byte receipt. Start quiet audio, synthetic video and local WebRTC; require “connected” and advancing frames in both video panels. No camera/mic grants are needed. |
| 10–15 | Turn on **Keep awake** for the live media tab, switch away for at least two minutes, and return. Check media/connection continuity. Exercise fullscreen and Escape; the origin toolbar should remain visible. Stop media afterwards. |
| 15–20 | Start a slow fixture download, select a disposable destination, switch tabs and inspect progress. Finish or cancel; never execute the downloaded text. Repeat a completed download and verify the existing file is preserved with a suffixed new filename. |
| 20–25 | Create/close/reorder/pin tabs repeatedly. Use Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+L, F7, move/pin shortcuts, tab-strip arrows and the all-tabs selector. Resize from wide to the minimum width and revisit every tab. |
| 25–30 | Close/restart normally using the same isolated profile. Verify URLs, order, pins, favorites and settings. Record resource recovery after closing tabs and any errors. Stop the fixture server and close test windows when finished. |

For an **abnormal-termination extension**, save the isolated browser process ID,
wait for the session file to be committed, and terminate **only that owned test
process**. Relaunch with the same isolated profile and verify restoration. Do not
terminate other browser/runtime processes. Do not infer recovery of unsaved forms,
in-flight downloads or the latest debounced changes. Native/regular tests cover
renderer failure, SQLite rollback and interrupted snapshot recovery separately.

Test 100%, 125%, 150% and 200% scaling on available displays: move the window between
monitors, resize, reopen managers/popups, inspect hit targets/text clipping and check
focus after tab switches/fullscreen. Record each configuration actually used. With a
screen reader, verify names, selected-tab state, loading/errors, logical reading order,
manager controls and native file selection. DOM/UIA inspection alone is not a spoken
screen-reader acceptance test. Physical sleep/wake and real device-call tests should
be scheduled separately; do not suspend another person's workstation during a run.

## Automated extended churn soak

Build the release probe using [benchmark instructions](../benchmarks/README.md), then:

```powershell
.\benchmarks\windows.ps1 -ProbeExecutable $probeExecutable -OutputRoot $emptyMarkedOutput `
    -Label v0.6.0 -TabCounts 20 -FreshOnly -Churn `
    -SettleSeconds 30 -SampleSeconds 10 -SoakSeconds 1800 -RecoverySeconds 15
```

Use `-SoakSeconds 3600` for an hour. Without `-FreshOnly`, both fresh and warm runs
execute, doubling the soak time. Every ten seconds the test closes the prior scratch
tab, creates/navigates a new one and switches to one of the twenty stable tabs. The
soak therefore has twenty base tabs plus at most one scratch tab. It then switches
through the base tabs, checks document responsiveness, closes to one tab and exits.
All activity and metrics are test-only. Include WebView2 descendants when measuring.

This automation is **not 30 minutes of human interactive browsing**, a real-site
compatibility guarantee or a memory-leak proof. It complements the interactive
procedure and short security/media/crash regressions. Separate resource observations
from claims of performance improvement, and retain failures and workload interference.
