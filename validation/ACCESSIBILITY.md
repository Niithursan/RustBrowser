# Accessibility validation (v0.12)

This records what is verified for the trusted browser chrome (toolbar, tabs and
library/settings panels). **It is not a WCAG conformance claim.** Website content
accessibility is provided by WebView2 and is outside this scope.

Environment: Windows 11 Pro 10.0.26200 x64, WebView2 157.0.4341.42, Rust 1.99.1,
100% display scaling, single monitor.

## Automated evidence

| Behaviour | Test | Result |
| --- | --- | --- |
| Text, secondary and error colours reach 4.5:1 on every chrome surface; accent focus rings and the active-tab bar reach 3:1 (WCAG 1.4.3 / 1.4.11 ratios) in Dark, Slate, Plum and Light | `ui::tests::every_theme_meets_wcag_contrast_for_text_and_focus_indicators` parses the shipped CSS tokens | Pass. Lowest text ratio 6.57:1 (Slate secondary), lowest accent ratio 5.56:1 (Slate) |
| Navigation failures are announced through an assertive `role="alert"` region, once per new failure | Native daily fixture, untrusted-TLS phase | Pass, mutation-checked |
| Active tab has a non-colour cue (inset accent bar plus bold label) | Native daily fixture, 640 px window | Pass, mutation-checked |
| Chrome does not overflow horizontally at 640 px with 150% CSS zoom (text-scaling proxy) | Native daily fixture compares the chrome's own scroll and client widths | Pass, mutation-checked with a zoom-only overflow (an earlier version compared mismatched coordinate spaces and could not fail; corrected) |
| Windows high contrast: active tab keeps a 2 px solid outline | Native daily fixture emulates `forced-colors: active` via DevTools `Emulation.setEmulatedMedia` | Pass, mutation-checked |
| Reduced motion: loading bar animation stops while loading is shown | Same emulation with `prefers-reduced-motion: reduce` | Pass, mutation-checked |
| Opening a library/settings panel moves focus to its heading | Native daily fixture | Pass, mutation-checked |
| Row actions have item-specific accessible names (for example "Remove *title*") | Native daily fixture renders a synthetic bookmark row | Pass |
| Tabs expose `tablist`/`tab`, `aria-selected`, position/size, roving `tabindex`, and arrow/Home/End movement | Existing markup; native tab and shortcut fixtures | Pass (existing) |
| Address focus selects text; drafts survive background title updates | Native daily fixture | Pass (existing) |
| Loading progress is hidden from assistive technology when idle | Native daily fixture | Pass (existing) |

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
