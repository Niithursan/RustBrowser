# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The browser chrome (tab strip, toolbar, dropdowns, library column, new-tab page,
Settings) is one HTML document, `src/ui/toolbar.html`, rendered in its own
WebView2 view inside a native Windows window (winit). The "Who's browsing?" profile
picker is a second, small HTML document, `src/ui/picker.html`, in its own window.
Their design language is web; their behaviour must respect Windows (see Capabilities
and Constraints).

## Users

- **Now:** the owner, using Rust Browser as their everyday browser on Windows 11 x64
  in place of Firefox/Chrome. Daily use by the owner precedes the v0.21 reliability
  and polish pass and any tagged release.
- **Next:** members of the public who download a release. Design for the owner today,
  but nothing should depend on knowing how the project works inside. A first-time
  Windows user must be able to find and understand every control.

## Product Purpose

A secure, fast, lightweight, customisable desktop browser that can replace Firefox
or Chrome for everyday use. Success is the owner browsing with it daily without
falling back to another browser, then a v1.0 release candidate that others can
adopt. Reliability comes before adding features.

## Positioning

Why someone would choose it over Chrome, Edge or Firefox (confirmed by the owner):

1. **Privacy by default.** Risky features (form autofill, saved passwords) are off
   until the user turns them on; private windows save nothing; extensions never run
   in private windows; the toolbar never loads anything from a website.
2. **Light and fast.** Minimal chrome, efficient memory use, no unnecessary
   background activity, no unnecessary webview recreation.
3. **Customisable.** Five themes (Dark, Black, Slate, Plum, Light), tabs down the side,
   bookmarks bar, new-tab backgrounds (Plain, Dusk, Glow, Panels), favourites,
   resizable library column.

It is a Rust application shell on Microsoft WebView2, not an independent web engine.
Servo is a possible future engine (v0.6+ experiments), not a current capability.

## Operating Context

- Everyday browsing in normal and private windows; up to 8 normal windows per
  profile; it can be set as the Windows default browser.
- Profiles keep the owner's own roles apart (for example personal, work, school),
  each with its own sign-ins, cookies, history, bookmarks, passwords, extensions,
  tabs and settings. Switching between them is a frequent, quick action, not a
  shared-computer login: the profile button left of Menu (⋮), shown once there is more
  than one profile, opens another profile in its own window, and a plain launch with more than one profile shows the "Who's
  browsing?" picker. Links from other apps skip the picker and open in the profile
  used last.
- Every action is reachable by keyboard shortcut as well as by pointer; shortcuts are
  listed in Menu (⋮) → Keyboard shortcuts.
- The shield appears only once an extension that can block requests is added, since
  there is no built-in blocker; an encrypted page shows a quiet lock, and the
  connection badge uses words only when the state is worth reading.
- Dropdowns (Menu, site information, star, shield, bookmark folders, profile button,
  permission prompts) open over the live page: the page keeps playing and updating
  undimmed, and pressing the page closes the dropdown.
- Distributed as an unsigned per-user Windows installer and portable ZIP; requires
  the WebView2 Runtime and the VC++ redistributable.

## Capabilities and Constraints

- Windows 10/11 only (Windows 11 x64 is the priority target). Rendering is WebView2.
- The tab strip is merged with the window frame: there is no separate title bar, and
  Minimize, Maximize/Restore and Close sit flush in the top-right corner. Snap layouts,
  dragging, top-edge resize, double-click to maximise, the native system menu and
  Narrator must keep working. The window border follows the selected theme on
  Windows 11 build 22000+.
- Look: the browser has **its own look**, not Fluent/Edge imitation, but it must always
  respect Windows behaviour: Snap, window dragging and caption controls, Narrator,
  Windows high contrast, reduced motion and display scaling (checked at 100% and 150%).
- Each profile runs as its own process with its own folder; the existing data is the
  Default profile, which cannot be deleted. At most 16 profiles.
- All icons are drawn for this project; tab icons are WebView2-supplied image data
  checked by the browser.
- Web content is untrusted and stays isolated from privileged native functionality;
  the browser chrome must not weaken that boundary.
- Rust Browser blocks ads out of the box (v0.23, owner decision 2026-11-26): the
  official uBlock Origin is built in on Windows, in normal and private windows, and
  hBlock's domain list is downloaded on Linux. It is on by default and stays off
  once the user turns it off. Users can still add their own extensions.
- Terminology in use: Menu (⋮), site-information badge, star, shield, library column,
  Settings (with its Privacy & security and Profiles sections), tabs down the side,
  bookmarks bar, new tab, Don't allow / Allow, profile button, Default profile,
  "Who's browsing?".

## Brand Commitments

- Name: Rust Browser.
- Voice observed in the README and the interface: plain, specific, and clear about
  limits (e.g. what the shield's blocked-request count does not cover). The owner did
  not choose "honest about limits" as a reason to pick the browser, but existing copy
  follows it and changes must not overstate what the browser does.
- Never describe planned functionality as though it already exists.

## Evidence on Hand

- Interface source: `src/ui/toolbar.html` (includes the new-tab page) and
  `src/ui/picker.html` (the profile picker).
- Design screenshots and audits: `docs/hallmark/v0.21/` (before/after captures in light
  and dark at 100%/150%), `docs/hallmark/v0.21b/README.md`, `docs/screenshots/`.
- Accessibility evidence and gaps: `validation/ACCESSIBILITY.md`.
- No release has been published, so there are no users other than the owner, no
  testimonials, no download numbers and no benchmarks to quote beyond those recorded
  in `benchmarks/` and `validation/`. Do not invent any.

## Product Principles

1. Reliability over feature count: preserve what works; change in small, verified steps.
2. Privacy and security first: off by default when risky, nothing stored in private
   windows, web content never reaches native privileges.
3. Stay out of the page's way: browser chrome is light and quiet and leaves the
   website the most room.
4. Customisable without clutter: options exist, but the defaults must be right for
   someone who never opens settings.
5. Keyboard and assistive parity: every action works by keyboard and with Narrator,
   in every theme and in high contrast.

## Accessibility & Inclusion

Windows high contrast, reduced motion, screen-reader announcement of navigation
errors, items named in list actions, keyboard access to every action, and contrast
checked across all five themes. Known gaps are tracked in
`validation/ACCESSIBILITY.md`.
