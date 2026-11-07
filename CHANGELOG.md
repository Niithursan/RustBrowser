# Changelog

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
