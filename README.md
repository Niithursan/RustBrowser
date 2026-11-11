# Rust Browser 0.20.0

A Windows browser with tabs and private windows, using Rust, **winit 0.30** and
**Wry 0.57 / Microsoft WebView2**. Websites render in the installed Chromium-based
WebView2 Runtime. This is an early application shell, not an independent Rust web
engine or a daily-use replacement for Firefox/Chrome yet.

v0.19 can save passwords and fill in forms using WebView2's own features. Both
are off until you turn them on in More (⋯) → Privacy & security, and private
windows never save either. **Form autofill is now off by default**: WebView2
turned it on by itself before this version. Bookmarks can be imported from an
exported bookmarks HTML file (Firefox, Chrome or Edge). Passwords cannot be
imported. See [Passwords and autofill](#passwords-and-autofill) and
[Importing bookmarks](#importing-bookmarks). This is not a release candidate.

v0.18 opens several normal windows on one profile: Ctrl+N or More (⋯) → New
window, up to 8 at a time. Links from other apps go to the window you used last,
which comes to the front. Closing a window closes only that window; the browser
exits when the last one closes, and every open window returns at the next start,
also after a crash. Private windows are unchanged. See
[Windows](#windows) below and the
[design record](https://github.com/Niithursan/RustBrowser/blob/main/MULTI_WINDOW_PROPOSAL.md).
This is not a release candidate.

v0.17 can be chosen as the Windows default browser: links from other apps open in
it, and starting it again while it runs hands the link to the open window instead
of showing "already running". The address field has Cut, Copy, Paste and Select
all.

v0.16 adds native find, per-tab zoom with a live percentage/reset, and print
preview through keyboard shortcuts and More. Toolbar recovery ignores stale
view events, limits its terminal alert to once per window and gives a reported
hang a bounded health check before recovery. It does not detect every hang.
See [v0.16 evidence and limits](https://github.com/Niithursan/RustBrowser/blob/main/validation/V016.md).
This is not a release candidate.

v0.15 prepares release criteria; it does not declare a release candidate. The
toolbar now recovers if its separate WebView2 process or renderer fails, and Setup
can repair the tested interrupted installations. Native keyboard/layout coverage,
public-site survey containment and CI minute controls are extended. Profile path,
session format 1, SQLite schema 3 and the WebView2 production engine stay unchanged.
See the [readiness record](https://github.com/Niithursan/RustBrowser/blob/main/validation/RELEASE_READINESS.md),
[proposed criteria](https://github.com/Niithursan/RustBrowser/blob/main/validation/RELEASE_CRITERIA.md)
and [owner checklist](https://github.com/Niithursan/RustBrowser/blob/main/validation/OWNER_CHECKLIST.md).

Post-merge qualification prioritizes Windows 11 x64 and investigates an opt-in
[static CRT build](https://github.com/Niithursan/RustBrowser/blob/main/validation/STATIC_CRT.md).
Production linking and prerequisite checks remain unchanged. Owner acceptance uses
a validation-identity installer plus a hash-checked disposable-profile launcher;
the normal review installer and its shortcuts are not isolated acceptance launchers.
Technical qualification does not authorize declaring, tagging or publishing an RC.
Windows 10 validation and human acceptance checks remain outstanding.

v0.14 is a reliability release found by browsing real websites through the
production session. Closing a tab or the window no longer crashes the WebView2
engine on pages that triggered it (reproduced with the Microsoft sign-in page), and
if the engine process ends for any reason the open tabs are rebuilt and reloaded
instead of the browser becoming unusable. Server error pages (404, 429, 500),
unreachable sites, downloads and blocked app links now report what actually
happened. Ctrl+Shift+T, Ctrl+1–9, Ctrl+PageUp/PageDown, Ctrl+F4 and Alt+Home were
added. Evidence, measurements and limits: [VALIDATION.md](VALIDATION.md) and the
[website compatibility record](https://github.com/Niithursan/RustBrowser/blob/main/validation/COMPATIBILITY.md).

v0.13 adds an unsigned **per-user Windows installer**, Start Menu and optional
desktop shortcuts, Apps & Features removal, upgrade/profile-preservation checks,
application icons and a manual artifact verifier. Portable ZIPs remain supported.
No automatic updater, runtime installation or default-browser/protocol registration
is added. Follow [installation and recovery](INSTALL_WINDOWS.md) and
[update authentication limits](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/UPDATE_SECURITY.md). `--version`
prints the version without opening a profile. This work publishes no release.

v0.12 redesigns the toolbar: the address line shows a plain connection word and
the site's host emphasised, tabs are text with a coloured rule under the active
one, and tab and library actions sit behind the More (⋯) button (also reachable by
keyboard shortcuts). It adds Windows high-contrast support, announces
navigation errors to screen readers, names list actions by their item, extends the
controlled compatibility suite to service workers and Cache Storage, fixes false
errors after rapid successive navigations, and makes CI faster
([CI.md](https://github.com/Niithursan/RustBrowser/blob/main/CI.md)). Accessibility evidence and gaps: [ACCESSIBILITY.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/ACCESSIBILITY.md).

v0.11 adds bounded local application diagnostics, improved navigation/Stop
feedback, AltGr-safe shortcuts, a light theme and a controlled modern-site
regression suite. The version intentionally jumps from 0.8.0 because v0.9
and v0.10 were research milestones. Existing profile locations and storage
schemas remain unchanged. See [Windows installation and upgrades](INSTALL_WINDOWS.md)
and [the daily-driver acceptance record](https://github.com/Niithursan/RustBrowser/blob/main/validation/DAILY_DRIVER.md).

## Setup on Windows

1. Use Windows 10/11, preferably x64.
2. Install [Rust stable with rustup](https://rustup.rs/). Choose the default
   `x86_64-pc-windows-msvc` toolchain.
3. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with **Desktop development with C++**, MSVC and a Windows SDK.
4. Install the [Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
   if it is missing. Microsoft Edge being installed is not a substitute for checking
   the Runtime. Use a current Evergreen Runtime: v0.4 requires native profile,
   permission and external-scheme controls. This project does not bundle or install it.
5. Restart your terminal after installing tools. Internet access is needed for
   Cargo's first dependency download and for browsing.

Open this folder in VS Code or PowerShell:

```powershell
cargo build --locked
cargo run --locked
# Optional initial website:
cargo run --locked -- https://www.rust-lang.org/
```

For an optimized executable:

```powershell
cargo build --release --locked
.\target\release\rust-browser.exe
```

**Where builds come from.** Official builds are published only on this
repository's [Releases page](https://github.com/Niithursan/RustBrowser/releases);
none has been published yet. Executables and packages attached to CI runs are
unsigned test builds for review, not releases. Every build is unsigned, so Windows
shows an unknown-publisher warning. Do not install Rust Browser from anywhere else.

**Reporting a vulnerability.** Report it privately: on GitHub open this
repository's **Security** tab and choose **Report a vulnerability**. Please do not
use public issues for security problems. See SECURITY.md.

CI stores an x64 Windows executable as a build artifact and tests portable
packaging. Approved version tags produce package/checksum Actions artifacts;
publishing a GitHub Release requires an explicit manual workflow action.
The executable is unsigned and has no installer. It still requires WebView2
Runtime and the matching x64 Visual C++ v14 Runtime; neither is bundled. See
[Microsoft's prerequisite downloads](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
Release startup failures use a Windows message when the loader can start the app; debug builds keep
console feedback. See [installation, package contents and removal](INSTALL_WINDOWS.md).

## Controls

| Action | Control |
| --- | --- |
| Open a website | Address field, then Enter or Go |
| Focus/select address | Ctrl+L, Alt+D, F6 |
| Back / forward | Buttons or Alt+Left / Alt+Right |
| Reload | Button, Ctrl+R or F5 |
| Stop | × button or Escape |
| Find in the visible website | Ctrl+F or More (⋯) → Find in page; uses WebView2's native find bar |
| Zoom in / out | Ctrl+plus (or Ctrl+equals) / Ctrl+minus, Ctrl+wheel over content, or More (⋯) → Zoom + / Zoom - |
| Reset zoom | Ctrl+0, More (⋯) → Reset zoom (100%), or the percentage/reset button shown away from 100% |
| Print preview | Ctrl+P or More (⋯) → Print; choose output and confirm in WebView2's preview, or cancel |
| Home | Home button or Alt+Home → local new-tab page |
| New tab | + button or Ctrl+T |
| Close active tab | Ctrl+W or Ctrl+F4 (last normal tab opens Home; last private tab closes its window) |
| Close a specific tab | Its × button |
| Reopen the last closed tab | Ctrl+Shift+T (this window only; up to 16; the address is reloaded, page state is not restored) |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab or Ctrl+PageDown / Ctrl+PageUp (wraps around) |
| Select a tab | Click its title, or Ctrl+1 to Ctrl+8 by position; Ctrl+9 selects the last tab |
| Reorder tabs | Drag a tab, More (⋯) → Move left/right, or Ctrl+Shift+PageUp/PageDown |
| Pin/unpin active tab | More (⋯) → Pin tab, or Ctrl+Shift+P |
| Navigate focused tab strip | Left/Right, Home/End; Tab reaches the selected tab |
| Find an overflowed tab | All-tabs selector or tab-strip scroll buttons |
| Prevent active tab sleeping | More (⋯) → Keep awake (saved per normal tab) |
| Focus webpage from toolbar | F7 |
| Bookmark current page | Ctrl+D |
| Bookmark manager | More (⋯) → Bookmarks, or Ctrl+Shift+O |
| Import bookmarks from another browser | More (⋯) → Bookmarks → Import from file… (an exported bookmarks HTML file) |
| Offer to save passwords / fill in forms | More (⋯) → Privacy & security → Passwords and forms (both off by default) |
| Browsing history | More (⋯) → History, or Ctrl+H |
| Download manager | More (⋯) → Downloads, or Ctrl+J |
| New window | More (⋯) → New window, or Ctrl+N (up to 8 normal windows) |
| Close this window | Its close button or Alt+F4 (other windows stay open; the last one exits) |
| Reopen a closed window | More (⋯) → Reopen closed window (this run only) |
| Close every window | More (⋯) → Exit (all of them return at the next start) |
| New private window | More (⋯) → Private window, or Ctrl+Shift+N |
| Cut, copy, paste or select all in the address field | Right-click the address field, or the menu key |
| Choose Rust Browser as the default browser | More (⋯) → Privacy & security → Make default browser (opens Windows Settings) |
| Privacy and security | Connection word at the start of the address line, More (⋯) → Privacy & security |
| Move through toolbar | Tab / Shift+Tab |

Find, zoom and print apply to the visible website, including in a private window,
and do not operate on local browser panels. Zoom is held by each open tab's
WebView2 controller, with no Rust Browser session/database persistence; new and
recreated controllers start at 100%. WebView2 navigation may change its factor,
and the toolbar always reads the current value. Older runtimes missing the
Find/print APIs show an update/retry error. Printing can create lasting output
even from private browsing. Actual printer/PDF output is not part of validation.

Bare domains receive `https://`. Explicit `http://` supports local development.
Words and phrases are searched with the selected provider. The toolbar selector
chooses Google (default), DuckDuckGo or Bing and saves that choice automatically.
Queries are URL-encoded; active schemes and embedded credentials remain blocked.
Inputs resembling a domain or localhost address are treated as URLs. Single-label
intranet hosts require an explicit `http://` or `https://` prefix. HTTP is not upgraded automatically when typed
explicitly. The connection indicator shows the engine's current scheme and origin.
Open it for details. HTTPS encrypts the connection; it does not vouch for a site.
HTTP traffic is not encrypted. Loading and failed navigations show separate messages.
Website navigation, redirects, fragments and History API changes
update the address field. Editing the field pauses updates until focus leaves it.
Back/forward use WebView2's actual history, rather than a separate Rust URL list.

The status line reports connecting/loading, completion, invalid input, blocked
schemes and native WebView2 navigation errors (including a numeric error status).
WebView2 also displays its built-in network and certificate error pages. No
certificate validation bypass is installed. An HTTP error response such as 404,
429 or 500 is a completed navigation: the status line says the website answered
with that code, and it is not counted as a connection failure. A navigation that
turns into a download, or a page link to an app address such as `mailto:`, leaves
the current page and its status unchanged; a blocked app link shows a notice.

## Data and boundaries

Cookies, site storage and cache persist in `%LOCALAPPDATA%\RustBrowser\v0.1\content`.
The toolbar has its own `toolbar` profile folder. The existing v0.1 folder is reused
to preserve normal cookies and data on upgrade. Saved passwords and form data, when
you turn them on, are kept by WebView2 in the same `content` folder (see
[Passwords and autofill](#passwords-and-autofill)); there is no separate password
manager or general profile manager. Private windows use separate temporary storage.
Close every instance before manually removing a profile folder.

### Windows

Every normal window shares the profile: cookies, site data, bookmarks, history,
downloads and settings are the same in all of them. Each window has its own tabs,
toolbar, closed-tab list (Ctrl+Shift+T reopens only that window's tabs), notices
and panels, and its own taskbar and Alt+Tab entry. At most 8 normal windows can
be open; asking for a ninth shows a notice in the current window. Up to 64 tabs
can be open in each window.

Closing one of several windows hides it at once and removes it from the saved
session; its pages are destroyed a few seconds later, like closed tabs. Until
Rust Browser exits, More (⋯) → Reopen closed window brings back the most recently
closed window with its tab addresses and pins, in the same order. Titles,
keep-awake choices, back/forward history and page state are not restored, and the
list is kept in memory only (up to 8 windows). Clearing browsing data empties it
and closes every other window first.

Closing the last window, or More (⋯) → Exit, ends the browser. At the next start
every window that was open returns with its tabs, the one used last in front.
After a crash the next start restores the windows from the last save.

Permission prompts appear only for the selected tab of the focused window. An
engine restart rebuilds the pages of every window; a toolbar failure is
recovered in its own window only.

### Links from other apps and a second launch

Other apps and Windows pass links on the command line. Rust Browser opens only
explicit `http://` and `https://` links from them, never searches, guesses hosts,
user names or passwords, `file:`, `javascript:` or `data:` addresses, and never
local `.htm` or `.html` files. Links from other apps open in a normal window. A
command line with an unknown option or text that is not valid is refused with a
message before anything opens.

If a normal window is already running, a second launch hands its links to the
window used last, which opens them as tabs and comes to the front; started with
no link, it asks the browser for a new window (or shows the 8-window notice).
Nothing else can be requested, and no second process is started on the same
profile. If the running window is starting, closing, busy or
not responding, the second launch waits up to six seconds and then explains what
happened. After a crash the next launch simply takes over the profile. The channel
is private to your user account and Windows session; see SECURITY.md for its
protections.

`session.json` stores, for every open normal window, tab URLs, bounded titles,
order, pinned/keep-awake state and the active tab, plus favorites, theme and search/performance settings in **plain text**
inside this profile. URLs may contain sensitive paths, queries or tokens. No URL log
or page content is saved by the Rust session layer. Keep the profile private and out
of Git; OS account permissions protect it, not application encryption. Session saves
are coalesced on a background thread, atomically replaced and flushed on normal exit.
A profile lock prevents current browser instances from overwriting the same session.
Do not run v0.1 against the same profile simultaneously.

By default, restart restores every normal window's tab URLs and active tab. The Privacy & security
panel can disable URL retention while preserving search and history preferences. An optional command
line URL opens an additional tab. Back/forward history, form data, scroll position and
page JavaScript state are not restored after restarting the application. Up to 64 live
tabs are supported in each window. Hidden tabs retain their engines; eligible tabs can sleep as
described below. Invalid, oversized or unsupported session files are
backed up as `session-invalid-*.json`, with a visible recovery notice. Disk-write
failures show a notice; check disk space and profile permissions before exiting.

Use a separate profile for testing:

```powershell
$env:RUST_BROWSER_PROFILE = Join-Path $PWD 'test-profile'
cargo run --locked
```

The toolbar is local embedded HTML in a separate child webview. Only it receives a
Rust IPC command bridge. Remote sites receive **no application IPC bridge or injected
application scripts**. Toolbar navigation is restricted to its exact bundled data
URL; remote content navigation is limited to HTTP(S) and a neutral `about:blank`
page used for the local new-tab content surface and data clearing. The native external-scheme handler blocks OS app launches. Address entry rejects
embedded credentials and active schemes such as `javascript:`, `data:` and `file:`.
Unsupported permissions are denied. HTTP(S) downloads require destination selection.
Wry's default extra browser arguments are replaced with an empty argument string to
retain the Runtime's default security features. Media autoplay is disabled.

Runtime-marked user-initiated HTTP(S) and blank popup requests open a linked tab in
the same window and browsing mode. Native opener relationships and `noopener` rules
are preserved. See compatibility limits below. There are no
extensions, sync or full crash recovery. Threat protections and compatibility
need a dedicated review before everyday browsing. Runtime behavior remains dependent
on Microsoft's installed WebView2 version and policy.

## Sleeping tabs and local new-tab page

Eligible background tabs use WebView2's optional `TrySuspend` API after two minutes.
Select a sleeping tab to resume its existing view and JavaScript document. Sleep does
not discard/reload views, and a runtime refusal leaves the tab awake. A failed resume
shows a notice and a **Wake tab** retry; unsupported APIs disable sleeping for that
view. The runtime chooses how much memory it can release; shared renderer processes
and storage remain. Sleeping is not a privacy boundary.

Active, pinned, Keep awake, loading and granted-permission tabs are protected. Any
active download, pending permission prompt or data-clearing operation blocks automatic
sleeping across the window. Audio is queried through WebView2; its own suspension
heuristics also apply. Grants conservatively protect a document until navigation.
Arbitrary live-site work cannot be completely inferred: use **Keep awake** for live
dashboards, calls, uploads or other important background work. In **Privacy & security**,
turn off **Let inactive tabs sleep** to resume sleeping tabs and
disable automation, or change the delay. Pinning also protects a tab. Performance
preferences are separate from permission and privacy decisions.

Home and new tabs show bundled local UI with search, up to twelve named HTTP(S)
favorites, Dark/Slate/Plum/Light themes and quick access to bookmarks. Recent history is
**off by default**; enabling it displays up to six eligible cached library entries.
It follows history recording/filtering rules and is unavailable in private windows.
Favorite titles/URLs and themes are saved in normal settings independently of session
URL retention. Private favorites, themes, pins and tab order last only for that window
and never read the normal session. No background image is fetched.

The local page lives in the trusted toolbar view, separately from the ordinary content
view. Its internal `rust-browser:new-tab` marker is never accepted as a website URL;
remote sites still receive no privileged IPC. The connection indicator says **LOCAL**
there because no website connection is established.

See [benchmarks/README.md](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/README.md) for reproducible Windows measurement
instructions, measured results and their limits. Performance results describe the
documented synthetic workload, not general website or everyday-browser performance.

## Experimental Servo foundation

v0.11 makes no Servo changes. Experimental source-build and sandbox failures
remain documented; they do not describe production WebView2 validation.

The v0.10 investigation adds [LPAC diagnostics and scoped synthetic IPC](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/isolation/DIAGNOSTICS_V010.md),
resource/failure tests, native fixture input observations and repeated numeric
samples. The [rendering assessment](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/SECURE_RENDERING_V010.md)
records the WGL/Win32k blocker: no real Servo renderer has been contained.
See the [validation record](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/VALIDATION_V010.md) and
[threat model](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/isolation/THREAT_MODEL.md). Production package
version remained 0.8.0 during that research; arbitrary Servo content remains blocked.

Normal Rust Browser tabs use WebView2. v0.8 adds a standalone Windows executable
that embeds one Servo view and opens two built-in local fixtures. Its separate
source build uses a pinned Servo checkout, toolchain, lockfile, and output directory.
The production Cargo build and browser profile formats stay independent.

Follow the [Windows source-build procedure](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/SOURCE_BUILD.md) to
build the upstream example, compile the embedder, and run the fixture and shutdown
checks. The [validation results](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/EMBEDDER_RESULTS.md) document
the actual GUI checks, measurements, and security limits. The v0.7
[official servoshell procedure](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/README.md) is still available.

Keep the embedder on its bundled fixtures. The pinned Servo revision lacks a
supported Windows content-process sandbox. Each run creates a fresh profile and
can leave files behind. The host exposes no Rust Browser IPC and denies navigation
outside the fixtures. Keep real accounts, files, and profiles out of the experiment.
See [Security](SECURITY.md) for the exact scope.

## Compatibility and recovery

Popup tabs use the opener's exact native environment/profile. Native same-origin
rules still apply; `postMessage`, shared cookies and script-created tab closure work
in the controlled authentication fixture. Closing the active popup returns to its
opener when it still exists. Requests not marked user initiated by WebView2 are
blocked. Popup sizes/positions are not honored because popups are tabs. Opener and
JavaScript relationships are not restored across application restarts. Real OAuth
providers can restrict embedded browsers, third-party cookies or popups; no provider
or personal-account compatibility guarantee is made. External OS-protocol callbacks
remain blocked, and certificate validation is unchanged.

Native observation adds conservative protection for non-read HTTP methods, media
requests and WebSocket contexts **when reported by the Runtime**, including supported
worker request sources. It reads only method/context classifications, retains a
boolean, and does not read request bodies or log URLs. Protection lasts until the
tab closes, so a completed upload or past media request can keep a tab awake. Linked
popup pairs and pending requests are also protected. If request observation is
unavailable, automatic sleeping is disabled for that view. Not every stream, upload,
worker, WebSocket or WebRTC operation is observable: use **Keep awake** for live work,
especially generated/muted media or calls. Private views use the same conservative
policy within their isolated context.

HTML fullscreen expands the native window while retaining the browser toolbar and
origin indicator. Escape reaches the document's fullscreen handling and native
controls. A renderer failure shows an actionable error on that tab; Reload recovers
it. If the shared WebView2 browser process ends, every tab's view is rebuilt and its
address reloaded automatically, keeping tab order, pins and the selected tab; a
notice says so. After three such restarts within two minutes the browser stops
restarting the engine and asks to be restarted. Recovery loses unsaved forms,
JavaScript state, page history within the tab, popup opener links and unfinished
downloads. GPU, utility and frame helper failures are left to WebView2, which
restarts them itself.

The toolbar runs in its own WebView2 engine, separate from webpages. If that engine
or the toolbar's page process ends, the toolbar is rebuilt within about half a
second and a notice says so; open tabs and their pages are not touched. After three
such restarts within two minutes a message asks you to close and reopen the browser.

Closing a tab hides its view immediately and destroys it one second later, and
closing the window hides it and exits one second later. Destroying a view that was
visible a moment earlier crashed the WebView2 browser process on some pages.
Starting Rust Browser while a normal window already uses the same profile hands
any link to that window (see "Links from other apps" above). A second normal window
is not supported yet.
After a long event-loop pause the browser resumes pending/sleeping views and delays
new sleep attempts. This guard was tested with an injected timing gap; physical
Windows sleep/wake remains a separate validation task.

A valid committed session wins over a leftover temporary save. If the main session
is missing or corrupt, a valid `session.json.tmp` can be recovered. Invalid files are
preserved under unused recovery names. Access and other I/O errors fail visibly
instead of resetting the profile. SQLite remains transactional with the existing
schema. Process-termination tests cover committed snapshot recovery and rollback of
an uncommitted SQLite transaction; this does not establish recovery from every OS,
storage-device or power failure.

See [validation/README.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/README.md) for controlled fixtures, the
compatibility matrix, and repeatable 30–60 minute interactive and automated procedures.

The public survey is manual (`validation/site-survey.ps1`), uses disposable
profiles and saves any downloads inside that test profile. It does not enter
credentials or grant real permissions. `validation/interrupted-install.ps1` is a
separate manual installer timing sweep, not a power-loss/full-disk test. The optional
`validation/accessibility-uia.ps1` needs an interactive desktop and stops if its
window loses foreground. Its synthetic Windows input and UIA names are distinct
from spoken screen-reader, physical keyboard/IME and real display-scale checks.
These procedures are not background CI workloads; native controlled regressions
remain in CI. Do not use real profiles or the production installer identity for tests.

## Private browsing

Use **Private window** or **Ctrl+Shift+N**, or run `cargo run --locked -- --private`.
Each clearly labelled private window runs separately and verifies native WebView2
InPrivate mode before any website loads. Its tabs share that window's temporary
content profile. Private windows do not share cookies, cache or site storage with
normal tabs or with other private windows. No private session store is opened;
history is not recorded and temporary permission decisions are kept in memory.
Normal bookmarks are unavailable in private windows and remain unchanged.

Closing the window releases its views and attempts to remove its temporary directory.
On a later launch, the browser makes a best-effort attempt to remove abandoned
private directories. It does not remove live windows' data. Runtime files can remain
temporarily locked or survive an abrupt exit until a later cleanup succeeds.
Private mode does not provide anonymity or hide activity from sites, network observers,
Windows, or other software. Downloads and their metadata can remain on disk.

## Passwords and autofill

More (⋯) → Privacy & security → **Passwords and forms** has two switches:

- **Offer to save passwords**: WebView2 offers to save a password when you sign in,
  and fills it in next time.
- **Fill in forms automatically**: WebView2 suggests and fills addresses and other
  form entries you typed before.

Both are off for new profiles and for profiles from earlier versions. WebView2
itself turns form autofill on by default; Rust Browser turns it off until you opt
in. A change applies at once to every open tab in every window, and to tabs,
windows and popups opened later and to pages rebuilt after an engine restart.
Private windows never save passwords or form data, whatever the switches say, and
the switches cannot be changed there. The toolbar never saves or fills anything,
including the address field.

The data is stored by WebView2 inside the profile's `content` folder
(`%LOCALAPPDATA%\RustBrowser\v0.1\content`), protected by Windows for your user
account. Rust Browser never reads, decrypts, lists or exports it. WebView2 offers
no way to list, view or delete one saved password: to remove them, use
**Clear browsing data → Saved passwords** (or **Autofill data**), which removes all of
them. Turning a switch off stops saving and filling; to remove what was saved, clear it.

Passwords cannot be imported: WebView2 has no import interface (the WebView2 API
used here has only the two switches and the clearing kinds). Keep using your old
browser's saved passwords for sites you have not signed in to yet, or use a
separate password manager app.

## Website permissions

Camera, microphone, geolocation and notifications can prompt on the active tab after
a user-initiated request. The prompt shows the actual requesting origin and capability.
Choose Allow or Deny, with optional remembering for that exact scheme, host, port and
capability. Private remembered decisions last only for that window. Unsupported
capabilities, background requests, cross-origin frames and nonsecure origins other
than loopback are denied. Pending requests expire after one minute and are denied when
their tab navigates, switches or closes. Windows and WebView2 policies still apply.

Privacy & security lists remembered rules. Revoking a rule closes tabs to stop
active access and preserves other application rules. The app suppresses WebView2's
automatic persistence for new decisions and resets legacy runtime permission settings
before navigation on startup. A failed or unconfirmed permission clear leaves requests
denied until restart. Keep the Runtime updated if startup security checks fail.

## Clear browsing data

Select categories in **Privacy & security**, then confirm. The browser prepares a
blank view and closes other tabs before clearing, so active pages cannot immediately
repopulate the old data. Finish or cancel downloads first. Clearing history covers
the active library and engine history; cookies include site storage; cache includes
disk cache and Cache Storage. Engine results are reported only after its callback.

**Saved passwords** and **Autofill data** are separate choices, unchecked by
default; they use WebView2's own password and autofill clearing. Clearing history,
cookies or anything else never removes saved passwords or form data.

Saved session clearing disables URL retention and removes session recovery files.
Closing the tabs also changes the current tab list, even if that category is unchecked.
Bookmarks and downloaded files remain. Library recovery copies are retained because
they can contain otherwise lost bookmarks; they can also contain older history and
permission rules. Review partial failures in the panel. This is logical deletion,
not secure disk erasure. Browsing again can create new data.

See [WebView2 controller options](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controlleroptions),
[permission persistence](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2permissionrequestedeventargs3),
and [asynchronous browsing data clearing](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2profile2).

## Bookmarks and history

Ctrl+D saves the current tab once and opens the bookmark manager. The manager supports
adding/editing titles and HTTP(S) URLs, removing bookmarks, flat folders, moving a
bookmark with its folder selector, searching titles/URLs and opening entries in the
current or a new tab. Removing a folder keeps its bookmarks in Unfiled. Duplicate
bookmark URLs are not added again. There are limits of 5000 bookmarks and 200 folders;
queries show the first 200 matching results, so narrow the search to find others.

History records successful top-level navigation completions with titles, URLs and
timestamps, independently of WebView2's native back/forward stack. Repeated completion
events are deduplicated and visits to the same URL within 30 seconds are coalesced.
The latest 10,000 visits are retained. Failed navigation and query-string/credential
URLs are not recorded; fragments are stripped. Consequently search-result and many
sign-in pages are deliberately absent. Page paths and titles can still be sensitive.

Use the history search field and From/Through dates to filter visits. Date boundaries
use UTC calendar days; displayed visit times use the local timezone. Search is literal
and SQLite's default case folding primarily covers ASCII. Entries open in the current
or a new tab. Individual removal is available; Clear all history asks for confirmation.
Clearing this library does not clear WebView2's back stack, cookies, cache or session
URLs. The Record history checkbox pauses/resumes new recording and persists across
restarts. Session restoration remains enabled independently and can retain full query
URLs even when history is paused, unless retention is disabled separately.

`library.sqlite` lives beside `session.json` in the existing profile. SQLite is bundled,
so no separate database installation is needed. One background worker owns the
connection and runs parameterized queries and transactional schema migrations (currently
version 3, including remembered permission rules). Native browser callbacks never write the database directly. A corrupt
database is preserved as `library.corrupt-<timestamp>` before recovery. A newer schema
or access error disables the library with a notice rather than resetting its data;
ordinary browsing remains available. Back up the profile with the browser closed.
Database files/backups contain personal data, are not encrypted and must stay out of Git.

## Importing bookmarks

More (⋯) → Bookmarks → **Import from file…** opens a Windows file dialog. Choose a
bookmarks HTML file exported from another browser:

- Firefox: Bookmarks → Manage bookmarks → Import and Backup → Export Bookmarks to HTML.
- Chrome or Edge: Bookmarks manager (or Favorites) → ⋯ → Export bookmarks.

Rust Browser reads only the file you choose. It never reads another browser's
profile folder or databases. The file is treated as untrusted text: nothing in it
runs, loads from the network or is followed, and only each link's address and the
names of links and folders are read. Only explicit `http://` and `https://`
addresses that bookmarks accept are imported; `javascript:`, `data:`, `file:`,
`place:` and addresses with a user name or password are refused.

Limits: the file may be at most 10 MiB with at most 20,000 links and 32 levels of
folders, or it is refused whole. Titles longer than 256 characters and folder names
longer than 80 are shortened; addresses longer than 8,192 characters are refused.

Each bookmark goes into a folder named like the innermost folder that held it, and
an existing folder with the same name is reused. Folders here cannot contain other
folders, so nested folders become top-level folders, and the summary says how many.
Addresses you already have, or that repeat in the file, are skipped. When the import
finishes, a notice says how many were added, skipped as duplicates and refused, and
why. The import is all or nothing: if it fails, the file is refused, or it would
pass the 5,000 bookmark limit, your bookmarks stay as they were. Private windows
cannot import.

## Downloads

When WebView2 starts an HTTP(S) file download, choose its destination **folder** in the
Windows picker. The browser sanitizes the server-suggested filename, rejects path
traversal/device names/alternate data streams, and stages the file in an app-created
subfolder of that destination. Completed files move atomically without replacing an
existing file: collisions receive `(1)`, `(2)`, and so on. The actual final name appears
in the manager. Windows file metadata, including any runtime-written zone information,
is retained by the same-volume move. No downloaded file is automatically opened or
executed. Open folder opens the selected directory in Explorer only.

The manager displays received/total bytes when available, completion, failures and
cancellation. Cancel uses WebView2's native API; cancelled/interrupted partial files
are cleaned up when safe. No progress for two minutes cancels a stalled transfer.
Interrupted operations are not resumed automatically by the application. Native retry
behavior can briefly precede an interruption event. Duplicate simultaneous downloads
from one address are rejected, with at most 16 active transfers and 256 in-memory
records. Download records are session-only; pause/resume and restart resumption are
not implemented. Unknown content lengths show bytes without a percentage. Blob/data
downloads are unsupported. Destination errors, runtime security policy and network
errors can fail a download; no certificate or security check is bypassed.

Abrupt browser termination can leave a `.rust-browser-download-*` staging directory.
Finalization failures preserve the staged file instead of overwriting another file.
New staging directories carry a locked ownership lease. Selecting a destination
checks up to 256 immediate entries off the UI thread and removes only recognized,
unlocked, empty abandoned staging directories. Live transfers, reparse points,
legacy/unrecognized directories and every directory containing a payload are kept.
Review nonempty staging folders manually: their files may be incomplete or complete.
No disk-wide/startup scan or automatic deletion of potentially completed files occurs.
Do not manually remove staging directories while transfers are active. Prefer a download
folder outside the source checkout. Downloaded content remains untrusted.

The Windows adapter uses native `DownloadStarting` deferrals, `StateChanged`, progress
queries and `Cancel`. Wry's high-level callbacks alone do not expose the required
progress/cancellation controls. See [download operations](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2downloadoperation)
and [destination behavior](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2downloadstartingeventargs).

## Structure and next steps

```text
src/main.rs           Entry point and Windows platform gate
src/app.rs            Event loop, session lifetime, layout, profile and state
src/navigation.rs     URL normalization and scheme policy, with unit tests
src/tabs.rs           Engine-independent tab IDs, ordering and selection
src/background.rs     Engine-independent sleeping eligibility and callback generations
src/native_background.rs Optional WebView2 suspend/resume/audio adapter
src/native_activity.rs Native activity classification, fullscreen and process failures
src/native_popup.rs   Bounded UI-thread popup deferrals and linked native views
src/settings.rs       Search provider settings and query encoding
src/storage.rs        Validated snapshots, profile lock and background persistence
src/privacy.rs        Private profile leases and clearing/retention policy
src/security.rs       Exact-origin permission types and connection indicators
src/native_security.rs Windows permission deferrals and confirmed engine data clearing
src/library.rs        SQLite bookmark/folder/history model and migrations
src/library_worker.rs Bounded database worker and application replies
src/downloads.rs      Filename policy, stall tracking and no-overwrite finalization
src/download_recovery.rs Locked staging ownership and bounded empty-folder recovery
src/native_downloads.rs Windows picker and WebView2 download operations
src/lib.rs            Testable core shared by application and integration tests
src/engine.rs         Wry content view and Windows COM adapter
src/engine_api.rs     Validated engine-independent navigation and owned page events
src/ui/mod.rs         Toolbar commands and serializable view state
src/ui/toolbar.html   Local accessible toolbar
```

Each tab owns one engine instance with its own navigation state. All content tabs
share the site's WebView2 context, while the toolbar remains separate. Native events
carry stable tab IDs, including background callbacks; late events from closed tabs
are ignored. Switching hides/shows views without recreation. The generic tab model,
settings, library model and persistence do not depend on WebView2. Native download
operations stay in the Windows adapter; UI rendering uses plain serializable records.
Multiple named normal profiles remain future work. Add an engine
trait only when a second backend's actual requirements are known. Servo would need
its own embedding, rendering/input integration and compatibility testing; it cannot
simply be substituted into Wry. No Servo dependency is included.

## Local diagnostics

**Privacy & security → Local application diagnostics** shows fixed application
event categories, timestamps and counts. Refresh reads the current bounded
snapshot; Clear requires confirmation and removes only diagnostic records.
Bookmarks, history, cookies, sessions and downloaded files remain. New events
may be recorded after clearing. Nothing is uploaded automatically.

Normal profiles store `local-diagnostics/journal.json` (at most 128 entries and
32 KiB), a bounded temporary file, a lock and a run marker. Writes are coalesced
off the UI thread. No URL, page title, form data, cookie, password, token, file
path or arbitrary error payload is accepted by this diagnostic API. Private
windows do not open the journal or its worker. Local times/counts can still reveal
application usage; protect the profile with your Windows account permissions.
Unreadable or unsupported records are preserved until explicitly cleared.

A leftover run marker produces a recovery notice on the next normal startup.
It is a best-effort unclean-exit hint, not a crash dump or proof that every
storage flush succeeded. Unsaved page state is not recovered. Release panic
feedback omits payloads and paths; debug startup failures use a static console
message. There is no remote crash-reporting service.

See [production architecture](https://github.com/Niithursan/RustBrowser/blob/main/ARCHITECTURE.md), [acceptance results](https://github.com/Niithursan/RustBrowser/blob/main/validation/DAILY_DRIVER.md)
and [v0.11 measurements](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/V011_RESULTS.md) for the verified scope.

## Development checks

```powershell
rustup component add rustfmt clippy
cargo fmt --all -- --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked
cargo build --locked --release
```

See [VALIDATION.md](VALIDATION.md) for checks actually performed and known gaps.
GitHub Actions runs formatting, Clippy with warnings treated as errors, all regular
and native WebView2 tests, and a parallel optimized Windows build on pull requests and
`main` (open a draft pull request to get CI for a feature branch). Dependency security
runs `cargo audit --deny warnings` on main, pull requests and weekly. Caching and
measured timings: [CI.md](https://github.com/Niithursan/RustBrowser/blob/main/CI.md). Dependabot proposes Cargo and Actions updates.

Development uses feature branches and pull requests into `main`, with Conventional
Commits. See [CONTRIBUTING.md](https://github.com/Niithursan/RustBrowser/blob/main/CONTRIBUTING.md) for the workflow and release steps.
Toolchain version is recorded in `rust-toolchain.toml`; commit `Cargo.lock` updates.
The private repository is [Niithursan/RustBrowser](https://github.com/Niithursan/RustBrowser).

Manual smoke checklist:

- Open HTTPS, follow a link, back, forward and reload; verify URL and button state.
- Focus a webpage and try every shortcut; edit the address without losing text.
- Try a fragment link or History API navigation and verify the address updates.
- Try a nonexistent domain and invalid/blocked schemes; verify readable feedback.
- Resize, minimize/restore and move between displays with different scaling.
- Restart with the same profile and check a non-sensitive test site's storage.

## Upstream references

- [Wry documentation](https://docs.rs/wry/0.57.1/wry/)
- [WebView2 best-effort suspension and resume](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_3)
- [Wry Windows extension](https://docs.rs/wry/0.57.1/x86_64-pc-windows-msvc/wry/trait.WebViewExtWindows.html)
- [winit documentation](https://docs.rs/winit/0.30.13/winit/)
- [WebView2 documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/)

MIT licensed; see LICENSE. Dependencies retain their respective licenses.
