# Windows release readiness (v0.14)

This records what is and is not established for a Windows release of 0.14.0. No
release, tag or publication was made in this milestone.

## Established by tests in this repository

| Area | Evidence |
| --- | --- |
| Build | `cargo build --locked --release` on Windows x64, Rust 1.99.1 |
| Quality gates | fmt, strict Clippy, 79 regular tests, eight native WebView2 suites, three daily runs |
| Dependencies | `cargo audit --deny warnings`, pinned cargo-audit 0.22.2 |
| Portable ZIP | Deterministic bytes, fixed eight-entry allowlist, checksums, manifest, unsafe-output refusals, packaged-link check |
| Installer | Fresh install, v0.13 → v0.14 upgrade, profile and download bytes preserved, metadata, Start Menu shortcut, optional desktop shortcut, uninstall, reinstall, refusal of downgrade, invalid metadata, running browser, reparse and profile destinations |
| Release metadata | Unsigned JSON, verifier with 36 refusal cases |
| Startup after upgrade | Installed 0.14.0 `--version` launch; separate GUI start, close and restore of the release executable |
| Workflows | `ci.yml` runs all of the above on pull requests; `release.yml` repeats them from a clean build for an approved tag and publishes only on manual approval |

Installer tests use a GUID validation identity and directory. The production
identity, a real installation and a real profile were never touched.

## Not established

| Area | State |
| --- | --- |
| Windows 10 x64 | Not tested |
| Clean machine or virtual machine | Not tested; none was available |
| Standard-user account without administrator membership | Not tested separately; Setup requests no elevation |
| Missing WebView2 Runtime | Not tested. The browser shows a startup message; Setup does not install the Runtime |
| Missing Visual C++ v14 runtime | Not tested. Windows reports the missing DLL before any browser code runs |
| Installer wizard pages used by a person | Not tested; all runs were silent |
| Application icon and taskbar identity appearance | Icon resources are checked by script; appearance was not inspected by a person in v0.14 |
| Interrupted installation (kill, power loss, full disk) | Not tested |
| Code signing, SmartScreen reputation | None. The installer and executable are unsigned |
| Authenticated updates | None. SHA-256 sidecars detect corruption; they do not identify the publisher |

## Decisions that need the owner

1. Signing identity and trusted update root, before any automatic update work.
2. Whether a second launch should hand its address to the running window. That needs
   a local interface between processes and was not added without approval.
3. A Windows 10 machine and a clean virtual machine for prerequisite and wizard tests.

See [INSTALL_WINDOWS.md](https://github.com/Niithursan/RustBrowser/blob/main/INSTALL_WINDOWS.md),
[packaging notes](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/README.md),
[update security](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/UPDATE_SECURITY.md) and
[VALIDATION.md](https://github.com/Niithursan/RustBrowser/blob/main/VALIDATION.md).
