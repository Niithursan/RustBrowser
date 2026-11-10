# Accessibility validation

This records what is verified for the trusted browser chrome (toolbar, tabs and
library/settings panels). **It is not a WCAG conformance claim.** Website content
accessibility is provided by WebView2 and is outside this scope.

Environment: Windows 11 Pro 10.0.26200 x64, WebView2 157.0.4341.42, Rust 1.99.1,
100% display scaling, single monitor.

## Automated evidence

| Behaviour | Test | Result |
| --- | --- | --- |
| Text, secondary and error colours reach 4.5:1 on every chrome surface; accent focus rings and the active-tab bar reach 3:1 (WCAG 1.4.3 / 1.4.11 ratios) in Dark, Slate, Plum and Light | `ui::tests::every_theme_meets_wcag_contrast_for_text_and_focus_indicators` parses the shipped CSS tokens | Pass. Lowest text ratio 5.84:1 (Slate secondary), lowest accent ratio 6.31:1 (Slate) |
| Navigation failures are announced through an assertive `role="alert"` region, once per new failure | Native daily fixture, untrusted-TLS phase | Pass, mutation-checked |
| Active tab has a non-colour cue (inset accent bar plus bold label) | Native daily fixture, 640 px window | Pass, mutation-checked |
| Chrome does not overflow horizontally at 640 px with 150% CSS zoom (text-scaling proxy) | Native daily fixture compares the chrome's own scroll and client widths | Pass, mutation-checked with a zoom-only overflow (an earlier version compared mismatched coordinate spaces and could not fail; corrected) |
| Windows high contrast: active tab keeps a 2 px solid outline | Native daily fixture emulates `forced-colors: active` via DevTools `Emulation.setEmulatedMedia` | Pass, mutation-checked |
| Reduced motion: loading bar animation stops while loading is shown | Same emulation with `prefers-reduced-motion: reduce` | Pass, mutation-checked |
| The More (⋯) disclosure toggles `aria-expanded` and the tab/library actions row | Native daily fixture | Pass, mutation-checked |
| The address overlay emphasises the real host only (credentials never shown; screen readers read the real input) | Native daily fixture with `https://user:pw@example.test/path?q=1` | Pass, mutation-checked |
| Opening a library/settings panel moves focus to its heading | Native daily fixture | Pass, mutation-checked |
| Row actions have item-specific accessible names (for example "Remove *title*") | Native daily fixture renders a synthetic bookmark row | Pass |
| Tabs expose `tablist`/`tab`, `aria-selected`, position/size, roving `tabindex`, and arrow/Home/End movement | Existing markup; native tab and shortcut fixtures | Pass (existing) |
| Address focus selects text; drafts survive background title updates | Native daily fixture | Pass (existing) |
| Loading progress is hidden from assistive technology when idle | Native daily fixture | Pass (existing) |

## v0.14 additions

Automated, in `native_failure_reporting_tab_retirement_and_engine_restart`:

- An HTTP error page (404) is exposed as polite status text: the assertive alert
  region stays empty and the status line has no error styling.
- The engine-restart notice appears in the polite `role="status"` notice region with
  its Dismiss button visible, and does not raise the assertive alert.
- Unit tests cover the new shortcuts (Ctrl+Shift+T, Ctrl+1–9, Ctrl+PageUp/PageDown,
  Ctrl+F4, Alt+Home), that plain digits, Home and F4 still reach the page, and that
  AltGr+digit is not intercepted.

Nothing in the "manual verification still required" list below was tested in
v0.14: no screen reader, real contrast theme, Windows text-size setting, display
scale other than 100%, second monitor, IME or non-US physical layout was used. No
WCAG conformance or screen-reader compatibility is claimed.

Known new limitation: Ctrl+1–9 are browser shortcuts in every layout. On layouts
where digits need Shift (for example French AZERTY) the unshifted digit keys still
report the same virtual keys, so the shortcuts should work, but this was not tried
on such a keyboard.

## v0.15 additions

Recorded local runs on Windows 11 x64 / WebView2 157.0.4341.42:

| Check | Evidence | Scope |
| --- | --- | --- |
| Keyboard reachability, More disclosure and tab arrows | `native_toolbar_keyboard_reachability_and_scaled_layout`, three passing runs; 28 expected controls, 29 reached by Tab or Shift+Tab | DevTools-injected keys in the production toolbar, not physical keyboard input |
| Responsive layout at DPR 1, 1.25, 1.5 and 2 | CSS widths 1920, 1536, 1280 and 960; 14 layout controls checked per configuration; no reported layout problems | Renderer/device-metric emulation; no Windows display-scale or monitor transition |
| Regression strength | Four product mutations each caused a test failure: untabbable Reload, mouse-only More, ignored left arrow, oversized layout | All mutants compiled; failures were assertion failures, not build errors |
| Windows accessible names and roles | Optional `validation/accessibility-uia.ps1`: 24 controls initially, 33 with More open, no problems | UI Automation metadata, not a spoken screen reader |
| Windows keyboard walk | Three foreground-guarded UIA/input runs reached 29 distinct controls | Automated Windows key injection; stops if foreground is unavailable; excludes tab close buttons and the panel heading |

The native regression is included in CI and release gates. The foreground-dependent
UIA script is manual and is not a CI gate. These recorded runs do not complete the
[owner checklist](OWNER_CHECKLIST.md). Narrator, real contrast themes, physical IME,
non-US keyboard layouts and native mixed-DPI transitions remain unverified.

## Emulated, not physically verified

- **Windows high contrast:** the `forced-colors: active` styles (system-colour focus
  rings, outlined and underlined active tab, system-coloured progress, `GrayText`
  disabled controls) are checked only under DevTools media emulation, and only the
  active-tab outline is asserted. A real Windows contrast theme was not applied.
- **Reduced motion:** asserted under emulation. The Windows "Show animations"
  setting was not toggled.

## Manual verification still required

These have **not** been tested and must not be reported as working:

- Narrator, NVDA or JAWS reading order, announcements and tab semantics.
- Real Windows contrast themes and the Windows "Text size" accessibility setting
  (the 150% CSS zoom check is only a proxy).
- Physical keyboard layouts (AltGr/IME composition) beyond the existing unit routing tests.
- Display scaling other than 100%, and mixed-DPI multi-monitor moves.
- Keyboard-only use of every settings and library control end to end.
- Focus return to the page after closing a panel in every path (F7 and Escape are documented).

## Known limitations

- Tab close buttons are skipped by Tab navigation (`tabindex=-1`); keyboard users
  close tabs with Ctrl+W.
- Toolbar glyph buttons rely on `aria-label`. Their visible symbols (← → ↻ × ⌂) have no text labels.
- WebView2's built-in error page (for example "Hmmm… can't reach this page") is not
  styled or controlled by Rust Browser.
