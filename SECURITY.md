# Security

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
unsaved forms. See [architecture](ARCHITECTURE.md) and [acceptance limits](validation/DAILY_DRIVER.md).

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

The v0.10 [rendering decision](experiments/servo/SECURE_RENDERING_V010.md)
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
launched under these restrictions. See [diagnostics](experiments/servo/isolation/DIAGNOSTICS_V010.md).
The separate upstream lockfile audit and strict minimal-feature Clippy
remain failing as recorded in [validation](experiments/servo/VALIDATION_V010.md).
Production audit and strict Clippy pass independently.

The v0.9 [threat model](experiments/servo/isolation/THREAT_MODEL.md) keeps
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

This is an experimental browser. Report suspected vulnerabilities privately through GitHub private vulnerability reporting if enabled, or contact the repository owner privately. Do not include cookies, passwords, browsing profiles, or personal URLs in public issues.

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
[Servo security findings](experiments/servo/RESEARCH.md). Cargo audit covers the
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
only the local fixture URLs. See [Servo results](experiments/servo/EMBEDDER_RESULTS.md)
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
Fullscreen keeps the origin toolbar visible. Renderer recovery is user initiated
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
navigation, shutdown or timeout. Revocation and data clearing close tabs and report
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
