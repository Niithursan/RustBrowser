# Windows installation, upgrades and removal

Rust Browser 0.15.0 supports Windows 10 (1809 or later)/11 **x64**. Validation uses
Windows 11; Windows 10 and a clean machine remain unverified. The installer and
application are **unsigned**. No automatic updater, service, scheduled task,
runtime download, protocol association or default-browser change is installed.
Do not run the application or installer elevated.

## Prerequisites

Obtain the [Microsoft Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
and [latest supported x64 Visual C++ v14 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)
from Microsoft if absent. The latter must be at least as recent as the MSVC tools
used for the application. Edge alone is not the Runtime. Do not download loose DLLs.

Setup checks WebView2 registration and presence of `VCRUNTIME140.dll` and
`VCRUNTIME140_1.dll`; this does not prove runtime health/currentness. It downloads
or installs neither prerequisite. Microsoft prerequisite installation can separately
require administrator consent. Missing imported DLLs can fail in the Windows loader
before the browser's own error dialog.

## Verify, install and launch

1. Obtain an explicitly approved version from this repository's GitHub Releases,
   or build reviewed source. This development task publishes nothing.
2. Review its version and source commit. The v0.15.0 review bundle contains
   `rust-browser-v0.15.0-windows-x64-setup.exe`, the portable `.zip`, their two
   `.sha256` files, and `rust-browser-v0.15.0-windows-x64-release.json`.
3. Compare `Get-FileHash .\rust-browser-v0.15.0-windows-x64-setup.exe -Algorithm SHA256`
   with the reviewed checksum. Do likewise for the ZIP when using it. A co-hosted
   checksum detects corruption, **not independent publisher authenticity**.
   See the [update threat model](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/UPDATE_SECURITY.md).
4. Run the reviewed installer as your ordinary Windows account. Read its
   unsigned/prerequisite/profile notice and license. Installation is fixed at
   `%LOCALAPPDATA%\Programs\RustBrowser`, separate from browsing data. It creates
   a current-user Start Menu shortcut and Apps & Features entry. The desktop
   shortcut is optional and unchecked. The application is not auto-launched.
5. Launch **Rust Browser** from the Start Menu, optional desktop shortcut, or
   that directory's executable. Windows may show reputation warnings; the project
   does not claim signed/trusted status or advise bypassing Windows protections.

Custom `/DIR` destinations, unowned nonempty directories, recognized profiles
and reparse points/junctions are refused. Preserve and move conflicting files
yourself, or use the portable edition. Per-user installation is not a sandbox.

## Portable alternative

Extract the reviewed ZIP into a new ordinary versioned directory, for example
`%LOCALAPPDATA%\Programs\RustBrowserPortable\0.15.0`, and launch `rust-browser.exe`.
Do not extract into a profile, Windows directory, installer-managed directory or
a running version. The fixed file allowlist and `package.json` describe hashes,
source commit, version and unsigned status. Nothing is registered.

## Default browser

Setup (0.17 and later) registers Rust Browser for your Windows user as a browser
that can handle `http` and `https` links. It does not make itself the default:
Windows 11 asks you to choose. In Rust Browser open More (⋯) → Privacy & security →
**Make default browser**, which opens Windows Settings at Default apps on Rust
Browser's page, then set it for HTTP and HTTPS there. You can also open
Settings → Apps → Default apps and search for Rust Browser. The Settings deep link
needs Windows 11 21H2 or 22H2 with the April 2023 update, or later; older builds
open the general Default apps page.

The registration covers only `http` and `https`; local `.htm` and `.html` files
are not opened by Rust Browser. The portable ZIP registers nothing, so Windows does
not offer it as a browser.

Uninstall removes exactly what Setup registered: the `RustBrowserHTML` link type,
the `RustBrowser` browser entry and the `Rust Browser` value under
`HKCU\Software\RegisteredApplications`. If Rust Browser was your default, Windows
asks which browser to use next time you open a link.

## Upgrade without moving data

Close **all** browser windows and wait for WebView2 processes to exit. Back up
the closed profile. The default remains `%LOCALAPPDATA%\RustBrowser\v0.1`;
`RUST_BROWSER_PROFILE` selects an alternative. Keep application/profile folders
separate. Session format **1** and SQLite schema **3** are unchanged.

Bookmarks/folders, history, tabs/pins/active session, settings, permissions,
cookies and site storage stay in the profile. Downloads stay in their selected
destinations. Do not delete live locks, merge SQLite files or copy a running
profile. One normal instance owns each profile; private contexts stay isolated.

Run the reviewed newer installer over an installer-managed copy. Setup refuses
a newer installed binary/version marker and invalid metadata. A non-exclusive
v0.13 running marker and a non-writing mapped-file probe protect running copies,
including legacy v0.12 executables. No processes are terminated or scheduled for
replacement on reboot. The marker is Windows-session scoped, not an atomic updater.

Upgrading v0.13 to v0.14 changes no stored data format. After closing the last
window, the browser process now stays for about one second while it shuts the engine
down; wait for it before running Setup, which otherwise reports the browser as
running. If the browser is started while a normal window is already open for the
same profile, it shows "Rust Browser is already running" and changes nothing.

For existing portable users, close the old copy, install the new version
normally and launch it: both use the same separate profile. Do not point Setup at the portable folder.
Verify tabs/bookmarks/settings before removing old application files. Portable
upgrades use new sibling version directories.

## Uninstall and reinstall

Close the browser, then use **Settings → Apps → Installed apps / Apps & Features
→ Rust Browser → Uninstall**, or its installed uninstaller. It removes owned
application files, shortcuts and current-user registration. It never recursively
deletes the application tree. Ordinary user-added files remain and may make a future fresh
install refuse that directory. Installer state uses reserved names: `installation.ini`,
`uninstall/unins???.*` and `is-*.tmp` in the application/uninstall folders. Do not
store personal files under these names; Setup manages and cleans them. Wait for cleanup to finish before reinstalling.

For a portable copy, remove only the extracted application directory and shortcuts
after closing it. Both methods preserve profiles and downloaded files. Profile
deletion is a separate explicit action; uninstall offers no data-wipe option.
Reinstallation uses the same preserved profile.

## Failed upgrade and rollback

Preflight failures leave current application bytes intact. Resolve the prerequisite,
running-process or path/version problem, then retry. Do not alter version markers
to force a downgrade. Preserve unknown files.

Installation is **not a power-loss transaction**. v0.15 tested termination of owned
Setup processes during fresh installation and upgrade: rerunning the same reviewed
Setup repaired the tested partial installations while preserving the separate profile.
Ownership is recorded before copying; stale Setup temporaries and orphan uninstallers
are cleaned within the owned application directory. Power loss, full disk, reboot and
clean-machine rollback remain unverified. If an older or unrecognized partial directory
has no valid ownership marker, preserve and move only that application directory aside,
then reinstall into the absent fixed location. Keep the prior reviewed package
and a whole closed-profile backup for recovery. An older binary may reject newer
settings: restore a compatible profile backup before downgrading, preserving newer
data separately. Never test destructive recovery against a real profile.

## Manual artifact verification

Use the script from reviewed source with both assets in an ordinary directory:

```powershell
.\packaging\windows\verify-release.ps1 `
  -MetadataPath C:\Downloads\rust-browser-v0.15.0-windows-x64-release.json `
  -ArtifactDirectory C:\Downloads -CurrentVersion 0.14.0
```

It validates bounded metadata, stable canonical versions, fixed names, lengths and
SHA-256 hashes. Older/same versions are refused; intentional same-version repair
requires `-AllowSameVersion`. `-ExpectedMetadataSha256` checks a pin obtained
independently, but cannot establish its provenance. Output always says
`PublisherAuthenticated=False`. It downloads, extracts, executes and installs
nothing, and writes no browser data. Manual review/consent still precedes launch.
No in-app update check is added.

## Package reviewed source

With pinned Rust 1.99.1/MSVC, Visual Studio C++ tools and Windows SDK installed:

```powershell
cargo build --locked --release
$compiler = .\packaging\windows\install-inno.ps1 -Destination C:\BuildScratch\inno
$package = .\packaging\windows\build-installer.ps1 `
  -Executable target\release\rust-browser.exe -OutputRoot C:\BuildScratch\packages `
  -Version 0.15.0 -SourceCommit (git rev-parse HEAD) -Compiler $compiler
```

Use scratch paths outside Git/profiles. The bootstrap pins official Inno 6.7.4
hashes and checks valid Pyrsys installer/compiler signatures before execution.
This **does not sign Rust Browser**. Compiler installation is isolated per-user,
without shortcuts/associations; it is not bundled with the browser.

See [design/tests in the source repository](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/README.md),
[validation](VALIDATION.md) and [CI](https://github.com/Niithursan/RustBrowser/blob/main/CI.md).
Equal portable inputs produce equal ZIP bytes. Byte-identical
application/installer builds across machines/SDKs/paths/timestamps are not established.
PR CI uploads review artifacts. Tags package only; GitHub Release publication
continues to require an explicitly approved manual action.
## Isolated owner acceptance

For candidate acceptance, use the prepared validation bundle's `OWNER_BUILD.md`,
`owner.json` and `Launch.ps1`. Run `Launch.ps1 -VerifyOnly` before the portable
launch, or `Launch.ps1 -Installed` after that bundle's validation-only Setup.
Do not use the ordinary production review installer or open validation shortcuts
directly for an isolated-profile check. The launcher pins the executable hash and
explicit test profile. Keep the bundle at its prepared location; Setup's destination
is fixed. Neither qualification nor owner testing authorizes publication.
The opt-in static CRT experiment retains existing prerequisite checks/disclosures.
