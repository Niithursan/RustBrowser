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
| `library.rs`, `library_worker.rs` | SQLite schema/migrations and one background worker for bookmarks/history/permission rules. Existing schema remains 3. |
| `privacy.rs`, `security.rs`, `native_security.rs` | Separate private leases/contexts, origin/capability decisions, default denial and callback-confirmed engine deletion. |
| `native_popup.rs`, `native_downloads.rs`, `downloads.rs`, `download_recovery.rs` | Linked popup deferrals and profile identity; destination selection, progress/cancel, staging/no-overwrite publication and conservative abandoned-staging handling. |
| `diagnostics.rs` | Bounded 32-message worker channel and fixed typed events. At most 128 records/32 KiB per journal plus a same-sized temporary journal. No free-form browsing data or private writer. |
| `build.rs`, `packaging/windows/` | Windows SDK version/manifest resource and allowlisted portable package. No new Cargo dependency, installer service or updater. |

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
destroys it one second later (`RETIRE_DELAY`) from `about_to_wait`. At most 16
retired views are held; closing faster destroys the oldest early. Window close
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

Default `%LOCALAPPDATA%/RustBrowser/v0.1`, session format 1 and library schema 3
remain unchanged. Application version 0.11.0 intentionally follows production
0.8.0. Light is an additive preference; old binaries can reject new enum values,
so rollback needs a closed whole-profile backup. There is no profile migration,
automatic update, telemetry upload or shared experimental browsing storage.
