# Validation

## Version 0.2.0

Local Windows x64 validation with Rust 1.99.1/MSVC and the installed WebView2 Runtime:

- Formatting, strict Clippy, debug build and optimized release build passed.
- 11 unit/integration tests passed: existing URL behavior, tab IDs/selection/closure,
  tab limits, search encoding for all three providers, session/settings round-trip,
  exclusive profile lock/release, last-queued save, atomic replacement, invalid and
  oversized snapshots, corrupted-file recovery, and background write-error reporting.
- Live desktop checks verified Ctrl+T, Ctrl+W, next/previous-tab shortcuts, independent
  addresses/history, redirect URL updates, selection of DuckDuckGo, and actual search
  result rendering. Restart restored two tabs in order, active tab 2 and DuckDuckGo.
- A startup focus issue found during GUI testing was corrected by restoring content
  focus once the toolbar signals readiness.
- Dependency audit passed using the repository's existing reviewed exceptions. No
  dependencies or audit exceptions were added. CI results are recorded on the PR.

The restricted execution environment denied filesystem rename operations during the
first persistence-test run. The same tests passed with normal Windows filesystem
access. A later test build encountered the running executable's Windows file lock;
closing the smoke-test browser allowed the build and tests to pass.

Limits: restoration saves URLs/order/active selection/settings, not history stacks,
form input, scroll position or JavaScript state. Session data is plain text in the
local profile. The write debounce can lose recent changes during an abrupt process
termination; normal shutdown flushes pending data. Every restored tab loads its URL.
There is a 64-tab limit, no suspension, no tab reordering and no private mode. Tabs
share cookies and site storage; separate navigation does not mean isolated identities.
Google/Bing query generation is tested automatically; their live results were not
separately verified. No broad compatibility, accessibility, mixed-DPI, crash/power-loss
durability or long-running resource review was performed. Tag-driven release
publication was inspected but not executed; no release tag was pushed.

## Historical Version 0.1 validation

Validated locally on Windows x64 with Rust 1.99.1 (MSVC), winit 0.30.13,
Wry 0.57.1, webview2-com 0.39.1 and the installed WebView2 Runtime.

## Passed

- `cargo build --locked`: compiled and linked the Windows executable.
- `cargo fmt --check`: passed.
- `cargo test --locked`: two URL-policy unit tests passed, including normalization,
  localhost ports, paths/query/fragments, invalid input, credentials and blocked schemes.
- `cargo clippy --locked -- -D warnings`: passed.
- Launched the actual native desktop window and inspected its toolbar and rendered page.
- Rendered `https://example.com/` and navigated through the address bar to
  `https://www.rust-lang.org/`; the redirected `https://rust-lang.org/` address appeared.
- Alt+Left returned to the first page and enabled Forward. Alt+Right returned to Rust.
- Ctrl+L selected the address while a webpage had focus, confirmed visually.
- Navigating to `https://rust-browser-smoke.invalid/` produced an error status in
  the toolbar and WebView2's DNS error page. Connecting feedback was observed.
- F5 attempted reload on the failed page and produced native error feedback.
- The Home button navigated back to the home address after the failed request.

The first restricted execution could not resolve external domains. The network-enabled
execution subsequently rendered real HTTPS pages. Runtime testing found and fixed a
toolbar initialization bug: the immutable embedded HTML URL must be allowed explicitly.

## Not established by these checks

No comprehensive website compatibility, long-running reliability, security audit,
certificate-interstitial behavior, authenticated flows, popup flows, download behavior,
permission behavior, screen-reader review, mixed-DPI/multi-monitor testing, persistent
site-storage restart testing, Windows ARM64 testing or clean-machine installation
testing was performed. Stop, Alt+D, F6, Ctrl+R and Ctrl+Tab are implemented but were
not separately exercised in this smoke test. Successful-page reload was not separately
verified. Current remote CI results are available in the repository's
[Actions history](https://github.com/Niithursan/RustBrowser/actions).

CI produces an unsigned Windows executable. Version tags configure portable ZIP
packaging and checksums; this is not a signed installer. Tabs, bookmarks, privacy
controls and Servo are future work.
