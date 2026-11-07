# Changelog

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
