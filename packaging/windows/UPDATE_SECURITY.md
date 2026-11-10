# Update trust and installation boundaries

## Decision

Updates remain **manual**. No signing certificate/key, authenticated metadata root,
rotation policy or update service exists. A version-check UI is deferred with that
infrastructure. No update download, execution, installation, task, service or feed
is added. GitHub Releases transports reviewed bundles; HTTPS authenticates GitHub's
connection, not the publisher of an unsigned executable. Replacing an asset and
its co-hosted checksum defeats such integrity checks. The compiler's valid Pyrsys
signature authenticates that tool, not its generated unsigned application packages.

## Threat model

| Input / threat | Control | Limit |
| --- | --- | --- |
| Corrupted/truncated artifacts | Length/SHA-256 verifier, before manual launch | Attacker-controlled matching metadata provides no authenticity |
| Malformed/future metadata, path traversal, duplicate asset names | Bounded JSON, supported fields/schema/storage/architecture, exact two filenames, ordinary/reparse-free paths | PowerShell JSON semantics; not a signed canonical format |
| Accidental downgrade/reinstall | Stable canonical version checks, installed binary/marker, explicit same-version repair | Mutable local state is not cryptographic anti-rollback or freshness |
| Overwriting unowned data | Fixed per-user destination, ownership marker, profile/reparse checks, exact payload | Not a boundary against same-account malware, hard links or path-replacement races |
| Running old/new binaries | Lifetime mutex plus non-writing file-use probe, ordinary errors preserved | Mutex is session scoped; not an atomic cross-session replacement protocol |
| Uninstall destroys data | Exact owned files/integration; no recursive profile/download/app-tree removal | User-added files remain and can require manual review before reinstall |
| Failed/interrupted upgrade | Preflight retains previous bytes; manual package/closed-profile recovery | No verified mid-copy/power-loss/full-disk transaction or automatic rollback |
| Runtime supply chain | Microsoft instructions/presence checks; no runtime download | Presence is not health/currentness/authenticity proof |
| Automation misuse | Approved stable existing tag, Cargo version/source/main ancestry, exact five-file bundle, manual publish input | Repository administrators/release credentials remain trusted; no signing authority |

`asInvoker`, per-user Setup and the marker are not sandboxes. Remote content gains
no bridge/updater authority. WebView2, private contexts, permissions, TLS and external
protocol restrictions remain intact. Servo stays fixture-only.

## Before an automatic updater

Adopt a reviewed authenticated design such as TUF, with an independently distributed
root, protected offline/root and delegated release keys, rotation/revocation,
thresholds, freshness and persisted trusted version state. Bind version, architecture,
profile compatibility, artifact lengths and hashes. Test replay/freeze/downgrade,
compromise and interruption. Define authorized repair/rollback and user consent.

Obtain an approved Windows code-signing identity/service, protect credentials outside
Git and define expected publisher, chain/timestamp/revocation verification. Signing
and authenticated update metadata serve different purposes; co-hosted checksums
replace neither. No self-signed trust root, certificate import, security bypass or
embedded private key is introduced. SmartScreen reputation is not guaranteed by signing.

Primary references: [Authenticode](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/authenticode),
[MSIX signing](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview),
[TUF specification](https://theupdateframework.github.io/specification/latest/),
[GitHub Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases).
