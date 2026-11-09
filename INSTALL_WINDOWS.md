# Windows installation, upgrade and removal

Rust Browser 0.11.0 is an unsigned x64 portable Windows application. It uses
the installed Microsoft Evergreen WebView2 Runtime and x64 Microsoft Visual C++
v14 Runtime. It has no automatic
updater, installer service, default-browser registration or administrator
requirement. Do not run it as administrator. `asInvoker` retains the access
of the account launching it; it is not an application sandbox.

## Install a reviewed portable package

1. Obtain an explicitly approved version from this repository's release
   page, or build the reviewed source with the pinned Rust MSVC toolchain,
   Visual Studio C++ tools and Windows SDK. No v0.11 release is published
   by this development task.
2. Install the [Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
   from Microsoft if necessary. The browser does not silently install it.
   Microsoft Edge alone is not a substitute for the Runtime.
   Also install the matching [latest supported x64 Visual C++ v14 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
   if absent. Its version must be at least as recent as the MSVC tools used to
   build the package. Inspection of this release executable's PE imports found
   `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll` plus Windows/UCRT libraries. Neither
   Microsoft runtime is bundled or silently installed. Prerequisite installation
   can require privileges independently of running Rust Browser.
3. Compare `Get-FileHash .\rust-browser-v0.11.0-windows-x64.zip -Algorithm SHA256`
   with the approved `.sha256` file. A checksum detects changed bytes; it
   does not authenticate the publisher. The executable is not signed or
   certified as trusted by Windows; Windows may show reputation warnings.
4. Extract into a new, ordinary per-user application directory, for example
   `%LOCALAPPDATA%\Programs\RustBrowser\0.11.0`. Keep it separate from the
   data profile. Do not extract into Windows system directories or overlay
   a running executable. Launch `rust-browser.exe`; a shortcut is optional.

The package contains the executable, documentation and `package.json` with
its source commit, architecture, version, signature status and file hashes.
The application version is embedded in Windows Properties and shown in
window titles. Release startup errors use a Windows message; debug builds
use the console. Local diagnostics may contain a startup event when the
profile was successfully opened. Missing Runtime/clean-machine startup
still needs independent clean-machine testing. Missing imported DLLs can fail in
the Windows loader before Rust Browser's own error message or diagnostics runs;
follow Microsoft's prerequisite instructions instead of downloading loose DLLs.

## Upgrade without moving browsing data

Close all Rust Browser windows and wait for their WebView2 processes to
exit. Make a backup of the closed profile before an upgrade. The default
path remains `%LOCALAPPDATA%\RustBrowser\v0.1`, preserving existing bookmarks,
history, cookies and session settings. `RUST_BROWSER_PROFILE`, when set,
selects a different profile; never use a source/installation directory.

Extract the new application into a new versioned sibling directory and
launch it. Verify tabs, bookmarks and preferences before removing the old
application folder. Do not copy files into a live profile, delete lock
files to bypass a running instance, merge SQLite files, or use a production
profile for test workloads. One normal instance owns a profile; private
windows use separate temporary contexts.

Version 0.11.0 intentionally follows production 0.8.0: v0.9 and v0.10 were
research milestones. Session version 1, SQLite schema 3 and the profile
location are unchanged. Light-theme settings are new; older applications
may reject them. Downgrade compatibility is not guaranteed. Close the
browser and restore an appropriate whole-profile backup before rolling
back; preserve the newer profile separately until recovery is verified.

## Remove the application

Close its windows, remove only the extracted application directory and
any shortcut you created. Browsing data remains in the separate profile.
The application installs no service or scheduled updater. Deleting a
profile is a separate, explicit user action; uninstall/removal instructions
do not delete it. Downloaded files remain in their selected destinations.

## Packaging decision and release controls

| Option | Assessment for this milestone |
| --- | --- |
| Portable ZIP | Selected for the verified subset. Fixed file allowlist, x64/version checks, manifest hashes, stable entry ordering/timestamps, no registry changes or elevation. Manual upgrade/removal preserves the separate profile. |
| Per-user MSI | Feasible future option with explicit ownership, repair/uninstall and downgrade policies. Installer identity, upgrade transactions and clean-machine tests are not implemented. MSI can support per-user installation. |
| MSIX | Requires a deployment/signing/identity plan and storage-virtualization compatibility validation. No signing identity is configured and no MSIX is produced. |

The workflow can build reviewed tag packages as Actions artifacts. Publishing
a GitHub Release requires an explicit manual workflow invocation with its
publish option; pushing a tag alone does not publish a release. No tag,
release, installer or deployment is created by this milestone. A future
updater needs reviewed authentication, signing, consent and rollback.

The build is reproducible from pinned Rust/dependencies and documented SDK
prerequisites. Byte-identical executables across different machines/SDKs
are not established. Identical packaging inputs use fixed ZIP entry times
and ordering; the packaging tests compare local archive hashes.

Primary references: [WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution),
[Windows Installer per-user context](https://learn.microsoft.com/en-us/windows/win32/msi/installation-context),
and [MSIX signing requirements](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview).
