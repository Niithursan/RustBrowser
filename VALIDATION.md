# Validation

## Version 0.4.0

Baseline: PR #6 was confirmed squash-merged at `3117426840c0d4338e1537d25551529fb4ecf2f1`.
The merged tree matched the verified v0.3 feature tree, and all 21 baseline tests
passed locally. Its [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42330920256)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42330920275)
passed. Development used a feature branch; no release/tag was created or changed.

Local checks on Windows x64, Rust 1.99.1/MSVC and WebView2 156.0.4314.58:

- Formatting, strict Clippy, 28 regular tests and Windows debug/release builds passed.
- The dependency audit passed for 369 locked dependencies with the existing reviewed
  exceptions unchanged. No dependency was added.
- Tests cover private lease isolation and abandoned-directory cleanup, snapshot
  exclusion, queued-write barriers, exact-origin permissions/revocation, confirmed
  database clearing, invalid settings, migration/corruption and connection indicators.
- The native isolation test used actual WebView2 views and a test-only local fixture at a reserved origin.
  It verified two-way cookie/local-storage isolation, a successful clear callback,
  cleared normal storage with private storage intact, origin/capability correctness,
  cancellation denial, and an explicit synthetic notification grant.
- The application-level native test verified unconfirmed clearing is rejected,
  active tabs are replaced by a blank view, the native callback and database worker
  both complete, bookmarks and library recovery files remain, and saved session URLs
  and recovery files are removed. A separately injected failed engine callback
  verified visible partial-failure reporting and denial of further grants.

The two native tests open real windows and run separately because winit supports one
event loop per process. CI runs them explicitly after the regular suite:

```powershell
cargo test --locked --bin rust-browser native_private_storage_and_confirmed_engine_deletion -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_application_clear -- --ignored --test-threads=1
```

Actual Windows GUI and process checks used isolated test profiles:

- Normal cookie and local-storage markers were absent from a private window on the
  same origin. Remote content reported no application IPC object. Normal markers
  were still present after private shutdown. The private window created neither
  `session.json` nor `library.sqlite`, and its temporary directory disappeared on
  normal close. Ctrl+Shift+N opened a correctly labelled private window.
- A unique private navigation did not appear in the normal session or SQLite
  history. After intentionally terminating the private test process, a later
  launch removed its abandoned directory and kept the current live directory.
- A normal five-tab test process was terminated after its session was saved. On
  restart, all five tabs and the active URL restored; normal shutdown then succeeded.
  This does not prove recovery of changes lost during the write debounce or an OS crash.
- HTTP and HTTPS displayed their actual scheme/origin with distinct descriptions.
  A loopback notification request displayed the requesting origin and capability;
  switching tabs dismissed it. Permission decisions and data clearing were exercised
  in the controlled native tests above, not by granting devices or deleting user
  data through the desktop UI. A `target=_blank` HTTP link opened a third normal tab.
- A 4 MiB slow HTTP transfer showed live progress and cancelled at 88%; no staged
  or final file remained. A deliberately truncated response showed Failed after
  the two-minute stall guard, leaving no current staging files. Abrupt termination
  during a transfer can still leave a staging folder at the chosen destination.

Process-tree samples over five seconds after loading pages:

| Tabs | Processes | Summed working set (MiB) | Private bytes (MiB) | CPU seconds |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 14 | 792.0 | 352.8 | 0.812 |
| 5 | 17 | 915.9 | 427.5 | 0.031 |
| 10 | 22 | 1131.2 | 507.9 | 0.000 |

These include browser and WebView2 descendants. Summed working sets can count shared
pages more than once. The two-tab run included an HTTPS home page; five and ten tabs
used static loopback content. These brief samples do not characterize real websites,
long-term growth, or background media. No suspension or performance gain is claimed.

Limits: private cleanup is best effort; private mode does not provide anonymity,
secure erasure, or protection from Windows and network observers. Downloaded files
and metadata remain. Library recovery copies may retain history and permission rules.
Physical camera/microphone/location access, broader site/redirect compatibility,
mixed content, accessibility, long-duration soak tests and real OS crashes were not
comprehensively verified. CI results on the final revision are recorded on the PR.

## Version 0.3.0

Preservation baseline: PR #5 was verified squash-merged into `main` at `7c9c152`.
The merged commit's [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42326479368)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42326479361)
completed successfully; its 11 baseline tests passed locally. No v0.2.0 tag or release
existed. The owner requested that v0.2.0 remain unpublished. No existing release was modified.

Local validation uses Windows x64, Rust 1.99.1/MSVC and the installed WebView2 Runtime.
Final local formatting, strict Clippy (`-D warnings`), all 21 automated tests,
debug/release builds, and `cargo audit --deny warnings` passed. The audit scanned
369 locked dependencies using the existing reviewed exceptions; no new exception
was added.
Automated coverage includes the existing v0.2 tests plus SQLite bookmarks/folders,
edit/search/remove/restart behavior, transactional v1→v2 migration, corrupt-file
preservation/recovery, newer-schema refusal, invalid inputs and SQL text, visit
deduplication, date ranges, query/credential exclusion, clear confirmation enforcement,
Windows filename/device/stream/traversal handling, atomic collision protection,
simultaneous finalization, partial cleanup, stalled-transfer tracking, and legacy
settings compatibility with the persisted recording toggle. CI results for the final
revision are recorded on the pull request, not assumed from local checks.

Actual Windows GUI checks performed using an isolated profile:

- Ctrl+D saved a bookmark; Ctrl+Shift+O opened its manager. A Research folder was
  created, the bookmark title was edited and assigned to it, and both survived restart.
  The saved bookmark opened in a new independent tab.
- Ctrl+H displayed recorded visits with titles and local timestamps. A nonmatching
  query produced an empty result. Clear history displayed a confirmation dialog;
  cancelling it preserved entries. Actual clearing/removal and date-boundary behavior
  are covered by automated tests rather than claimed as manually verified.
- A loopback HTTP fixture sent a traversal-style suggested name. Native folder
  selection, live byte/percentage progress, completion and a second collision-safe
  suffixed file were verified without altering the first file.
- Cancelling a slow transfer produced Cancelled status and removed its partial output.
  A truncated response exposed WebView2 retry behavior. The final implementation
  captured terminal state events, rejected duplicate active requests, and displayed
  Failed after the two-minute no-forward-progress safeguard. No current staging
  file remained for that failed transfer. Earlier forcibly stopped development runs
  left staging directories, consistent with the abrupt-exit limitation below.
- A real HTTPS Wry source archive from GitHub completed (2,303,056 bytes during this
  check). ZIP integrity passed, and Windows Zone.Identifier metadata was present
  after finalization. The archive was not executed. Open folder opened its destination
  in Explorer.
- Existing tab switching, separate addresses, native Back/Forward, search provider
  selection and address-bar DuckDuckGo search were exercised. Query-bearing search
  URLs were absent from the SQLite history.
- A final normal restart restored both tabs, their order, the active DuckDuckGo
  results page and the DuckDuckGo setting. Ctrl+J opened the empty downloads manager
  in the new session, as expected for session-only download records.
- With a separate library at unsupported schema version 99, the browser rendered
  its HTTPS home page and displayed an unavailable-library notice. Normal shutdown
  succeeded and the database remained at version 99.

GUI testing found and corrected Windows extended-path handling at the WebView2
download boundary, parent-window ownership for the destination dialog, transient
interruption handling, and stable progress-row rendering. The browser adapter keeps
native COM operations on the UI apartment; database and finalization work run off it.

Limits: flat folders; 200 results per query; 5000 bookmarks; 200 folders; latest 10,000
visits. History query matching has SQLite's default Unicode/case-folding limitations.
Date filters use UTC days while timestamps display locally. Titles/paths/bookmarks
and full session URLs can still contain sensitive data; storage is local plaintext.
Library history clearing does not clear native back/forward, cookies, cache, session
URLs or recovery backups. No private mode, encryption, history import/export, nested
folders or full compatibility/security audit was implemented.

Downloads: HTTP(S) only; 16 active transfers, one per source URL; 256 in-memory records.
Records and transfers are not restored after restart. No pause/resume, execution,
malware-scanner integration or certificate-policy bypass. Unknown lengths have no
percentage. Two minutes without forward progress cancels even a legitimately stalled
server. Closing/terminating during a transfer or failed finalization may leave staging
data. Native runtime security-policy behavior, mixed DPI, screen-reader operation and
long-running resource use were not comprehensively validated. Release packaging was
inspected but no tag/release was published.

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
- A startup focus issue found during GUI testing led to a shared shortcut map for
  both WebView2 controllers and the parent window, plus focus after toolbar readiness.
  Ctrl+T then worked immediately after restart without a preceding content click.
  Ctrl+W closed exactly one tab, and closing the final tab left one working Home tab.
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
