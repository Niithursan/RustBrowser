# Security

## Extension popups and fullscreen (after v0.21.0)

- **Extension popups.** Clicking the shield opens the extension's own toolbar page
  (`action.default_popup` or `browser_action.default_popup`) in a small view of the
  normal website profile, as other browsers do. The page is the extension's code with
  the extension's own rights; Rust Browser adds no rights to it. The path is read from
  the manifest the user added and accepted only as a plain relative `.html` path (no
  scheme, query, fragment, `..` or leading slash). The view may navigate only within
  `chrome-extension://<that id>/`, has no DevTools, downloads, permissions, clipboard
  or browser accelerators, and cannot open windows: an HTTP(S) address it asks to open
  becomes a new tab like any link; anything else is dropped. Its only message to the
  browser is its size (`size:<w>,<h>`, clamped to 25-800 by 25-600). It never opens
  in private windows.
- **The tab it acts on.** WebView2 treats every view as its own tab and window, so a
  script added before the popup's own replaces `chrome.tabs.query({active: true,
  currentWindow: true})` inside the popup with the tab showing the current page,
  matched by address and, for duplicates, by `performance.timeOrigin` read through the
  extension's own `chrome.scripting` (its own host access applies). An extension
  without access to the page sees no tab, as without `activeTab` in other browsers.
  Nothing else in the extension changes.
- **Fullscreen hides the toolbar.** HTML fullscreen now shows the page alone, as
  other browsers do, so the origin indicator is hidden while it lasts. To keep a page
  from passing itself off as the browser, every entry into fullscreen (including
  switching to a tab that is in fullscreen) names the page's host for four seconds
  in a notice drawn by the trusted toolbar view, clipped to the notice and raised
  above the page: "<host> is now full screen - Press Esc to exit". The host comes
  from the parsed page address, never from page content. Escape always reaches the
  page and leaves fullscreen; a dropdown or panel brings the toolbar back.

## v0.21 shield, side columns and new-tab tiles

- **Shield count.** While at least one extension is added, each content view in a
  normal window turns on WebView2's DevTools `Network` domain for itself only, with no
  response, resource or upload buffering (`maxTotalBufferSize`, `maxResourceBufferSize`
  and `maxPostDataSize` are 0). Two events are read: a failure with
  `net::ERR_BLOCKED_BY_CLIENT`, and a redirect from a website address into
  `chrome-extension://`. Addresses are compared and dropped; only a count per page is
  kept, in memory, and nothing is logged. No DevTools client is exposed to pages or
  to the toolbar. Turning off Settings > Extensions > Count blocked
  requests, or removing the last extension, removes the three event handlers from
  every page and turns the domain off. Private windows never turn it on. The measured cost is in VALIDATION.md.
- **What the shield claims.** Whether an extension "can block requests" comes from its
  manifest permissions (`declarativeNetRequest`, `declarativeNetRequestWithHostAccess`,
  `webRequestBlocking`), read from the folder the user chose (at most 1 MiB); asking
  for a permission is not proof that it blocks ads, and the panel does not say it is.
- **Side columns.** The tab column and the library column are trusted views built
  exactly like the toolbar: the same local document (the role is a URL fragment), the
  toolbar's separate profile and engine process, password saving and autofill off, no
  extensions, navigation limited to that exact document, the same content security
  policy and the same command bridge. They never overlap the page or the toolbar. A
  failed column is rebuilt at most three times in two minutes, then the window falls
  back to the tab strip or closes the library. A test checks the profile, engine,
  form settings, refused navigation and recovery.
- **Private windows and vertical tabs.** A private window reads one value from the
  normal profile's session file when it opens, read-only: whether tabs go down the
  side. Nothing else is read (favourites, history, theme and the rest stay default),
  the switch is refused in a private window, and private windows never write the
  normal session; a test compares the file byte for byte.
- **Favourite icons.** A favourite keeps its site's icon as a `data:image/png;base64`
  URL of at most 8 KiB, taken from the same checked PNG bytes as tab icons. It is
  checked when saved, when settings load (a bad one makes the settings invalid like
  any other bad favourite) and again by the page before it is shown. No icon is ever
  fetched by the toolbar. New-tab backgrounds are CSS drawn from the theme tokens; no
  image is stored or downloaded.

## v0.21 profiles

- Each profile is a separate process with its own folder, session lock, WebView2
  environments and hand-off endpoint; no cookies, storage, passwords, extensions or
  library data are shared. Private windows of a profile stay InPrivate and temporary.
- Profile ids name folders, so only `default` and `p<n>` (1 to 6 digits, no leading
  zero) are accepted, from the command line (`--profile=<id>`, one token, never from
  a link), from the toolbar and from the list file. Names are bounded text and are
  only ever set as text in the toolbar and picker; the picker's data is JSON with
  `<` escaped.
- `profiles.json` is size-limited and validated on read; a damaged list is never
  replaced, and the Default profile still opens. Changes are atomic under a lock.
- Deleting a profile needs a confirmation in the toolbar, is refused for Default,
  for the window's own profile and while the profile is open (its session lock),
  and only ever removes `profiles/<id>` for a validated id.

## v0.21 dropdowns over the page

- Dropdowns no longer capture the page. While one is open over a webpage, the trusted
  toolbar view is raised above the page and clipped to the chrome and the dropdown;
  the page stays a separate, unprivileged view and only receives input outside that
  clip. The dropdown's position comes from the toolbar document, and the browser
  ignores it unless a dropdown is open and every value is finite and positive.
  (Earlier v0.21 builds showed an in-memory JPEG still of the page instead.)

## v0.20 extensions, layout and the first release

### Browser extensions

- **Extensions run with broad access.** An extension the user adds can read and
  change every page it matches in normal windows, including form contents and
  passwords as they are typed, make network requests and keep its own storage.
  Rust Browser does not restrict or review what an extension does; WebView2 enforces
  the permissions in the extension's manifest. Users should add only extensions they
  trust, from the extension's official release.
- **Only the normal website profile has extensions.** WebView2's extension support is
  turned on only for normal website views. Private windows use separate temporary
  profiles with it off, and refuse every extension command. The toolbar has its own
  profile, never has extension support turned on and never runs the user's
  extensions (tested: the toolbar profile lists only WebView2's two built-in
  Microsoft extensions).
- **Nothing is downloaded.** Rust Browser never downloads, bundles, updates or
  installs an extension on its own. The user picks an unpacked folder in a Windows
  folder dialog; the browser checks that it holds a valid `manifest.json` (version 2
  or 3, at most 1 MiB) and passes the folder to WebView2's `AddBrowserExtension`.
- **What is active is visible.** Settings > Extensions lists every extension the user
  added and whether it is on; the extension button in the toolbar shows how many are
  on and their names. WebView2 keeps the installed and on or off state in the profile
  across restarts (tested in two processes). The settings only record which extension
  IDs the user added, so WebView2's built-in extensions are never listed or removed.
- **Web content and toolbar messages cannot add extensions.** Adding happens only
  after the native folder dialog. Toolbar commands can turn on, turn off or remove
  only extension IDs the user added (checked against the saved list and the ID
  format).
- **Not available:** extension toolbar buttons, pop-ups, options pages and
  `chrome-extension:` pages (navigation to them stays blocked), so an extension runs
  with its default settings.

### Layout and the toolbar policy

- Icons are inline SVG in the toolbar's own markup. Tab icons are read from WebView2
  by the browser as PNG bytes, checked to be PNG and at most 32 KiB, and passed to
  the toolbar as `data:` URLs. The toolbar's content security policy gains only
  `img-src data:`; it still loads nothing from the network and runs only its own
  inline code. A test pins the exact policy.

### First release

- v0.20.0 was prepared as the first published release, but the owner postponed the
  first release (2026-11-12); nothing is published yet. Builds remain **unsigned**; Windows
  SmartScreen warns about an unknown publisher. The release workflow publishes only
  when the owner runs it by hand with the publish input, from an existing tag on
  `main`, with the exact five checked files and the reviewed release notes. See
  RELEASING.md and packaging/windows/UPDATE_SECURITY.md.

## v0.19 passwords, autofill and imported bookmark files

### Password saving and autofill boundary

- Passwords and form data are saved only by WebView2's own features, and only after
  the user turns on "Offer to save passwords" or "Fill in forms automatically". Both
  are off for new and existing profiles. WebView2 157 turns general autofill on by
  default and applies these settings per view (a view created later starts from the
  defaults again), so Rust Browser sets both on every content view as it is created
  and on every open view when a switch changes.
- Rust Browser never reads, decrypts, lists, exports or imports saved passwords or
  form data, and has no code that could. WebView2 stores them in the profile's
  `content` folder, protected by Windows for the user account; anyone who can sign
  in as that user, or run software as them, may be able to reach them.
- Private windows never save passwords or form data: the setting is forced off when
  each private view is created, independently of the switches, and the switches are
  refused in private windows. The toolbar view always has both off, so the address
  field never offers autofill.
- Web content cannot change these settings; they are set only from the toolbar's
  Settings panel through the existing toolbar command bridge.
- Clearing uses WebView2's `PASSWORD_AUTOSAVE` and `GENERAL_AUTOFILL` kinds only
  when those separate choices are checked; clearing history never includes them.
  WebView2 cannot remove one saved password, so clearing removes all of them.
- Password import is not offered: the WebView2 API in use has no import interface
  (the `webview2-com-sys` 0.39.1 bindings define 266 `ICoreWebView2*` interfaces,
  none with an import member; the only password members are the autosave switches
  and HTTP basic authentication responses).

### Imported bookmark files

| Threat | Protection | Tests |
| --- | --- | --- |
| A crafted file runs script or loads content | The file is scanned as plain text by a small parser; it is never rendered, so nothing in it runs, loads, resolves entities from outside or follows paths. Only each link's `HREF` (exact attribute name, so `ICON_URI` or `FEEDURL` are never used) and link and folder text are read | Fixture and hostile-input tests in `tests/bookmark_import.rs` |
| Dangerous or misleading addresses | Only explicit `http://` and `https://` addresses that saved bookmarks accept; `javascript:` (also when written with character references), `data:`, `file:`, `place:`, `vbscript:` and addresses with a user name or password are refused and counted | `hostile_addresses_are_refused`, Edge fixture |
| Resource exhaustion | 10 MiB file limit checked before reading it all, at most 20,000 links and 32 folder levels (the whole file is refused beyond these), titles and folder names shortened, addresses over 8,192 characters refused, invalid UTF-8 refused, linear-time scanning | `size_count_and_depth_limits`, malformed-file tests |
| Partial or corrupting import | One SQLite transaction: a failure part way, a refused file or passing the 5,000 bookmark limit leaves bookmarks and folders unchanged; duplicates are skipped | Interrupted-import and limit tests |
| Reading another browser's data | Only the file the user picks in a Windows file dialog is read; no other browser's profile folder or database is opened | Code review; no such code exists |
| Web or toolbar content injecting bookmarks | The import action cannot be deserialized from toolbar messages; it is built only from the chosen file. Private windows refuse imports | `the_toolbar_cannot_send_import_entries`, private-window suite |

## Reporting a vulnerability

Report suspected vulnerabilities privately on GitHub: open the repository's
**Security** tab and choose **Report a vulnerability**. See the end of this file for
what not to include.

## v0.18 multiple windows

- All normal windows share one profile, engine environment, permission rules and
  hand-off server; nothing new is exposed to web content. Toolbar IPC is tagged
  with its own window, so a toolbar can act only on the window that holds it.
- Only the selected tab of the focused window may show a permission prompt. A
  change of tab or of focused window cancels pending requests, and closing a
  window cancels its tabs' requests and popups.
- The prompt is part of the trusted toolbar, drawn as a dropdown under the
  site-information badge over a still image of the page, so page content cannot
  cover or imitate it. It fails closed: only Allow grants, and Allow is ignored for
  the first 500 ms so a double-click in the page cannot grant by accident. Closing
  the dropdown in any way, or letting the minute pass, answers Don't allow.
- Every `other_windows` entry in `session.json` is validated like the main window
  (at most 8 windows and 64 tabs each, same address rules). An invalid entry makes
  the whole file invalid; it is backed up as `session-invalid-*.json` as before.
  The saved file stays within the existing 1 MiB limit by dropping other windows
  rather than tabs of the focused window.
- Clearing browsing data closes every other window first and waits until no page
  of a closed tab or window remains. Reopening a closed window is in memory only
  and is emptied by clearing.
- Private windows are unchanged: they never join the normal window list, are never
  saved and do not receive links.

## v0.17 links from other apps and single-instance hand-off

### Command-line links

Windows and other apps pass links on the command line. Every argument is
untrusted. The raw command line is parsed by one function (`src/launch.rs`):

- The registered form is `rust-browser.exe --single-argument %1`. Everything
  after the marker is one link, taken verbatim, so quotes, spaces or dashes in a
  link cannot become options or further links.
- Other forms accept only `--private`, `--new-window`, `--version` and `--`. Any
  other option, text that is not valid UTF-16, more than 16 links or a command
  line over 32,768 characters refuses the whole launch (exit code 2, with a
  message) before any window or profile is opened.
- A link must be an explicit `http://` or `https://` address with a valid host
  and no user name or password. Nothing becomes a search and no host is guessed.
  `file:`, `javascript:`, `data:` and app schemes are refused. Local `.htm` and
  `.html` files are not accepted, because the address bar refuses `file:` too.
- Links from other apps always open in a normal window.

### Hand-off channel

When a normal window already holds the profile, a later launch sends its links,
or a new-window request, to that window and exits. The profile lock is an
operating-system file lock, released when its holder exits for any reason, so a
crash cannot leave a stale lock; the next launch then takes over the profile.

| Threat | Protection | Test |
| --- | --- | --- |
| Remote client | `PIPE_REJECT_REMOTE_CLIENTS` | Flag only; no remote machine here |
| Another Windows user connects | Protected DACL with one entry: this user's SID. The server also requires the client process to be this user in this Windows session | DACL inspected in `the_endpoint_allows_only_this_user`; user rule unit-tested. A second account could not be created here |
| Another program owns the name first (squatting) | The first instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE`; if the name is taken the server does not start and the window says so. A new instance is created before each client is served, so the name never lapses | `an_endpoint_owned_by_anything_else_is_not_used` (same name, and a PowerShell squatter) |
| Fake server receives links | The client opens the pipe at identification level (the server cannot impersonate it) and requires the server process to be this user, in this session, running this same executable | `a_fake_server_is_not_sent_the_request` (PowerShell server, same user); mutant without the executable check caught |
| Malformed, wrong version, oversized or hostile message | Fixed format: magic, version 1, kind, at most 16 links of at most 8,192 bytes, no trailing bytes. Every link is checked again with the command-line policy. Oversized messages are drained and refused | `malformed_messages_are_refused_before_dispatch` and library tests |
| Flood | At most 8 accepted requests per 10 seconds and 4 clients at once; the rest are told the browser is busy | `a_flood_is_rate_limited` |
| Hung or closing browser | The UI thread answers; after 5 seconds the client is told the browser is not responding. A closing window stops listening; a later launch waits up to 6 seconds and then takes over the profile or explains | `a_hung_browser_is_reported_not_accepted`, two-process suite |
| Channel abuse | It carries only "open these links" or "open a new window". It never runs commands, reads files or returns data: the only reply is one status byte (accepted, refused, busy, closing, not responding) | Format tests |

A process that runs as the same user in the same session can already do
anything the user can, including starting the browser with a link, so it is
outside this boundary. The second launch grants the running browser permission to
take the foreground (`AllowSetForegroundWindow`); no focus-stealing workaround is
used.

### Hand-off channel on Linux

Linux uses a Unix socket (`linux_handoff`) with the same message format, limits,
rate limit, timeouts, link checks and status replies as Windows. A private window
never listens and never hands off: each runs in its own process.

| Threat | Protection | Test |
| --- | --- | --- |
| Another user connects, or places a socket a launch connects to | The socket is in a directory checked before every use: a real directory owned by this user with no group or other access (`$XDG_RUNTIME_DIR`, or else a `handoff` folder made private in the profile). The socket is mode 0600 | `only_a_private_directory_holds_the_socket`, mode checked in `a_request_reaches_the_browser_and_its_answer_comes_back`. A second account could not be created here |
| Fake server receives links | A launch connects only to a socket owned by this user; anything else in its place is reported, and nothing is sent | `a_socket_left_by_a_crash_is_replaced_and_anything_else_is_not` |
| Stale socket after a crash | The browser binds only while holding the profile lock, so a socket already there is left over and is replaced; anything that is not a socket is never removed | Same test; checked in a live crash and relaunch |
| Malformed or hostile message, flood | The shared decoder and rate limit; a message is read for at most 2 seconds in all and never beyond the largest valid size | `malformed_and_too_many_requests_are_refused_without_reaching_the_browser` |
| Exit blocked by the channel | Stopping the server never waits on a socket that was removed meanwhile | `the_browser_can_stop_listening_even_after_its_socket_was_removed` |

The client does not check the server's process, as Windows does: the file
ownership and permission checks above stand in for it. As on Windows, a process
running as the same user is outside this boundary.

### Default browser on Linux

Make default browser (Settings > System) runs only when the user presses it. It
writes one launcher, `rust-browser.desktop`, to the user's applications folder
(`$XDG_DATA_HOME/applications` or `~/.local/share/applications`), whole and then
renamed. It then sets it through GIO as the user's default for `http` and
`https` links in `~/.config/mimeapps.list`. It claims no file types, needs no
administrator rights and changes nothing for other users. A program path with
characters the launcher's command line would have to escape (quotes,
backslashes, `$`, `%`, backticks or control characters) is refused rather than
escaped (`paths_a_command_line_would_need_to_escape_are_refused`). Links from
other apps arrive as command-line arguments and pass the same checks and
hand-off as any other launch.

### Saved passwords and form data on Linux

WebKitGTK has no password manager, so Linux has its own, behind the same
Settings switches as Windows ("Offer to save passwords", "Fill in forms
automatically"; both off by default, never in private windows). Logins and
contact details are kept in the desktop keyring (the Secret Service: GNOME
Keyring, KWallet and others, through libsecret), which encrypts them and unlocks
them with the user's login. Rust Browser writes no cryptography of its own and
keeps nothing in the profile's files.

| Threat | Protection | Test |
| --- | --- | --- |
| A page forges a login to save, or calls the fill | The form helper (`linux_forms.js`) runs in its own WebKit script world in the top frame. Page scripts run in another world and see neither its message handler nor its fill functions | Checked in the container: a probe page saw `handler=hidden fill=undefined` |
| A fill reaches the wrong site | Every message names its origin, which must be the view's own. Logins are kept per origin (`https://host[:port]`, or HTTP on loopback only) and listed only for that origin. A fill goes to the tab only if it still shows that origin | `passwords_are_kept_only_for_secure_or_local_sites`, `only_bounded_well_formed_messages_are_read` |
| A page script reads a filled password | Nothing is filled on page load. A login or contact details are filled only when the user chooses them from the key button in the address field. Values reach the page as call arguments, never as script text | GUI check in the container (fill after choosing) |
| Saving more than the user meant | A submitted login is only offered (Save, Never for this site, Not now). Its password stays in memory until it is answered, its tab closes or five minutes pass, and it never reaches the toolbar page. Contact details are saved only from a submitted form, only for known fields (name, email, phone, organization, address), never passwords or card fields | `contact_details_merge_known_fields_only`, `the_script_reports_only_what_the_browser_reads` |
| Profiles see each other's logins | Every keyring item carries a digest of its profile's folder, and every search names it | `profiles_have_separate_items` |
| Visiting a page prompts for the keyring | Listing usernames and checking for contact details never unlock the keyring. Only saving, filling and a submitted login's check do | Design |

Clear browsing data removes this profile's saved passwords or contact details
from the keyring (checked in the container with `secret-tool`). Without a
running keyring nothing is saved, and the browser says so when asked to save or
fill.

### Shield on Linux

WebKitGTK cannot run extensions. On Linux, Add from folder reads only an
extension's declarative blocking rules (`declarativeNetRequest` rulesets, as
uBlock Origin Lite ships them). `content_rules` translates them into WebKit
content-blocker rules, and WebKit compiles and applies them itself, before a
request leaves the browser. None of the extension's code ever runs: no
scripts, pages, buttons or settings.

| Threat | Protection | Test |
| --- | --- | --- |
| Hostile or damaged extension folder | The manifest is at most 1 MiB and must name a Manifest V2 or V3 extension. Rulesets must be relative `.json` files inside the folder with no parent steps or backslashes. There are at most 100 rulesets, 32 MiB each and 128 MiB in all. A localized name is read only from `_locales/<default_locale>/messages.json` with a checked locale name. Anything else refuses the folder | `only_enabled_rulesets_inside_the_folder_are_read`, `a_localized_name_is_read_from_the_default_locale`, `an_extension_folder_is_read_and_translated` |
| A rule blocks more than the extension would (a broken site) | A rule WebKit cannot express exactly is left out, never widened: redirects, header changes, regular expressions, request methods, excluded request domains, unknown resource types or conditions, and an exception list with a domain WebKit cannot take. Only lists that narrow a rule drop entries WebKit cannot take. A top-level visit is never blocked by a rule that does not ask for it, and an allow never lets a blocked top document load | `what_webkit_cannot_express_is_skipped_not_widened`, `blocks_after_a_top_document_block_are_not_merged`, `domain_lists_narrow_safely_and_drop_covered_subdomains` |
| Filter text escapes its pattern | Every filter character that is not a letter, digit or `/` is escaped, and non-ASCII or control characters refuse the rule | `filters_become_webkit_patterns` |
| Web content changes the rules | Only Settings > Extensions in a normal window adds, turns on or off, or removes an extension. The toolbar commands name only saved extension IDs (32 letters `a` to `p`, derived from the folder's path as Chromium does), which are also the only identifiers given to WebKit's store | `ids_match_chromium_and_are_the_only_identifiers` |
| Memory safety at the C boundary | `linux_shield` calls WebKit's content-filter API through `webkit2gtk::ffi`, on the UI thread only. Each asynchronous call's callback reclaims its boxed closure exactly once, and every filter reference is counted and released (`Filter` clones take a reference, and `Drop` releases one) | `webkit_compiles_translated_rules` (compiles, refuses a non-rule list, loads again) |

Private windows never use the filters. The compiled filter is kept in the
profile's `content-filters` folder, about 60 MB for uBlock Origin Lite's default
lists (117,031 WebKit rules from 17,811 of its rules, prepared in about 6
seconds on WebKit's own thread). It is deleted when the extension is removed or
the profile is deleted. WebKit reports nothing about what it blocked, so Linux
shows no count. A page that starts loading at startup before the kept filter
has loaded may load unfiltered; in the container checks the first page was
already filtered.

## v0.16 content tools and recovery

Find, zoom and print target a visible website content controller. They are refused
on the local new-tab/library/settings panels. Native browser accelerators, zoom
and context menus are disabled on the trusted toolbar; custom shortcuts route to
content. No privileged IPC, host object, permission grant or TLS bypass is added
to websites. Printing uses WebView2 browser preview with normal user confirmation,
without a shell, silent output or elevation. Print/PDF output remains outside
private-profile cleanup and can contain sensitive page data.

Toolbar failure and health-response events identify their source and probe so
retired views cannot replace a healthy toolbar or cancel its current health check.
A reported hang gets a five-second grace and shares the existing restart budget;
the native watchdog did not report the deliberately hung fixture within 30 seconds.
This is bounded recovery for a reported condition, not universal hang detection.
The terminal native alert is admitted once per window. Tests and limitations:
[v0.16 validation](https://github.com/Niithursan/RustBrowser/blob/main/validation/V016.md).

Profile formats, private-window isolation, dynamic production CRT, permission and
installer safeguards are retained. Owner acceptance from an older exact build does
not qualify this modified build or authorize an RC, tag or publication.

## Post-v0.15 candidate investigation

Static CRT linking is an opt-in validation experiment, not the production default.
Direct/delay import checks do not prove every dynamically loaded module or WebView2
dependency works on a missing-runtime host. Bundled runtime security fixes require
an application rebuild; no security improvement is claimed from static linking.
No TLS, permission, private-profile, IPC or installer safeguard is relaxed.

Owner bundles contain unsigned validation artifacts, not authenticated releases.
The launcher checks the expected executable hash, installed identity and disposable
profile path; it does not authenticate a modified bundle or its publisher. Review
the source/commit and hashes through the PR. Do not open its installation shortcuts
directly, because they do not select the test profile. Foreign profiles, reparse
paths and production destinations are refused; test data is never deleted by the
launcher. Never share credentials, profile contents or private meeting links.

Qualification, owner acceptance and publication authorization are separate gates.
No RC, tag, signing identity, updater or publication is approved by automated success.
See the [investigation](https://github.com/Niithursan/RustBrowser/blob/main/validation/STATIC_CRT.md)
and [acceptance baseline](https://github.com/Niithursan/RustBrowser/blob/main/validation/RELEASE_CRITERIA.md).

## v0.14 reliability changes and their boundaries

- Engine restart reuses the same profile, private-window isolation, permission
  handler, download handler and navigation policy as ordinary tab creation. Pending
  permission prompts and popup requests are cancelled, never auto-granted. If the
  engine ends before startup security initialization finished, that initialization
  is run again before any website loads.
- A closed tab's document stays alive, hidden, for one second. It can no longer be
  shown or focused, and events from it are ignored. Clearing browsing data waits
  until every such view is destroyed before asking WebView2 to clear.
- "Reopen closed tab" keeps up to 16 addresses in memory for the running window. They
  are never written to the session file and are discarded when browsing data is
  cleared. Private windows keep theirs in memory until the window closes.
- HTTP error pages are no longer described as failed connections. Certificate
  failures are unchanged: they remain failures, and no validation bypass exists.
- The public-site survey and the test that ends the engine process are compiled only
  into the test executable. The survey enters no credentials and grants no permission.
- No new native interface is exposed to websites, no telemetry is added, and the
  installer remains unsigned: a matching SHA-256 checksum detects corruption but does
  not identify the publisher.

At v0.14, toolbar engine recovery was not addressed; v0.15 adds it. Still unresolved:
the root cause of the WebView2 crash, which is inside the engine and is avoided, not fixed; and
single-instance hand-off, which would need a new local interface between processes.

## v0.13 installation and manual updates

The new unsigned Inno installer uses a fixed per-user target, exact payload and
owned-file uninstall, separate from existing profiles/downloads. It requests no
elevation and never downloads runtimes, registers protocols/defaults, auto-launches
updates or installs a service. Profile/reparse/ownership/version/file-use checks
fail closed; they do not defeat same-account malware, hard links or path races.
The running marker is non-exclusive and session-scoped; normal/private multi-instance
semantics remain. No privileged web IPC or new Servo capability is introduced.

The manual verifier checks bounded schema/versions/names/lengths/hashes and writes
no user data. Co-hosted checksums and unsigned metadata provide no independent
authenticity. No signing root/certificate is configured, so automatic update checks,
downloads and execution remain absent. A verified signed compiler does not sign
its generated Rust Browser packages. See [threat model and signing prerequisites](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/UPDATE_SECURITY.md)
and [user recovery instructions](INSTALL_WINDOWS.md). Mid-copy/power-loss recovery,
clean-machine prerequisites and signed distribution remain unverified.

## v0.11 production hardening

WebView2 remains the sole production engine. No Servo sandbox integration, browsing
allowlist expansion or profile sharing is introduced. Existing default-deny website
permissions, linked-popup boundaries, native external-protocol blocking, certificate
validation and separate trusted UI context remain in place. The new controlled TLS
test observes rejection of an untrusted self-signed certificate; it installs no root,
changes no certificate policy and adds no production certificate handler.

Only the exact generated toolbar document receives Rust Browser's command bridge.
Remote pages, workers, redirects and popup content receive no application IPC. The
modern native suite rechecks this boundary alongside cross-origin local storage.
Explicit Stop is distinguished from genuine connection/certificate failure; a status
9 abort is cancellation only for the same native navigation ID the host stopped.
HTTPS is connection encryption, never a claim of site trustworthiness.

Local diagnostic records accept only a fixed enum, timestamps and counts. No API
accepts URL/title/path/error strings, tokens, cookies, credentials, page/form contents
or request bodies. Private windows create no diagnostic files/worker. The normal
profile journal is bounded (128 entries/32 KiB, plus a same-sized temp), asynchronously
coalesced and never uploaded. Times/counts still reveal app usage; it is plaintext
under the Windows account's profile protections, not encrypted or anonymized data.
Inspect/confirmed clear is separate from browsing-history deletion and does not remove
bookmarks/cookies/sessions/downloads. New events may follow clearing. Corrupt/unknown
files are preserved until explicitly reset. Local reparse/ordinary-file checks do
not defend against malware running as the same account replacing files mid-operation.

Unclean-exit detection is best effort. Journal writes/marker cleanup are not a single
transaction with all engine/session/SQLite shutdown work, and missing markers do not
prove every flush succeeded. Early startup failures may have no profile/journal.
Release startup and panic feedback is static; no payload/path/backtrace or automatic
crash report is uploaded. It does not replace Runtime/OS crash reporting or restore
unsaved forms. See [architecture](https://github.com/Niithursan/RustBrowser/blob/main/ARCHITECTURE.md) and [acceptance limits](https://github.com/Niithursan/RustBrowser/blob/main/validation/DAILY_DRIVER.md).

The unsigned executable embeds version metadata and `asInvoker`, with `uiAccess=false`.
It requests no administrator access but inherits its launcher's rights; this is not
a sandbox and users should not launch elevated. Portable packages use a fixed file
allowlist and SHA-256 integrity manifest. Hashes do not authenticate the publisher.
No installer/service/updater is added; signing, authenticated updates and clean-machine
upgrade/rollback verification remain unresolved. GitHub Release publication now needs
an explicitly approved manual workflow action; this task creates no tag or release.

The development tests' fresh TLS keys, profiles, raw logs and downloaded fixture data
remain outside Git. Do not include them in issues or package artifacts. New production
features justify the version jump; the default data folder and database formats are
preserved. Older binaries may reject Light settings; use closed-profile backups for
rollback rather than editing a live SQLite database or deleting locks.

The v0.10 [rendering decision](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/SECURE_RENDERING_V010.md)
keeps the Servo host fixture-only. The synthetic diagnostic workers retain
Win32k and child-process restrictions, zero capabilities and verified token
limits. Launch first derives a restricted LUA primary token with maximum
privileges disabled and Administrators deny-only, then applies the lowbox
attributes using CreateProcessAsUserW. Both filtered and worker tokens must
be non-elevated. Failure stops launch without an original-token fallback.
They receive exactly two anonymous pipe endpoints through an
explicit handle list. The protocol accepts fixed numeric reports only and
grants no filesystem, network or browser operation. A run identifier and
fixture-origin number are not website authentication. Worker-reported
metadata must never authorize privileges.

Malformed frames, worker hangs, allocation limits and live-IPC broker
termination have controlled tests. LPAC still fails Winsock startup; its
inability to query catalog keys is evidence for investigation, not a proven
unique cause. No network timeout/nonreceipt establishes general containment.
OpenGL initialization fails under retained Win32k lockdown, and the pinned
offscreen/software paths still depend on WGL. No real Servo renderer was
launched under these restrictions. See [diagnostics](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/isolation/DIAGNOSTICS_V010.md).
The separate upstream lockfile audit and strict minimal-feature Clippy
remain failing as recorded in [validation](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/VALIDATION_V010.md).
Production audit and strict Clippy pass independently.

The v0.9 [threat model](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/isolation/THREAT_MODEL.md) keeps
arbitrary Servo browsing blocked. A separate synthetic worker validates
selected AppContainer and Job Object mechanisms. Its standard AppContainer
probe observed inconclusive loopback timeouts, and its capability-free LPAC
probe failed Winsock initialization. These results leave Servo containment
unverified. The tests create their own package identities and modify ACLs
only on newly staged synthetic files. They include journaled recovery for
their exact profile after broker termination. An AppContainer profile
can remain after a failed cleanup, which is reported.

The shared event fixture has no native bridge or network callbacks. Both
hosts accept only a bounded numeric report from their fixed document.
WebView2's test harness allows its exact generated HTML navigation. The
Servo host retains its two-file navigation and resource allowlist.
Simulated raster/device scales and dispatched DOM composition events
do not validate Windows input services, monitor changes, or physical
input latency. Servo's pinned DOM composition constructor is disabled
by default and remains unavailable in that fixture.

This is an experimental browser. Report suspected vulnerabilities privately on GitHub: open the repository's **Security** tab and choose **Report a vulnerability** (https://github.com/Niithursan/RustBrowser/security/advisories/new). Only you and the maintainer see the report. Do not report vulnerabilities in public issues, and do not include cookies, passwords, browsing profiles or personal URLs in any report.

The browser uses the installed Microsoft WebView2 Runtime. Keep that runtime and Windows updated. Remote content receives no application IPC handler; the local toolbar has a separate profile and restricted navigation. HTTP traffic is unencrypted. HTTPS does not establish trustworthiness, and certificate validation is never bypassed.

The v0.7 Servo spike is a separately launched official servoshell package, not a
production engine. Its pinned source explicitly lacks Windows OS sandbox support.
A disposable profile separates data but does not restrict the process's OS access.
Use controlled content only, without real accounts/files/device permissions. No
normal browser or Servo profile is reused. Run directories can retain cookies,
authentication metadata and SQLite data even with `--temporary-storage`; the launcher
does not claim erasure or private-mode guarantees and never deletes arbitrary data.

Preparation verifies a fixed official archive SHA-256 before extraction; launch also
checks the fixed executable digest. The adjacent native DLLs/resources must remain
from that verified extraction. No TLS bypass, arbitrary command-line flags, devtools,
WebDriver, user scripts or Rust IPC bridge is enabled. The static loopback fixture
has no host/file API or network callbacks. The small shared navigation interface
does not grant privileged capabilities to content. See
[Servo security findings](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/RESEARCH.md). Cargo audit covers the
application's locked Rust dependencies, **not** the downloaded Servo distribution
or all of its native libraries; a green application audit is not Servo certification.

The v0.8 source embedder runs as a separate executable with its own fresh marked
run directory, temporary storage, and a generated profile. Servo's content
runs in the embedder process with the pinned single-process defaults. The
pinned revision has no supported Windows content-process sandbox. Treat its
process as running with the current user's access. Supply only the two
built-in fixture pages and keep real data out of the experiment. Five profile
files remained after each verified shutdown. The run directory remains
available for inspection.

The host allows navigation to the two exact generated file URLs, with optional
fragments. The resource delegates allow only GET for those main documents. The
fixtures restrict subresources with CSP. The host denies permissions and devtools
requests, refuses new views, and supplies no external launcher, native file picker,
download destination, or Rust Browser IPC. Download refusal relies on the fixed
fixture and navigation/resource policy. The pinned API exposes no dedicated
download decision hook in this host. The experiment has no TLS bypass and permits
only the local fixture URLs. See [Servo results](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/EMBEDDER_RESULTS.md)
for the separate upstream dependency audit and the limits of these checks.

Session URLs and provider settings are saved
in plain text within the user's local profile, protected by OS account permissions.
URLs may contain sensitive query parameters or tokens. Session files and backups
must never enter Git. Restored URLs are validated and bounded before loading. Only
HTTP(S) website navigation is allowed, with explicit URL credentials and native external
URI launches rejected. The app uses an internal blank page during clearing. Restoring
normal tabs makes network requests to those sites unless session retention is disabled.
Hidden tabs share storage within their window and can continue background activity.

Linked popup tabs use the exact opener environment/profile and native same-origin
and `noopener` behavior. The host only closes script-created popup tabs in response
to native window-close requests. Popup decisions use the Runtime's user-initiation
flag, validated HTTP(S)/blank URLs, bounded deferrals and the existing tab limit.
Opener access is standard browser access, not access to the Rust bridge. Some identity
providers reject embedded browsers; no user-agent spoofing, cookie-policy bypass,
external-protocol launch or certificate exception is installed for authentication.

Native activity observation reads request method/context classifications only; it
does not inspect payloads or record request URLs. Conservative protection can persist
for a view's lifetime after uploads/media. Unsupported observation disables automatic
sleeping for the view. Native heuristics and Keep awake are still required for
unobservable activity, including arbitrary calls, streams and generated media.
Fullscreen hides the toolbar (see above). Renderer recovery is user initiated
and may lose unsaved page state; browser-process failures can require a restart.

Session recovery validates temporary snapshots before promotion and preserves
corrupt originals using newly reserved backup names. A valid committed snapshot,
including its retention preference, always takes precedence over stale temporary
data. I/O failures are not treated as corruption or permission to reset data.
Download recovery is limited to an explicitly selected folder and recognized unlocked
ownership leases. It removes only empty directories/lease metadata, never payloads,
live transfers, reparse points or legacy unmarked staging folders. No download path
registry is added, including for private downloads. This is logical cleanup, not
secure erasure; downloaded files and abandoned payloads remain untrusted.

Eligible background views can use WebView2's optional native suspension APIs. Sleeping
does not isolate, clear or destroy a view, profile, cookies or page memory. The engine
may refuse suspension. The app protects active/pinned/Keep awake/loading views, audio,
granted permissions, downloads, pending permission requests and clearing operations;
asynchronous results are checked against stable IDs and generations. Native runtime
heuristics are additionally relied upon for operations the app cannot observe. No API
fully identifies arbitrary important JavaScript work; disable automatic sleeping or
use Keep awake for critical live sites. No security setting is weakened for performance.

The bundled new-tab page runs inside the separate trusted toolbar view. Its internal
marker is translated to a neutral content view, never enabled as a navigable remote
scheme. Favorites accept validated HTTP(S) URLs, titles use text nodes, theme values
are an enum, and remote content still has no Rust IPC. Recent history is opt-in and
unavailable privately. Normal favorites and saved tab titles can be sensitive and are
stored as plaintext settings; private new-tab preferences remain in memory only.
Turning off session URL retention removes tab titles/pins/order metadata, while
explicitly saved favorites and visual/performance preferences remain.

Private windows use separate processes and temporary profiles with verified WebView2
InPrivate mode. They open no persistent application session store and never queue
history visits. Memory-only decisions and temporary runtime data are scoped to each
window. Directory cleanup is best effort, including after crashes. This does not
provide anonymity, secure erasure, or protection from websites, observers, Windows
or other local software. Downloaded files and metadata persist outside the profile.

Permission prompts identify the engine-supplied origin and kind. A request remains
denied until explicitly allowed; only supported active-tab requests can be granted.
Remembered rules are scoped to an exact origin, capability and profile. New decisions
do not use WebView2's native persistence. Pending deferrals are denied on tab change,
change of focused window, navigation, shutdown or timeout. Revocation and data clearing close tabs and report
native completion or failure; an unconfirmed permission clear suppresses grants
until restart. Device access remains subject to Windows and WebView2 policy.

The app stores bookmarks, history and permission rules in `library.sqlite` using parameterized
queries. Automatic history excludes query strings and credentials and strips fragments;
titles/paths may still be sensitive. Recording can be paused, and clear-history requires
confirmation. The history manager's clear action is separate from the privacy panel's
category-based clearing of engine data, sessions and permissions.
The database, session data and recovery backups are not encrypted. Keep the profile
private and outside version control; recovery files can retain deleted historical data.

Downloads accept HTTP(S) sources and require destination selection. Server filenames
are sanitized, Windows device/stream syntax is neutralized, and no-replace finalization
retains existing files. Staging stays on the selected volume, preserving file metadata
on completion. Runtime policy failures are not bypassed. The app never opens or executes
a downloaded file; the download manager can only open a validated directory in Explorer.
Download records are not persisted. A same-user local attacker or a compromised OS is
outside this application's filesystem threat boundary. This is not a malware scanner.

Dependency security runs on pushes to main, pull requests and a weekly schedule. Dependabot proposes Cargo and GitHub Actions updates; they still require review and passing checks. The release executable is unsigned. This project has not undergone an independent security review.

`.cargo/audit.toml` records three advisory exceptions for crates absent from the supported Windows dependency graph: proc-macro-error, ttf-parser and glib. They remain in Cargo.lock because Cargo resolves other platforms too. Reassess these exceptions before adding non-Windows support or changing the desktop dependencies. All other audit warnings remain build failures.
