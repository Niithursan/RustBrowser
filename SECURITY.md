# Security

This is an experimental browser. Report suspected vulnerabilities privately through GitHub private vulnerability reporting if enabled, or contact the repository owner privately. Do not include cookies, passwords, browsing profiles, or personal URLs in public issues.

The browser uses the installed Microsoft WebView2 Runtime. Keep that runtime and Windows updated. Remote content receives no application IPC handler; the local toolbar has a separate profile and restricted navigation. Website permission requests are denied. HTTP is supported and does not provide HTTPS transport protection.

Session URLs and provider settings are saved
in plain text within the user's local profile, protected by OS account permissions.
URLs may contain sensitive query parameters or tokens. Session files and backups
must never enter Git. Restored URLs are validated and bounded before loading. Only
HTTP(S) navigation is allowed, with explicit URL credentials rejected. Restoring
tabs makes network requests to those sites; there is no private mode or restore
prompt. Hidden tabs share site storage and can continue background activity.

Version 0.3 stores bookmarks and history locally in `library.sqlite` using parameterized
queries. Automatic history excludes query strings and credentials and strips fragments;
titles/paths may still be sensitive. Recording can be paused, and clear-history requires
confirmation. This does not clear session URLs, cookies or native back/forward entries.
The database, session data and recovery backups are not encrypted. Keep the profile
private and outside version control; recovery files can retain deleted historical data.

Downloads accept HTTP(S) sources and require destination selection. Server filenames
are sanitized, Windows device/stream syntax is neutralized, and no-replace finalization
retains existing files. Staging stays on the selected volume, preserving file metadata
on completion. Runtime policy failures are not bypassed. The app never opens or executes
a downloaded file; its only launch action opens a validated directory in Explorer.
Download records are not persisted. A same-user local attacker or a compromised OS is
outside this application's filesystem threat boundary. This is not a malware scanner.

Dependency security runs on pushes to main, pull requests and a weekly schedule. Dependabot proposes Cargo and GitHub Actions updates; they still require review and passing checks. The release executable is unsigned. This project has not undergone an independent security review.

`.cargo/audit.toml` records three advisory exceptions for crates absent from the supported Windows dependency graph: proc-macro-error, ttf-parser and glib. They remain in Cargo.lock because Cargo resolves other platforms too. Reassess these exceptions before adding non-Windows support or changing the desktop dependencies. All other audit warnings remain build failures.
