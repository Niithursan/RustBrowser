# Windows release readiness (v0.19 development)

This is preparation for a release candidate, not a declaration of one. No release,
tag, publication or deployment is authorized. The [criteria](RELEASE_CRITERIA.md)
are the proposed acceptance baseline; [owner checks](OWNER_CHECKLIST.md) must be
repeated on the exact next candidate. They remain open for v0.16.
WebView2 stays production and Servo remains experimental/fixture-only.

## Current milestone and exact prior-build owner results

v0.19 branches from verified main `efd1a2a` (CI 43091111950 and security
43091111977 passed). It adds opt-in password saving and form autofill through
WebView2 (form autofill is now off by default), two clearing choices and bookmark
import from an exported HTML file, and fixes silent loss of windows when the session
is too large. All 39 local gates passed on `1336439` and 14 mutation checks were
caught; see VALIDATION.md. Owner checklist steps 22 to 27 are new and not yet done.
No release candidate is declared.

v0.16 branches from verified main `cb3d81d`; Windows CI 42931608985 and security
42931608910 passed. It adds content-only native find, zoom and print, and toolbar
recovery follow-ups. All 31 local gates passed on initial qualified head ed90da2:
81 regular tests, 12 separate native suites, three daily runs, both builds, Python
and PowerShell fixtures, imports, portable/installer/metadata/owner safety tests and
audit. Nine compiled mutation runs across three native workloads rejected eight
defect types. The final evidence-only head repeats every local gate before the
second push; exact final-head hosted results are recorded in PR #22 and delivery.
Details: [v0.16 record](V016.md). No RC is declared.

The owner's 2026-11-10 report passes checklist items 1 to 8 for **one build**:
bundle `o-d8ee96f5`, source `062c93dc7688ef296345f20ac7b1e39fd7c06fae`, static
CRT experiment, EXE SHA-256
`bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58`.
It used the validation installation identity and only `Launch.ps1 -Installed`
with the bundle's disposable profile. Installer, identity, Narrator, contrast,
IME, sign-in/restart, video-call page and mixed-scale monitor checks passed by
owner report. They do not cover later bytes or the new v0.16 tools. A6 must be
repeated on the exact candidate. Static CRT stays opt-in; criteria approval,
clean-machine testing and publication authorization remain the owner's decisions.

Owner checklist steps 12 to 16, 2026-11-11, bundle `o-4349a481`, version 0.17.0,
built from PR #23 head `25a94d0` (EXE SHA-256
`72b9a9f8a8806f829a8ca3a65b6424b26f22be8ba841396d289d16fa6c26a95c`). All passed by
owner report: the default was chosen in Windows Settings through the validation
entry; links from Discord and from a mail or PDF app opened as tabs in the running
window; a second launch handed off with no second window; Ctrl+N did nothing
unexpected; the address-field menu showed exactly Cut, Copy, Paste and Select all,
and paste was one line. Firefox was restored as the default and the validation
install was removed. These results cover that build only.

Owner checklist on bundle `o-9ad95301`, version 0.18.0, built from PR #24 head
`42e3f45` (EXE SHA-256
`db97e72331f2ae708396eb7103ad5cf2e37e0e32f109424a322b3a758fc84a06`), 2026-11-11:
steps 12, 13, 14, 17, 18, 19, 20 and 21 all passed by owner report. Steps 15 and 16
were not re-run (they passed on 0.17 bundle `o-4349a481`; 0.17's step 15 checked
that Ctrl+N did nothing, so the 0.18 New window step was not checked by hand, though
steps 17 and 18 used Ctrl+N). Step 20 used two monitors at the same scale. Firefox
was restored as the default and the validation install was removed. More > Exit is
approved as named. These results cover that build only.

## Historical v0.15 technical evidence

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
| Human installer wizard / icon / taskbar review | Owner passed items 1 and 2 on exact bundle o-d8ee96f5 only; repeat on the next candidate |
| Narrator / contrast themes / physical IME / native mixed DPI | Owner passed items 3, 4, 5 and 8 on that exact build only; new bytes remain unverified |
| Real sign-in / camera / microphone / video call | Owner passed items 6 and 7 on that exact build only; repeat on the next candidate |
| Power loss / full disk / every interruption timing | Unverified; controlled process termination is only a tested subset |
| 30-minute v0.15 soak | Pinned static experiment completed one fresh controlled run; dynamic v0.15 was not measured in a matched soak. Finite results do not establish leak freedom or human/media/device behavior |
| Signing / authenticated updates | No identity/root/service; no signing, updater or invented key added |
| Second-launch hand-off and links from other apps | Implemented in v0.17; owner passed checklist steps 13 and 14 on exact bundle o-4349a481 only |
| Default-browser registration | Implemented in v0.17 (per-user Setup); owner passed checklist step 12 on exact bundle o-4349a481 only |

Approval of the proposed dispositions is separate from approval of this milestone's
implementation. Do not treat automated success as owner acceptance or permission to
tag/publish. See [installation/recovery](../INSTALL_WINDOWS.md),
[update security](../packaging/windows/UPDATE_SECURITY.md) and [CI](../CI.md).
