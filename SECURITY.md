# Security

This is an experimental browser. Report suspected vulnerabilities privately through GitHub private vulnerability reporting if enabled, or contact the repository owner privately. Do not include cookies, passwords, browsing profiles, or personal URLs in public issues.

Version 0.1 uses the installed Microsoft WebView2 Runtime. Keep that runtime and Windows updated. Remote content receives no application IPC handler; the local toolbar has a separate profile and restricted navigation. Website permission requests and downloads are denied in this version. HTTP is supported for development and does not provide HTTPS transport protection.

Version 0.2 retains these boundaries. Session URLs and provider settings are saved
in plain text within the user's local profile, protected by OS account permissions.
URLs may contain sensitive query parameters or tokens. Session files and backups
must never enter Git. Restored URLs are validated and bounded before loading. Only
HTTP(S) navigation is allowed, with explicit URL credentials rejected. Restoring
tabs makes network requests to those sites; there is no private mode or restore
prompt. Hidden tabs share site storage and can continue background activity.

Dependency security runs on pushes to main, pull requests and a weekly schedule. Dependabot proposes Cargo and GitHub Actions updates; they still require review and passing checks. The release executable is unsigned. This project has not undergone an independent security review.

`.cargo/audit.toml` records three advisory exceptions for crates absent from the supported Windows dependency graph: proc-macro-error, ttf-parser and glib. They remain in Cargo.lock because Cargo resolves other platforms too. Reassess these exceptions before adding non-Windows support or changing the desktop dependencies. All other audit warnings remain build failures.
