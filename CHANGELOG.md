# Changelog

## Unreleased

- **Resize from every edge:** drag the left, right or bottom edge (or a bottom corner) of a restored window to resize it, as in other Windows apps. Before, only the top edge resized it, because the page covered the other edges. Windows supplies the invisible resize borders; a maximized or full-screen window is unchanged.
- **Bookmark menus:** right-click a bookmark or folder on the bookmarks bar or in a folder's dropdown (or Shift+F10 on it). A bookmark opens in a new tab, window or private window, and can be edited, deleted, copied, or have a bookmark or folder added beside it. A folder opens all its bookmarks in tabs, and can be renamed, deleted (after a confirmation; nothing inside is deleted) or have a bookmark or folder added inside it. New `open_saved_tabs`, `open_saved_window` and `open_saved_private` commands back it, and bar links now carry their folder.
- Fourth chrome critique follow-up (27/40):
  - **Address line:** while a page loads, the badge keeps its round shape and turns a quiet arc (still with reduced motion) instead of widening into a "Connecting" pill, so the address no longer jumps on every load.
  - **Tab gestures:** middle-click closes any tab, pinned ones included. Right-click a tab (or Shift+F10 / the Menu key on it) for Duplicate, Pin or Unpin, Move left or right, Close tab, Close other tabs and Reopen closed tab; the menu acts on that tab, not the selected one. New `pin_tab`, `duplicate_tab` and `close_other_tabs` commands back it.
  - **Menu:** Reopen closed tab joins This tab; Profiles… and Extensions… open Settings at those sections. Move tab left and right moved to the tab menu (their shortcuts are now in the shortcut list).
  - **Shield:** a plain outline without the check mark, so it no longer reads as a verdict on the site beside the lock.
  - **Tab strip:** the selected tab keeps at least 120px and is scrolled back into view when the scroll arrows appear; narrow sleeping tabs drop the moon; keyboard focus rings the whole tab card, clear of the signal rule.
- Third chrome critique follow-up (27/40):
  - **Tab strip:** tabs share the strip equally (up to 240px, shrinking together to 64px before it scrolls), so a page changing its title no longer moves the other tabs. After closing a tab with its close button, tabs keep their width until the pointer leaves the strip. Long titles fade out instead of ending in an ellipsis; a narrow inactive tab shows its close button on hover. Pinned tabs are icon-only cards without a close button. The selected tab's accent line stops short of the rounded corners, a hairline separates the strip from the toolbar, and the close button is a drawn icon.
  - **Tab list:** the all-tabs button is a down chevron next to +, away from the window's Minimize button.
  - **Address line:** an encrypted page shows a quiet lock without the word or filled pill, which are kept for Not encrypted, failures and browser pages. The field's border meets 3:1 contrast in every theme.
  - **Shield:** shown only once an extension that can block requests is added, and never in private windows. It reads in the text colour with a grey count instead of the accent colour, and its panel no longer says "WebView2".
  - **Bookmarks bar:** folder icons match site icons in size and colour, letter monograms are borderless, and All bookmarks has its own library icon behind a divider.
  - **Menu:** a New group comes first (New tab, New window, New private window, Reopen closed window); Move tab left and right follow Pin and Keep awake.
  - **Profiles:** the profile button appears once there is more than one profile. In a narrow window it and Menu stay at the right edge.
  - **Fix:** a tab whose page has no icon now shows the default globe from the start.
- Second chrome critique follow-up (27/40):
  - **Profile switching:** the profile dropdown marks profiles already open in another window ("Open in another window"; the action switches to that window) and Up and Down move between profiles.
  - **Settings honesty:** while Settings or another full panel covers the page, the toolbar reads "Browser page" and shows the panel's name, the star and shield are disabled, and Settings no longer reports on whichever tab was active.
  - **First run:** "Show bookmarks bar" explains that the bar appears once you bookmark a page.
  - **Plainer Settings:** one-sentence Clear browsing data introduction (and it says this profile's other windows stay open), "Saved tabs for next time", a plain private-window explanation, and the diagnostics record behind "Show recorded events".
  - **Consistency:** one focus rule for every dropdown (menus their first item, the star its field, the others their heading), `openSettings` scrolls when Settings is already open, the shortcut list includes Ctrl+Shift+B, a lighter address tooltip, Exit set apart in the menu, and disabled Delete buttons in Disabled Grey.
  - **Fix:** reopening a dropdown right after closing one no longer closes it again (the page focus from the first close was taken as a press on the page).
- Release metadata states the library database schema the build actually uses (5).
  The v0.21.0 metadata file (`rust-browser-v0.21.0-windows-x64-release.json`) says
  `profile_library: 3`, a value hard-coded since v0.15 that the producer and the
  verifier both still expected; nothing reads it apart from the verifier, and the
  v0.21.0 notes and INSTALL_WINDOWS.md describe the schema 5 migration correctly. A
  test now ties both scripts to `library::SCHEMA_VERSION`.

## 0.21.0 (2026-11-19, the first published release)

- Chrome critique follow-up (critique 27/40):
  - **Shield:** the shield no longer draws its check mark when no extension can block requests; off is an empty outline.
  - **Profile colours:** swatches are true 24px circles with a chosen ring that coexists with focus, and avatars and swatches keep their colour in Windows high contrast. The eight colours were retuned so none matches a theme's signal or error colour (Light's cobalt and crimson were reused before), with a test; profile lists saved with the old colours still load.
  - **Profile management:** rows explain why a profile cannot be deleted and disable Delete while it is open in another window. The profile dropdown focuses its heading, and its Add profile opens the editor instead of creating a profile at once.
  - **Settings:** a centred 760px column, sections separated by space (no doubled rule under the section bar), one-sentence explanations with details behind disclosures, plainer wording ("Reopen your tabs next time", "Each rule covers one site and one permission"), and Default browser and Diagnostics grouped under System.
  - **Copy:** Exit says what happens to tabs in the menu itself. The unclean-exit notice is in plain words. "Settings > Extensions" no longer breaks across lines, and interface spelling is US throughout.
- Profiles: separate profiles for different people or roles, each with its own
  sign-ins, cookies, history, bookmarks, passwords, extensions, tabs and settings,
  running in its own process and folder (`profiles\<id>` inside the existing profile
  folder, which stays the Default profile). A profile button left of the menu
  switches to or adds profiles; Settings > Profiles renames, recolours and deletes
  them (deleting asks first and refuses open profiles and Default). A "Who's
  browsing?" picker opens at launch when there is more than one profile; links from
  other apps open in the profile used last. New `--profile=<id>` launch option.
- Live page behind dropdowns: the menu, star, shield, site information, permission
  prompt and bookmarks-bar folders no longer freeze the page behind a still image.
  The page keeps playing and updating while a dropdown is open; the toolbar view is
  clipped to the chrome and the dropdown and raised above the page, and pressing the
  page closes the dropdown. The selected tab's title is no longer bold.
- Bookmark icons and flush window controls: bookmarks now persist validated raster
  `data:image/...` site icons in SQLite (schema 5), importing `ICON` data URLs from
  Firefox, Chrome and Edge HTML exports (and backfilling missing icons on existing
  bookmarks when re-importing), learning live favicons from WebView2 when bookmarking or
  visiting saved sites in normal windows, and rendering a letter monogram chip instead
  of a generic globe placeholder when no site icon is stored yet across the bookmarks
  bar, folder dropdowns and the Bookmarks panel. Browser windows now merge the tab strip
  with the window frame (`with_decorations(false).with_undecorated_shadow(true)`),
  removing the separate `Rust Browser 0.21.0` title bar and placing `46x36` Minimize,
  Maximize/Restore and Close controls flush with the top-right edge across horizontal
  tabs, vertical tabs and `New tab + library` while keeping Aero Snap, drag, top-edge
  resize, double-click maximize and the native system menu.
- Chrome harden, audit, layout, clarify and polish pass: Bookmarks, History and
  Downloads now open side by side with the new-tab page instead of staying hidden
  behind `toolbar`; narrow address bars collapse subdomains before the registrable
  domain tail and preserve `/` before query strings; the bookmark star fills in the
  theme's accent colour when the active page is saved; bookmark, folder and new-tab
  favorite forms validate inline with warning icons and `aria-invalid` while keeping
  typed input intact; `Menu > Keyboard shortcuts` expands at full readable height
  without flex crushing; dropdown panels declare `role="dialog"`, active permission
  prompts set `aria-modal="true"` with `Tab` focus containment and a single 2px
  signal border, bookmarks-bar folders support Arrow/Home/End keys, and history day
  groups use `h2` headings; `#chrome` carries a 1px bottom hairline at idle, Settings
  adds section dividers and disables `Sleep after` when tab sleeping is off, new-tab
  favorite cards condense into single-row tiles with quiet `×` remove buttons, `Exit`
  tooltips reflect private and restoration state, and `F7` copy clarifies `focus
  webpage`.
- Chrome polish pass: input placeholders use `--secondary` across all four themes for
  WCAG AA contrast; active permission prompts hide static connection details inside the
  site dropdown, disable Allow during the 500ms double-click guard, and count down only
  in the final 10 seconds; panel headers and the Settings section bar stay sticky with
  an `aria-current` active-section cue and a `68ch` prose cap; New tab (`Customize
  page`, `Add favorite`) and Bookmarks (`New bookmark`, `Folders`) collapse secondary
  forms until needed, and `Remove folder…` uses the confirmation dialog; pointer tab
  dragging highlights the insertion edge, `Keep tab awake` shows `Off`/`On`, overflowing
  bookmarks-bar items show `All bookmarks (+N)`, history date filters match local
  calendar days, Clear browsing data groups site data separately from saved credentials
  and sessions without echoing global notices, and error notices in normal windows offer
  a quick `Diagnostics` button.
- Polish: the tab scroll arrows show only while the tabs overflow the strip (the
  all-tabs list stays); in the tab column, + New tab lines up with the tab titles and
  the selected tab is marked by a short rounded bar instead of a rule that bent around
  the row's corners; a favourite's Remove is quieter than the favourite; the
  bookmarks bar keeps the 4px spacing scale; comments that still described the old
  menu sidebar are corrected.
- Library and Clear browsing data: history search filters as you type (the Search /
  refresh button is gone) and the dates sit behind Filter by date; visits are grouped
  by day and show the time without seconds; a bookmark's or visit's whole title opens
  it, with quiet New tab, Edit and Remove actions. The history description is in plain
  words. Clear browsing data opens with a short lead and puts its caveats behind "What
  clearing keeps and what it cannot do". Destructive buttons are in the error colour,
  and every confirmation (clear data, clear history, clear diagnostics, stop restoring
  tabs, revoke a permission, remove an extension) uses the browser's own dialog
  instead of Windows' plain box: it lists what will be cleared and what is kept,
  names the action on its button, and starts on Cancel.
- Notices under the toolbar and in Settings were always in the error colour, so
  "Extension added" looked like a failure, and long ones were cut to one line. A
  completed action or progress (an extension added, switched or removed, a window
  closed, bookmarks imported, data cleared without a failure) now shows in the text
  colour with an information icon; problems keep the error colour with a warning
  icon. Anything not marked as information stays an error. Notices wrap to two lines
  (the tooltip still has the full text), and the dismiss button is a drawn icon.
- Menu (⋮): its 19 items were one flat list. They are now labelled groups (This page,
  This tab, Library, Windows, then Settings, Keyboard shortcuts and Exit), each item
  shows its shortcut at the right, and zoom is one row (− 100% +) that shows the
  current percentage. Screen readers hear real menu items (Keep tab awake as a
  checkbox), and Up, Down, Home and End move through them; Tab still works.
- Settings: the panel that held every preference was called Privacy & security. It is
  now Settings, with links to its sections at the top: Appearance (theme, new-tab
  background, tabs down the side, bookmarks bar), Search (moved out of the menu),
  Background tabs, Privacy & security (this site's connection, history and sessions,
  passwords and forms, remembered permissions), Extensions, Clear browsing data,
  Default browser and Diagnostics. The menu item is Settings; the site-information
  dropdown's Privacy & security button and the shield's Manage extensions open their
  sections. The theme and background are no longer only on the new-tab page.
- Website permission requests open as a dropdown under the site-information badge,
  over the page that asked, instead of replacing the page with Privacy & security.
  The dropdown names the site and what it wants in plain words, shows how long is
  left before the request is denied, starts keyboard focus on Don't allow, and
  ignores Allow for its first half second (a double-click cannot allow by accident).
  Closing it in any way answers Don't allow for that request only. After an answer
  the page stays shown. Remembered rules in Privacy & security now read "Camera ·
  Allowed" instead of the internal names. An open dropdown now takes keyboard focus
  even when it opened without a click, so Escape reaches it.
- Dropdowns: the menu (⋮), the site-information badge, the star, the shield and each
  bookmarks-bar folder open a dropdown under their button instead of a sidebar, a
  full page, the library column or a panel that pushed the page down. While one is
  open the page behind it is a dimmed still image. The star (and Ctrl+D) saves the
  page and lets you rename it, move it or remove it from its dropdown; a folder lists
  its subfolders and bookmarks, and subfolders open in the same dropdown.
- The status line under the toolbar ("Ready") is gone. Loading shows in the progress
  bar and the reload/stop button, connection errors on WebView2's own error page, and
  failures are still announced to screen readers.
- Library query results gain `menu` (a folder's contents) and `page_bookmark`;
  `Filter` gains browser-only `menu_folder` and `page`.
- Bookmarks bar under the address line in normal windows (Settings > Appearance >
  Show bookmarks bar, on by default). It shows the bookmarks in a top-level
  Bookmarks bar / Bookmarks Toolbar / Favorites bar folder (where imports put the
  other browser's bar), then unfiled bookmarks, then folders; a folder opens a
  dropdown listing it. Imported bookmarks show without importing again.
  An import names a translated bar folder "Bookmarks bar". New optional settings
  field `bookmarks_bar`; library query results gain a `bar` list.
- Shield at the end of the address line. Its dropdown shows which of your extensions
  ask to block requests, which do not, and how many requests were blocked on the current page, read from WebView2's own
  network events. It does not say which extension blocked a request, does not count
  requests inside frames from other sites, and cannot turn blocking off for one site;
  the panel says so. Counting runs only in normal windows while an extension is added
  and Privacy & security > Extensions > Count blocked requests (on by default) is on;
  off removes the listener from every page.
- Bookmarks, history and downloads open in a library column at the right, beside the
  page, which stays usable (Ctrl+Shift+B, the menu, Ctrl+Shift+O, Ctrl+H, Ctrl+J).
  Drag its edge or use the arrow keys on it to resize it (260 to 640 pixels); its
  width lasts for the window and session. Privacy & security stays a full panel.
- Bookmark folders can contain subfolders. Import keeps the exported hierarchy,
  folder controls show the full path, and existing flat library databases migrate
  transactionally to schema 4.
- Pre-1.0 Rust library API change: `Action::SaveFolder` now accepts a parent ID, and
  bookmark-import `Entry.folder` is a full path (`Vec<String>`); downstream callers
  using these types must update.
- The native title bar and border follow the selected toolbar theme where Windows
  supports custom caption colors. Button hover and press feedback is briefly animated
  and disabled for reduced-motion preferences.
- Settings > Appearance > Show tabs down the side (off by default): the tab list moves
  to a column at the left of every open window, with the same actions, dragging,
  pinned tabs, keys (Up and Down too) and names for Narrator. Private windows follow
  it read-only.
- New-tab favourites are tiles with the site's icon, saved when you visit the
  favourite's page, or the first letter of the title. The new-tab page has an optional
  background (Plain, Dusk, Glow, Panels) drawn from the theme's own colours.
- A page in fullscreen keeps the toolbar but takes the room of the tab column and the
  library.
- New optional settings fields (`vertical_tabs`, `newtab_background`, `count_blocked`,
  a favourite's `icon`, an extension's `blocks`); 0.20 reads files that contain them.
  Profile path and session format 1 are unchanged; SQLite library schema 4 migrates
  existing data.
- Draft release notes in `release-notes/v0.21.0.md`; nothing is tagged or released.
- Toolbar polish from a design audit: the keyboard shortcuts move to Menu > Keyboard shortcuts and the address field's
  tooltip; reload and stop swap places while a page loads; the all-tabs list is an
  icon button; the pin, loading, sleep, warning and tab-scroll symbols are icons from
  the toolbar's own set; tab tooltips put the state on its own line; disabled
  controls change colour instead of only fading; buttons show a pressed state;
  spacing is on a 4px scale; "Private window" is no longer in capitals.
- The v0.20.0 release notes and RELEASING.md are kept as drafts: the first release is
  postponed until the owner decides.
- Going back to the start of a tab that began as the new-tab page (with Back or
  the mouse's back button) shows the new-tab page again instead of a blank
  `about:blank` page that kept the previous address; Forward returns to the page.

## 0.20.0 (not released; the first release was postponed)

- New layout: tabs are cards with the page's icon, title and close button, the
  selected card joined to the toolbar; + follows the last tab. The toolbar row has
  icon buttons for back, forward, reload or stop and home, the address line with a
  site-information badge and a bookmark star, an extension button, and a single
  Menu (⋮) holding every other action (it replaces More, and now also holds the
  search engine choice). The Go button is gone; Enter still opens the address. Icons
  are drawn for this project. The normal Windows title bar is kept.
- Browser extensions: Privacy & security > Extensions adds an unpacked extension
  folder you downloaded, lists your extensions, turns them on or off and removes
  them. uBlock Origin Lite works for ad blocking (see README). Extensions never run
  in private windows or the toolbar, and Rust Browser never downloads them.
- Fixed: since 0.18 the status line, the "Connecting" and "Loading" text and the
  window-title separator showed garbled characters instead of the middle dot,
  ellipsis and dash.
- Window titles and tab tooltips separate their parts with " - " (for example
  "Private - Example Domain"), and no user-facing text uses an em dash (tested).
- Fixed: the toolbar's own profile could keep form entries saved before 0.19, when
  WebView2 had autofill on in the address field. They are cleared once on upgrade;
  saved passwords and website form data are not touched.
- Release preparation: release notes for v0.20.0, RELEASING.md with the owner's
  release steps, and a release workflow that runs every CI suite and publishes only
  the reviewed notes. Builds remain unsigned.
- Installer upgrade tests now start from a real 0.19 installation. Profile path,
  session format 1 and SQLite schema 3 are unchanged; new settings fields are
  optional and older versions ignore them.

## 0.19.0 (unreleased)

- **Form autofill is now off by default.** WebView2 turned general autofill on by
  itself in content views and in the toolbar; it is now off until you turn it on.
- Privacy & security has two new switches, "Offer to save passwords" and "Fill in
  forms automatically", both off for new and existing profiles. They use WebView2's
  own password saving and autofill and apply at once to every open tab, and to new
  tabs, windows, popups and pages rebuilt after an engine restart. Private windows
  and the toolbar never save passwords or form data.
- Clear browsing data has two new choices, "Saved passwords" and "Autofill data",
  unchecked by default. Clearing history never removes passwords.
- Bookmarks can be imported from an exported bookmarks HTML file (Firefox, Chrome or
  Edge) with More > Bookmarks > Import from file. The file is read as untrusted
  text with strict limits; only http and https addresses are imported, duplicates
  are skipped, nested folders become top-level folders, and the import is all or
  nothing. Passwords cannot be imported (WebView2 offers no way to).
- Fixed: when saving every window would pass the 1 MiB session limit, all other
  windows were left out silently. Now the least recently focused windows are left
  out one at a time until it fits, and a notice names them once.
- Private vulnerability reporting is enabled: use the Security tab's "Report a
  vulnerability".
- Installer upgrade tests now start from a real 0.18 installation. Profile path,
  session format 1 and SQLite schema 3 are unchanged; the two new settings fields
  are optional and 0.18 ignores them.

## 0.18.0 (unreleased)

- Several normal windows on one profile: Ctrl+N, More > New window and a second
  launch with no link open a window on the new-tab page, up to 8. A ninth request
  shows a notice. Each window has its own tabs, toolbar, closed-tab list, taskbar
  and Alt+Tab entry; the profile, downloads, history and settings are shared.
- Links from other apps go to the most recently focused window, which comes to the
  front.
- Closing one window closes only that window; its pages are destroyed after the
  usual retirement delay. More > Reopen closed window brings back its tab
  addresses and pins until the browser exits. Closing the last window or
  More > Exit ends the browser.
- Every open window is saved and restored at the next start, also after a crash.
  Session format 1 keeps the focused window in its existing fields and adds an
  optional `other_windows` list (at most 8 windows, 64 tabs each), written only
  when more than one window is open. 0.17 reads the file but restores only the
  focused window; see INSTALL_WINDOWS.md before downgrading.
- An engine restart rebuilds every window; data clearing closes other windows and
  waits for every retired page; a toolbar failure is recovered in its own window.
  Only the selected tab of the focused window may show a permission prompt.
- Fixed before release: handling a focus change by moving focus into the page made
  several windows hand focus to each other forever, and restored windows took focus
  as they loaded, so the wrong window ended up in front; and More > Exit could save
  another window as the main one when Windows activated it during shutdown.
- Native suites cover routing, the limit, close, reopen, hand-off, engine restart,
  toolbar recovery, clearing with retired pages in two windows, and restore after a
  normal exit and after a killed process. Installer upgrade tests now start from a
  real 0.17 installation. Profile path and SQLite schema 3 are unchanged.

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
- Multiple normal windows were not included in 0.17; they arrived in 0.18.
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
