# Changelog

## 0.17.0 (unreleased)

- Rust Browser can be chosen as the Windows default browser. Setup registers it for
  your user (http and https only, no elevation); a new Privacy & security button
  opens Windows Default apps on its page, because Windows asks the user to choose.
  The portable ZIP registers nothing. Uninstall removes only what Setup added.
- Links from other apps open in Rust Browser. The command line is parsed strictly:
  the registered form takes everything after `--single-argument` as one link, only
  explicit http and https links are accepted, and unknown options, invalid text and
  unsafe links are refused with a message. Fixed: an argument that was not valid
  Unicode crashed startup, and unknown options such as `--disable-web-security`
  were silently ignored.
- Starting Rust Browser while it runs now hands the link (or a new-tab request) to
  the running window instead of showing "already running". The channel is private
  to the user and Windows session, protected against impersonation and squatting,
  size and rate limited, and can only ask to open links. See SECURITY.md.
- The address field has Cut, Copy, Paste and Select all on right-click or the menu
  key. Paste inserts plain text only, on one line, at most 8,192 characters.
- Multiple normal windows are not included; a design and session-format proposal
  awaits approval (MULTI_WINDOW_PROPOSAL.md).
- Installer upgrade tests now start from a real 0.16 installation. Profile path,
  session format 1 and SQLite schema 3 are unchanged.

## 0.16.0 (unreleased)

- Add WebView2 find and print preview to the keyboard and More menu. Transfer
  focus to content before preview so it can receive keyboard input.
- Enable content zoom, show the live percentage/reset away from 100%, and add
  keyboard/menu controls. Keep zoom out of session format 1 and SQLite schema 3.
- Keep native browser tools disabled on the trusted toolbar and reject page
  operations on local browser panels. Preserve AltGr and the pin shortcut.
- Tag toolbar failure events by view, ignore retired views, and admit its terminal
  alert once per window. A reported hang gets a five-second local health grace
  and the existing bounded recovery; arbitrary hang detection is not established.
- Add native tool/boundary coverage and extend toolbar regressions, with eight
  compiled mutations rejected. Preserve the existing production checks.
- Record the owner's eight passes for the exact PR #21 acceptance build only.
  Keep static CRT opt-in and release-candidate status unapproved.
- Preserve the already merged #20 Actions pins and #4 base64 baseline without
  further dependency changes. Document why the hosted docs-only skip remains
  unverified on a PR containing code.

## 0.15.0 (unreleased, release candidate preparation)

- Add an opt-in static CRT investigation and direct/delay import inspection;
  production linking, runtime prerequisite checks and dependency versions stay unchanged.
- Prepare owner acceptance bundles with a separate installer identity, explicit
  disposable profile, executable hash verification and refusal of unsafe paths.
  Cover portable and installed startup/shutdown in CI without distributing test profiles.
- Cover cold process initialization before its image path is available; retain
  strict identity checks and the existing startup timeout.
- Separate technical qualification from tag/publication authorization. Prioritize
  Windows 11 x64 and review base64 #4 separately without changing the dependency.
- Recover the toolbar after its own WebView2 browser process or renderer ends.
  Previously pages could remain usable while tabs, address and menus stayed dead.
  Rebuild through the existing trusted toolbar builder, show a notice and limit
  recovery to three attempts within two minutes. No new web IPC capability.
- Repair installer-owned partial installations by recording ownership before copy,
  cleaning stale Setup temporaries and keeping the registered uninstaller after
  repair. Remove empty installer folders after uninstall. Two 32-case timing sweeps
  repaired all fresh/upgrade cases and preserved the disposable profile.
- Contain public-survey downloads inside the disposable profile; record download
  starts and permission prompts per visit. The 41-address survey and two nine-site
  burst-close runs found no engine exits, renderer exits, crash dumps or timeouts.
- Add native toolbar failure-recovery and keyboard/layout regressions. Add an
  optional foreground-guarded Windows UI Automation check. Emulation and UIA results
  do not establish screen-reader, physical IME or native mixed-DPI compatibility.
- Fix parallel diagnostic tests sharing a temporary directory.
- Classify the entire PR diff before CI. Only unpackaged Markdown-only changes
  may skip Windows jobs; ambiguity and packaged documents require all checks.
  Keep the stable aggregate and mandatory audit. Hosted skip behavior is not yet tested.
- Group scheduled Dependabot version updates monthly, limit one open PR per
  ecosystem and disable automatic rebasing. Existing dependency PRs remain owner decisions.
- Pin the installer upgrade fixture to verified v0.14 and retain cache isolation.
  Archive older checkpoints and add proposed release criteria and an isolated
  owner checklist. No release candidate is declared; no CRT/signing/updater or
  profile-format change is approved by these proposals.

## 0.14.0 (unreleased)

- Fixed a WebView2 browser-process crash (access violation) when a tab or the window
  was closed while some pages were visible, reproduced with the Microsoft sign-in
  page: 6 of 8 tab closes and 8 of 8 window exits before, 0 of 12 and 0 of 8 after.
  Closed tabs' views are now hidden at once and destroyed one second later; shutdown
  hides the window and exits after the same delay. Up to 64 closed views are held for
  the delay (initially 16): closing 17 or more tabs within one second, for example by
  holding Ctrl+W, or clearing browsing data with that many tabs open, destroyed the
  oldest early and crashed again (10 of 12 bursts with the sign-in page; 0 of 13 after).
- Added recovery when the WebView2 browser process ends for any reason: tabs are
  rebuilt and reloaded with their order, pins and selection. Previously every tab
  failed with `0x8007139F` until the browser was restarted. Restarts are limited to
  three within two minutes.
- Helper-process failures that WebView2 repairs itself (GPU, utility, frame) no
  longer mark a tab as failed. An unresponsive page shows a notice.
- HTTP 4xx/5xx pages are reported as the website's answer instead of "the page could
  not be loaded", and are no longer counted as navigation failures.
- An unreachable or unresolvable site keeps its real reason. WebView2 follows such a
  failure with a second navigation that aborts; that replaced the reason with
  "connection interrupted (status 9)". This was the open v0.12 `.invalid` finding.
- A navigation that becomes a download no longer marks the page as failed or adds a
  false navigation failure to local diagnostics. Navigation failures are written to
  diagnostics two seconds after they occur.
- A page link to `mailto:` or another non-HTTP(S) address shows a notice instead of
  marking the unchanged page as failed.
- Added Ctrl+Shift+T (reopen closed tab, in memory only), Ctrl+1–8/Ctrl+9, Ctrl+PageUp
  and Ctrl+PageDown, Ctrl+F4 and Alt+Home.
- Starting a second normal window for an open profile now says the browser is already
  running instead of reporting a startup failure.
- Added a manual public-website survey (`validation/site-survey.ps1`, with a
  `-BurstCloseFillers` option that closes many tabs at once), a native
  regression for all of the above, and a new local diagnostic event, `engine_restarted`.
- Installer upgrade tests now upgrade from the pinned v0.13 build. Repository-only
  links in packaged documentation are absolute and checked by the package test.
- Profile location, session format 1 and SQLite schema 3 are unchanged.

## 0.13.0 (unreleased)

- Added an unsigned per-user Inno Setup installer alongside the portable ZIP:
  Start Menu, optional desktop shortcut, HKCU Apps & Features and owned-file removal.
  No elevation request, runtime download, default-browser/protocol registration or updater.
- Preserved the existing profile path, session format 1 and SQLite schema 3. Installer
  refuses unowned/profile/reparse destinations, invalid versions, downgrades and running
  installed executables, including v0.12. Synthetic upgrade/uninstall/reinstall checks
  preserve sessions, pins, settings, library, permissions, opaque site data and downloads.
- Added source-owned multi-size globe branding in PE/window/taskbar/installer resources,
  a non-exclusive process-lifetime installer marker, and profile-free `--version` output.
- Added bounded unsigned release metadata and a non-executing manual integrity verifier.
  Signed update infrastructure remains blocked; matching hashes do not authenticate a publisher.
- Extended cached/parallel PR CI with actual pinned-v0.12 upgrade fixtures, installer tests,
  tamper/version/path failure checks and review bundles. Retained aggregate/audit/native
  gates, no duplicate feature pushes and manual publication approval/permissions.
- Documented install/upgrade/removal/recovery and signing limits. Clean-machine/Windows 10,
  mid-copy/power-loss recovery and cross-machine binary reproducibility remain unverified.

## 0.12.0 (unreleased)

- CI: pull requests, `main` and manual runs only (no duplicate feature-branch push
  runs); SHA-pinned dependency caching; parallel release build and packaging job;
  aggregate `CI result` check; pinned and cached cargo-audit 0.22.2 with a fresh
  advisory database. Measured warm CI wall time 144–202 s (previous median 458 s),
  dependency security under 30 s (previously 342 s). Cold runs cost more runner
  time. See CI.md.
- UI redesign ("graphite and signal"): the address line leads with a plain
  connection word (Encrypted, Not encrypted, Connecting, Not connected, Browser
  page) and shows the host in heavier Segoe UI Variable Display with the path
  muted; the overlay uses `URL.host`, so embedded credentials are never shown.
  Tabs are plain text with a signal-coloured rule under the active tab. Tab and
  library actions move into a More (⋯) disclosure, so the chrome is one row
  shorter. New palettes for Dark (graphite/yellow), Slate, Plum and Light
  (paper/cobalt); sliding loading bar (static with reduced motion); Windows
  high-contrast (forced-colors) styling; the "Normal" mode label and the new-tab
  panel title are visually hidden but kept for screen readers.
- Accessibility: assertive announcement of navigation failures; item-specific
  names for library/download/favorite/permission actions; focus moves to the
  heading of an opened panel; WCAG contrast test for all four themes; native
  checks of forced-colors and reduced-motion styling under media emulation. See
  validation/ACCESSIBILITY.md for the verified scope and manual gaps.
- Compatibility: controlled fixture now covers service workers, Cache Storage,
  structuredClone, AbortController and BroadcastChannel.
- Fixed false "connection interrupted" errors when a new address replaced a page
  whose navigation had not started yet: a superseded host load's late
  `NavigationStarting` no longer becomes the current navigation. Adds unit tests
  and a deterministic native same-turn regression.

## 0.11.0

- Intentionally advances production 0.8.0 to 0.11.0 after the v0.9/v0.10 research
  milestones. WebView2 remains the default; Servo and existing profile/storage
  formats are unchanged.
- Added bounded local application diagnostics with fixed categories/times/counts,
  asynchronous coalesced writes, inspect/confirmed-clear UI, corrupt-data
  preservation and best-effort unclean-exit notice. Private windows open no journal;
  no URL, arbitrary error payload, crash upload or remote telemetry is collected.
- Fixed Ctrl+Alt/AltGr interception and repeated native tab-action routing. Guarded
  navigation completion by native ID and distinguished explicit Stop/cancellation
  from stale completion and actual network/certificate failures.
- Fixed superseded blank-page aborts arriving before the replacement's starting
  event; focused unit/native/churn regressions preserve real connection failures.
- Fixed minimized-window tab activation reporting `E_INVALIDARG`: defer only focus
  until restoration, retaining visibility, resize and engine error handling.
- Fixed the intermittent hosted-CI loopback navigation failure in the test fixture
  server: Windows accepted sockets inherit non-blocking mode, so early reads failed
  with `WouldBlock` and dropped the request. Test-only; adds a regression test.
- Corrected benchmark process identity after reproducing PID reuse contamination;
  sampling/cleanup now validate creation times and export numeric fields only.
  The initial comparisons are withdrawn and replaced by an alternating-order
  corrected comparison (mixed results, no improvement claimed) and a clean
  30-minute synthetic churn soak.
- Added status-specific navigation recovery advice, Light chrome, stronger keyboard
  focus outlines, reduced-motion-aware loading progress and dismissible notices.
- Added controlled modern JavaScript/storage/SPA/cross-origin/TLS/native regression
  coverage and a daily-driver acceptance record separating automation from actual
  Windows GUI results and untested account/device/accessibility scenarios.
- Added Windows x64 version resources and an `asInvoker` manifest, GUI release startup
  feedback and static privacy-conscious panic/startup errors. Executables remain
  unsigned; no installer, service or automatic updater is installed.
- Added allowlisted portable ZIP/manifest/checksum packaging with metadata/path/profile
  safety tests, manual upgrade/removal instructions, and explicit manual GitHub Release
  publication after approved tagging. Tag pushes only create Actions package artifacts.
- Recorded matched baseline/current Windows resource/startup screening and a separate
  synthetic extended-stability procedure. These do not establish a general performance
  improvement or everyday-browser replacement readiness.

## v0.10 investigation

- Added fixed-target AppContainer/LPAC diagnostics for Winsock catalogs,
  services, system DLLs and privilege/resource boundaries, without extra
  capabilities or relaxed mitigations.
- Added explicitly scoped pipe handles, fixed numeric frames, protocol
  mutation tests, malformed/hung-worker rejection, live-IPC broker-crash
  recovery and measured resource limits in the experimental Windows job.
- Filtered administrative primary tokens before AppContainer creation;
  retained non-elevation assertions and verified the stronger launch on the
  hosted Windows runner after its initial fail-closed rejection.
- Documented the pinned WGL/offscreen rendering blocker. No real Servo
  renderer is contained; LPAC Winsock and general network policy remain open.
- Expanded key down/up/repeat and composition/cancellation fixtures, added
  a visible native-input observation panel, and recorded actual Windows
  input/resize/closure checks with explicit IME/DPI limitations.
- Recorded repeated synthetic worker/IPC and shared frame-callback samples.
  Kept WebView2, existing profiles/dependencies and package version 0.8.0.

## v0.9 investigation

- Documented the Windows Servo threat model, candidate broker/worker design,
  and blockers to permitting arbitrary content.
- Added synthetic AppContainer boundary and lifecycle probes with a separate
  Windows workflow and exact owned-profile recovery after broker termination.
- Added shared keyboard, pointer, composition capability, and device-scale
  fixtures with comparable script-to-frame timing endpoints.
- Added Servo IME event forwarding and unit regressions. Corrected converted
  wheel distances to use pixel units.
- Kept the production package at 0.8.0 while Servo containment remains unverified.

## 0.8.0

- Added an opt-in Windows Servo embedder with a pinned, separate source build.
- Rendered two local fixtures in one winit window with pointer, wheel, keyboard,
  resize, Back, Forward, and Reload support.
- Added fixture-only navigation and resource policy, disposable profiles, and
  checks for presentation, shutdown, and profile reuse refusal.
- Recorded Windows source-build costs, GUI validation, and the upstream sandbox
  and dependency limits. Normal browser tabs continue to use WebView2.

## 0.7.0

- Small engine-independent content navigation/event interface, used by the existing
  WebView2 adapter and covered by regular and native navigation regressions.
- Separate opt-in, digest-pinned official servoshell prototype with unique disposable
  profiles, controlled HTML/input/storage fixtures and a fixed HTTPS test target.
- Launcher/archive refusal and output-isolation tests plus loopback fixture tests in
  Windows CI, without downloading Servo or changing the production dependency graph.
- Official API/build/security research, repeatable source-build procedure, integration
  plan and measured capability/resource findings with explicit unavailable comparisons.
- WebView2 remains the only production engine; no Servo tab selector, browser profile
  migration, certificate bypass or native bridge is added to experimental content.

## 0.6.0

- Native linked popup tabs preserve opener relationships, profile/cookie sharing,
  `noopener`, and script-created tab closure. Runtime-marked non-user requests stay
  blocked; no external URI or certificate security boundary is relaxed.
- Conservative native upload/media/request protection, linked-popup safeguards,
  event-loop pause recovery and fullscreen integration with the origin toolbar kept.
- Actionable renderer-failure feedback and a native crash/reload regression.
- Interrupted first-save and corrupt-session recovery from validated temporary data;
  committed sessions take precedence and I/O errors do not reset profiles.
- Locked download staging leases and bounded empty-abandoned-folder cleanup when a
  destination is selected. Potential payloads and unrecognized/legacy data are kept.
- Process-kill/SQLite rollback, recovery and six native Windows regressions in CI.
- Controlled loopback upload/auth/media/WebRTC fixtures, an extended churn soak and
  repeatable interactive compatibility/accessibility procedures with explicit gaps.

## 0.5.0

- Best-effort native sleeping tabs, protected activity checks, Keep awake, configurable
  automation/delay and explicit resume failure/retry feedback; views are retained.
- Drag and keyboard tab reordering, pinned tabs, overflow selector/scroll controls,
  roving tab focus, selected/loading/sleep indicators and saved tab metadata.
- Local new-tab search, favorites, three themes, optional recent history and bookmark
  access, with ephemeral private settings and unchanged remote IPC restrictions.
- Deduplicate unchanged toolbar renders, window titles and session snapshots; preserve
  address drafts during background title updates and measure responsive chrome height.
- Native sleep/resume/race and local/private new-tab tests, added to Windows CI.
- Windows process-tree benchmark harness and documented v0.4/v0.5 measurements.

## 0.4.0

- Isolated private windows with verified WebView2 InPrivate mode, no history or
  session retention, and temporary profile cleanup on exit or later recovery.
- Origin-scoped camera, microphone, location and notification prompts, optional
  remembered decisions and revocation. Unknown and background requests stay denied.
- Confirmed browsing-data clearing for history, cookies/site storage, cache,
  sessions and permissions, with engine completion and partial-failure reporting.
- Separate normal session-retention preference and protection against stale writes.
- HTTP/HTTPS origin indicators and native blocking of external URI launches.
- Schema 3 permission storage and Windows native security regression checks.

## 0.3.0

- Bookmark manager: Ctrl+D, add/edit/remove, flat folders, search and open in current/new tab.
- Persistent searchable browsing history with titles/timestamps, date filters, duplicate
  coalescing, individual removal, confirmed clearing and a saved recording toggle.
- Query-string and credential URLs excluded from automatic history; fragments stripped.
- Bundled SQLite, background database ownership, transactional schema migrations and
  preservation of corrupt or newer-schema data.
- Native HTTP(S) downloads with destination folder picker, progress, terminal-state
  events, cancellation, collision-safe filenames and Open folder.
- Filename/path protections, atomic no-replace finalization, partial cleanup and a
  two-minute no-forward-progress safeguard; no automatic execution or application resume.
- Ctrl+H, Ctrl+J and Ctrl+Shift+O; existing tab/navigation/search shortcuts retained.
- Database, filesystem-safety and session-compatibility regression tests.

## 0.2.0

- Multiple independent WebView2 tabs with stable IDs and separate navigation state.
- Ctrl+T, Ctrl+W, Ctrl+Tab and Ctrl+Shift+Tab; clickable titles and close buttons.
- Google, DuckDuckGo and Bing search from the address bar with saved provider settings.
- Automatic restoration of tab URLs, order and active tab after restart.
- Bounded background session writes, atomic replacement, exclusive profile lock and
  preservation of invalid session files for recovery.
- Engine-independent tab/settings/storage modules and unit/integration regression tests.
- F7 now focuses the webpage; Ctrl+Tab switches tabs. The final tab closes to Home.

## 0.1.0

- Windows desktop window and isolated browser toolbar.
- Wry/WebView2 HTTP(S) rendering, address entry, back, forward and reload.
- Keyboard navigation, loading status and navigation error feedback.
- Separate engine adapter for future experimentation with Servo.
- Windows CI, dependency audit, Dependabot and version-tag release packaging.
