# Production architecture

WebView2 remains the production engine in 0.11.0. Servo is a separate fixture-only
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

WebView2 can report explicit Stop as `CONNECTION_ABORTED` (9), as well as
`OPERATION_CANCELED` (14). A status-9 result is cancellation only when this host
recorded Stop for that same navigation. Otherwise it remains an error. Failed
Stop calls restore the previous marker. Cancellation does not record a successful
history visit or replace the newer tab's loading state. A completed HTTP error
response can still be a successful engine navigation, as before.

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
