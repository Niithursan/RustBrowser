# Windows package engineering

| Technology | Assessment |
| --- | --- |
| Inno Setup 6.7.4 | Selected. Pinned official compiler; native x64 payload, per-user integration, no runtime chaining/framework migration. Controlled installs verify contents, shortcuts, registration, ownership/version/path refusal, upgrade and removal. |
| WiX / per-user MSI | Windows Installer supports per-user context, repair and transactions. Feasible, but needs component/upgrade identities and additional tool maintenance. Not implemented or claimed tested. |
| MSIX | Needs approved identity/signing/deployment and profile/virtualization validation. No signing identity exists; self-signed trust import/developer-mode workarounds are inappropriate for normal users. |
| Portable ZIP | Retained: no registration, fixed allowlist and deterministic entry ordering/timestamps. |

Primary references: [Inno downloads/publisher](https://jrsoftware.org/isdl.php),
[privileges](https://jrsoftware.org/ishelp/topic_setup_privilegesrequired.htm),
[running applications](https://jrsoftware.org/ishelp/topic_setup_appmutex.htm),
[closure defaults](https://jrsoftware.org/ishelp/topic_setup_closeapplications.htm),
[RedirectionGuard](https://jrsoftware.org/ishelp/topic_setup_redirectionguard.htm),
[MSI contexts](https://learn.microsoft.com/en-us/windows/win32/msi/installation-context),
[MSIX signing](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview).

## Reproduction and ownership

`install-inno.ps1` verifies pinned official installer/compiler hashes and valid
Pyrsys signatures. `build-installer.ps1` stages through the existing portable
producer, compiles an exact eight-file payload, and emits two artifacts, their
SHA-256 sidecars and unsigned release metadata. No profile enters the package.
The source-owned six-frame globe icon has a stdlib maintenance generator/tests.

Production identity is `Niithursan.RustBrowser`; destination is
`{localappdata}\Programs\RustBrowser`. No custom `/DIR`, `[Run]`, associations,
runtime downloads, service or updater. Owned marker/uninstall files are separate
from profiles. Unowned files survive removal. `PrivilegesRequired=lowest` avoids
requesting elevation; deliberately elevated callers still retain their rights.

Tests compile the **same installer script** with GUID validation identity and
destination. Production registration, shortcuts and profiles are not targeted.
`ValidationId`/`ValidationRoot` are compile-time test settings; the production
builder does not accept them. Test installers are not uploaded as review artifacts.

```powershell
$baseline = .\tests\build_install_baseline.ps1 -OutputRoot C:\BuildScratch\baseline
.\tests\windows_installer.ps1 -Executable target\release\rust-browser.exe `
  -BaselineExecutable $baseline.executable -OutputRoot C:\BuildScratch\install-tests `
  -Compiler C:\BuildScratch\inno\inno\ISCC.exe -SourceCommit (git rev-parse HEAD)
```

The actual pinned v0.12 source builds in a **separate** target: sharing one target
reproduced stale workspace build-script reuse. The candidate is never replaced.
PR CI separately caches this immutable fixture by commit/OS/architecture/Rust.
Release application builds remain clean/uncached. The v0.12 binary is repackaged
by the test harness because no historical v0.12 installer exists; this does not
certify an unknown old installer.

Tests compare bytes around lifecycle operations and use current typed readers to
check sessions/pins/settings, schema-3 folders/bookmarks/history/permissions, opaque
site-data and a download sentinel. CLI launch checks the loader/version; actual
GUI validation is recorded separately. A running legacy binary is tested using
its own disposable profile and retained process identity.

Inno's cleanup child outlives the original uninstaller. Tests wait for owned cleanup
before reinstallation. A pre-existing empty directory is left intact; no recursive
deletion forces a pass. Windows 10, clean-machine/standard-user prerequisite behavior,
mid-copy kill, power loss and full-disk transactions need separate validation.

See [installation/recovery](../../INSTALL_WINDOWS.md),
[update security](UPDATE_SECURITY.md), [validation](../../VALIDATION.md) and [CI](../../CI.md).
