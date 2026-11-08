# Security

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
