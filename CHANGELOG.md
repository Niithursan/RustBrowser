# Changelog

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
