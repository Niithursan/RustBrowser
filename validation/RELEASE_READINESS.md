# Windows release readiness (v0.15 preparation)

This is preparation for a release candidate, not a declaration of one. No release,
tag, publication or deployment is authorized. The [criteria](RELEASE_CRITERIA.md)
are the proposed acceptance baseline; [owner checks](OWNER_CHECKLIST.md) remain open.
WebView2 stays production and Servo remains experimental/fixture-only.

After PR #19 merged at `13ff603`, Windows 11 x64 is the initial qualification
priority. Technical qualification is separate from declaring/tagging/publishing
an RC, which remains unapproved. The [static CRT investigation](STATIC_CRT.md)
is opt-in; production linking and Setup's prerequisite checks remain unchanged.
A safe owner bundle now supplies an explicit profile launcher as well as a
validation installer identity. Neither passing automation nor preparing the bundle
marks human acceptance as passed.

The pinned static experiment completed a fresh 1800-second controlled soak:
178 churn cycles, zero navigation errors, responsive final switches, clean
shutdown and exit of observed children. The earlier interrupted run is excluded.
Its 41-visit survey and nine-site burst were clean except three deliberate DNS/TLS
failures; OpenStreetMap's HTTP 429 remains a known response. Memory/CPU details and
unavailable samples are in VALIDATION.md. This is not a matched dynamic comparison
or a human/device acceptance pass. Qualification and RC approval remain open.

## Recorded local evidence

| Area | Evidence and limit |
| --- | --- |
| Existing functionality | Profile path `%LOCALAPPDATA%/RustBrowser/v0.1`, session 1 and SQLite schema 3 remain unchanged; pinned upgrade baseline is v0.14 `b4f0c23` |
| Toolbar recovery | Browser-process and renderer failure reproduced before fixing; four passing native runs, five caught product mutants; recovery bounded to three attempts per two minutes |
| Public-site survey | 41 addresses, zero engine/renderer exits, dumps and timeouts; three deliberate DNS/TLS failures diagnosed; OpenStreetMap HTTP 429 remains a known site response |
| Burst closure | Two nine-site runs with 20 filler tabs: no exits, dumps or timeouts |
| Interrupted Setup | Two 32-case fresh/upgrade timing sweeps repaired every case, including completed-process controls; 24 and 26 cases respectively recorded killed owned processes; no uninstall leftovers; synthetic profile hashes/semantics preserved |
| Keyboard/layout | Three native keyboard/layout runs and four caught mutants; DPR/CSS emulation is distinct from physical Windows DPI |
| Windows accessibility metadata/input | Three foreground-guarded UIA walks: 24 initial/33 expanded controls, 29 distinct controls reached; not a spoken screen-reader test |
| Distribution | Portable and per-user installer retained; validation identities only; unsigned metadata/checksums provide integrity, not independent authenticity |
| CI economy | Whole-change classification, packaged-document protection, stable aggregate and mandatory audit retained; docs-only hosted skip is not yet exercised; no saving claimed |

The final full local gates and exact-head hosted status are recorded in
[VALIDATION.md](../VALIDATION.md) and the final pull-request delivery record.
The public survey, interruption sweep and physical/foreground checks are manual,
not recurring CI workloads. Their raw data stays outside Git.

## Not established or requiring an owner decision

| Area | Current state |
| --- | --- |
| Clean machine / Windows Sandbox / VM | Not available; Sandbox is not enabled. No Windows feature or VM was enabled/created |
| Windows 10 | Not tested; a Windows-11-only 1.0 scope is proposed, not approved or implemented |
| Standard user outside Administrators | Not separately tested; non-elevated local token is only partial evidence |
| Missing WebView2 | No missing-runtime host test; existing checks/instructions remain; no automatic runtime installation |
| Missing Visual C++ runtime | Production EXE remains dynamic; Setup checks both VCRUNTIME140 DLLs. The opt-in static EXE has no direct/delay CRT imports, but clean-host/runtime-chain verification and adoption approval remain open |
| Human installer wizard / icon / taskbar review | Owner checklist, not established by silent installation or resource checks |
| Narrator / contrast themes / physical IME / native mixed DPI | Still owner checks; renderer emulation and UIA do not replace them |
| Real sign-in / camera / microphone / video call | Owner-controlled test-account/device checks outstanding |
| Power loss / full disk / every interruption timing | Unverified; controlled process termination is only a tested subset |
| 30-minute v0.15 soak | Pinned static experiment completed one fresh controlled run; dynamic v0.15 was not measured in a matched soak. Finite results do not establish leak freedom or human/media/device behavior |
| Signing / authenticated updates | No identity/root/service; no signing, updater or invented key added |
| Second-launch address hand-off | Not approved; existing already-running behavior retained |

Approval of the proposed dispositions is separate from approval of this milestone's
implementation. Do not treat automated success as owner acceptance or permission to
tag/publish. See [installation/recovery](../INSTALL_WINDOWS.md),
[update security](../packaging/windows/UPDATE_SECURITY.md) and [CI](../CI.md).
