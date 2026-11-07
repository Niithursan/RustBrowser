# Validation for Version 0.1

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
verified. The GitHub Actions workflow is supplied but has not run remotely.

The included executable is a development build from the supplied source, not a signed
release or installer. Tabs, bookmarks, privacy controls and Servo are future work.
