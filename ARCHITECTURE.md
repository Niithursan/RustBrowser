# Production architecture

WebView2 remains the production engine in 0.14.0. Servo is a separate fixture-only
experiment; none of its dependency graph, profiles, privileges or process work
is introduced into normal browsing by this milestone.

| Component | Ownership and boundary |
| --- | --- |
| `app.rs` | One winit event loop owns the window, tab/session state, native views and UI commands. Stable tab IDs route callbacks; closed-tab callbacks are ignored. |
| `tabs.rs`, `settings.rs`, `navigation.rs` | Engine-independent identity/order/pins, validated settings and address/search handling. |
| `engine_api.rs` | Small content-target/navigation/event interface. It does not pretend input, surfaces or storage are identical between engines. |
| `engine.rs` | Wry/WebView2 adapter: native navigation, title/history observation, input accelerators and renderer failures. Native navigation IDs reject stale/duplicate completions. |
| `ui/` | Bundled trusted HTML in a separate toolbar WebContext. Only its exact generated document receives the privileged command bridge. Ordinary pages receive no application bridge. |
| `background.rs`, `native_background.rs`, `native_activity.rs` | Suspension policy, native suspend/resume and conservative protected-activity observation. Existing controllers/documents survive sleeping. |
| `storage.rs` | Locked normal profile, coalesced atomic session snapshots, recovery barriers and committed-data restoration. Private sessions do not open this store. |
| `library.rs`, `library_worker.rs` | SQLite schema/migrations and one background worker for bookmarks/history/permission rules. Schema 5 stores validated raster `data:image/...` bookmark icons (`bookmarks.icon`) with automatic migration from older schemas. |
| `privacy.rs`, `security.rs`, `native_security.rs` | Separate private leases/contexts, origin/capability decisions, default denial and callback-confirmed engine deletion. |
| `native_popup.rs`, `native_downloads.rs`, `downloads.rs`, `download_recovery.rs` | Linked popup deferrals and profile identity; destination selection, progress/cancel, staging/no-overwrite publication and conservative abandoned-staging handling. |
| `diagnostics.rs` | Bounded 32-message worker channel and fixed typed events. At most 128 records/32 KiB per journal plus a same-sized temporary journal. No free-form browsing data or private writer. |
| `build.rs`, `packaging/windows/` | Windows SDK version/manifest resource and allowlisted portable package. No new Cargo dependency, installer service or updater. |

## Views beside the page (v0.21)

A window has a toolbar view, the active page's view and, when used, two side views:
the tab column (vertical tabs) and the library column. All trusted views load the same
local document; the URL fragment (`#tabs`, `#library`) picks the part each shows, and
each is built by the same function with the same restrictions. Side views are created
the first time they are needed and hidden, not destroyed, when not shown.

Where each goes comes from one pure function, `layout()`, of the window size, the
toolbar height, whether vertical tabs and the library are on, and whether a full
panel, the new-tab page or an open dropdown fills the window:

| State | Tab column | Toolbar | Page | Library |
| --- | --- | --- | --- | --- |
| Normal | none | full width, top | below the toolbar | none |
| Vertical tabs | left, 240 wide, full height | right of the column, top | right of the column, below the toolbar | none |
| Library open | as above | as above | narrower by the library width | right, below the toolbar |
| New tab + library | as above | narrower by the library width, full height | hidden | right, full height |
| Full panel, dropdown | as above | its column, full height | hidden | hidden |
| Page fullscreen | hidden | full width, top | full width, below the toolbar | hidden |

The library keeps at least 320 pixels for the page (or new-tab column). A unit test checks 1,080
combinations for overlap, and native tests measure the real windows.

**Frameless window and flush caption controls.** Browser windows are created with
`winit` `with_decorations(false).with_undecorated_shadow(true)`, removing the separate
top OS title bar while preserving the native Win32 frame styles (`WS_CAPTION`,
`WS_SYSMENU`, `WS_SIZEBOX`, `WS_MAXIMIZEBOX`, `WS_MINIMIZEBOX`), Aero Snap, Windows 11
rounded corners, and theme-coloured borders. Because child WebView2 surfaces cover
`(0, 0, width, height)`, the three `46x36` caption buttons (`#win-minimize`,
`#win-maximize`, `#win-close`) are rendered inside the trusted chrome surface that owns
`(width, 0)`: `#tabrow` in horizontal-tabs mode, `#nav` in vertical-tabs mode, and
`#library-caption` when the library column sits beside `New tab` (`y == 0`), hiding
during HTML5 page fullscreen. Empty strip space starts `window.drag_window()` after a
`4px` pointer threshold (preserving `dblclick` maximize/restore), right-click opens
`window.show_window_menu()`, and a `4px` `#resize-top` strip invokes
`window.drag_resize_window(North | NorthWest | NorthEast)` when restored.

**Why the side views never overlap the page.** Each WebView2 view is a separate window
under the browser window, and PR #27 found that Windows does not keep their stacking
order: a view laid over another can end up underneath it. So no view is ever placed
over another; anything beside the page takes its own space and the page shrinks or
moves. That is why vertical tabs are a column of their own rather than a panel that
slides over the page, and why the library is a column rather than a drawer.

**Profiles.** A profile is a folder: the original profile folder is the Default
profile (the `root`) and others are `root/profiles/p<n>`. Each profile runs in its own
process, started with `--profile=<id>`, so it has its own session lock, `Store`,
library database, WebView2 environments and link hand-off endpoint (keyed by the
folder), and nothing in `Session` is shared between profiles. `rust_browser::profiles`
owns the list (`root/profiles/profiles.json`): every change runs under a short
exclusive file lock and is written atomically, because several profile processes
edit it. Ids are `default` or `p<n>` and are never reused. Deleting renames the folder
to `<id>.deleted` under the list lock (refused while the profile's session lock is
held) and removes it off the UI thread; leftovers are swept on a later start. A plain
launch with more than one profile runs `native_picker` first: a small window with its
own event loop that only chooses, then starts the chosen profile as a new process,
because a process can run one winit event loop.

**Dropdowns.** The menu, the site-information badge, the star, the shield and the
bookmarks-bar folders open dropdowns under their buttons. A dropdown is part of the
toolbar document, so while one is open over a webpage the toolbar view grows down its
column above the page, which stays where it is and keeps running. The toolbar reports
where the dropdown is drawn (`set_menu_area`), and the browser clips the toolbar's host
window to the chrome strip plus the dropdown's rounded shape (`SetWindowRgn`) and
raises it above the page. Outside that clip the page is visible and takes input; every
view host has `WS_CLIPSIBLINGS` so the page never paints over the raised toolbar.
Pressing the page gives it focus (`GotFocus`), which closes the dropdown. Over a full
panel such as the new-tab page there is no page, so the toolbar covers its column
unclipped and the panel stays visible, dimmed, behind the dropdown. Only one dropdown
is open at a time. A folder
dropdown asks the library worker for that folder's contents (`Filter::menu_folder`,
set by the browser only) and the star's dropdown reads the current page's bookmark
(`Filter::page`). A borderless popup window above the browser window was considered
and not used: it would add a new kind of window; the still keeps every view side by
side.

## Navigation completion and cancellation

Wry's navigation policy runs before the production native starting observer.
Already-cancelled prohibited starts do not become the current load. Completion
must match the current native ID and occur only once before URL/title extraction
or history recording. Redirects remain within the engine's navigation lifecycle.
History API changes still use native history/source observation.

A host load invalidates the old completion before calling the native Navigate
API. An obsolete blank navigation may start only after its web replacement has
already been requested; an awaiting-web flag excludes that blank start without
discarding errors from the requested document. Rejected native commands restore
the prior state when a newer navigation has not already started. No certificate
or network status is globally converted to success.

WebView2 can report explicit Stop as `CONNECTION_ABORTED` (9), as well as
`OPERATION_CANCELED` (14). A status-9 result is cancellation only when this host
recorded Stop for that same navigation. Otherwise it remains an error, except for
the engine follow-up and download cases described under failure reporting below. Failed
Stop calls restore the previous marker. Cancellation does not record a successful
history visit or replace the newer tab's loading state. A completed HTTP error
response can still be a successful engine navigation, as before.

## View lifetime and engine restart (v0.14)

A tab's WebView2 view is never destroyed while visible. `Session::close_tab` and
data clearing move the page to `Session::retired` after hiding it; `reap_retired`
destroys it one second later (`RETIRE_DELAY`) from `about_to_wait`. At most
`MAX_TABS` (64) retired views are held, so closing every tab within the delay keeps
all of them; only opening and closing more than 64 tabs within one second destroys
the oldest early. Window close
goes through `App::begin_shutdown`: persist, hide the window and every view, then
drop the session after the same delay. Events for a retired tab find no tab and are
ignored. Engine data clearing does not start while a retired view exists.

`native_activity::classify` maps WebView2 `ProcessFailed` kinds to four cases.
Only a browser-process exit sets `Session::engine_lost`; `recover_engine` then runs
once on the next idle turn, after already queued events for the dead views. It
builds a new engine per tab with the same tab ID through the same `build_engine`
path as ordinary tabs, reloads each address, resets suspension state and opener
links, cancels permission and popup deferrals, and re-runs startup security
initialization if it had not completed. `admit_engine_restart` allows three
restarts per two minutes. The toolbar lives in its own WebView2 process group and
is not covered.

## Failure reporting (v0.14)

`NavigationCompleted` with `IsSuccess == false`, an unknown web error status and an
HTTP status of 400 to 599 is a completed navigation carrying `http_status`; it sets
no failed state. After a reported connection failure, the next engine-initiated
start for the same address is treated as the engine's follow-up: it produces no
loading event, and if it ends as "connection aborted" it is dropped. Any other
outcome of that navigation, a redirect of it, or any host command in between is
reported normally. Navigation failures are written to diagnostics after two
seconds so that a download request arriving in between can claim the "aborted"
result of the navigation it replaced.

## Diagnostic lifecycle

The normal session lock is acquired before diagnostics starts. The journal also
has its own exclusive lock. It checks ordinary files/reparse points, schema and
size; malformed/unsupported content is retained and reported unavailable.
Coalesced events flush at roughly two-second intervals when dirty. Channel
overflow increments a numeric lost-event counter instead of blocking browsing.
Inspection/clear commands are bounded requests; a busy queue is visible to users.

Normal recorder drop drains queued events, saves a final category and removes
its exact marker only after a successful journal write. Panic unwind retains
the marker. It is not an OS crash reporter: some early failures create no
journal, worker/final-marker errors can leave stale hints, and diagnostic teardown
is not a transaction with every session/library/engine flush. Absence of a marker
does not prove a fully durable shutdown. Existing session/SQLite recovery remains
independent. Private calls use `None` and create no diagnostic directory/thread.

## Persistence and upgrades

Default `%LOCALAPPDATA%/RustBrowser/v0.1` and session format 1 remain unchanged; library
schema 5 adds `bookmarks.icon` via transactional `ALTER TABLE` migration from earlier
schemas. Application version 0.11.0 intentionally follows production
0.8.0. Light is an additive preference; old binaries can reject new enum values,
so rollback needs a closed whole-profile backup. There is no profile migration,
automatic update, telemetry upload or shared experimental browsing storage.
