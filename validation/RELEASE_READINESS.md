# Windows release readiness (v0.20 development)

This is preparation for a release candidate, not a declaration of one. No release,
tag, publication or deployment is authorized. The [criteria](RELEASE_CRITERIA.md)
are the proposed acceptance baseline; [owner checks](OWNER_CHECKLIST.md) must be
repeated on the exact next candidate. They remain open for v0.16.
WebView2 stays production and Servo remains experimental/fixture-only.

## v0.22.2 release candidate (2026-11-23)

The v0.22.1 tag build passed (run 45236099610), but the owner's publish run (45239524988) stopped when `native_newtab_tiles_and_background` failed once; it was not published, and tags are never moved. The owner asked for a new release candidate after the test fixes' checks passed. The candidate is `main` at `cdf2bb6` (v0.22.1) plus PR #73 (two native tests wait for state instead of fixed delays; test code only) plus this pull request: version 0.22.2 and its release notes. The browser is unchanged from 0.22.1, the library schema (5) and session format are unchanged, and the installer upgrade check still starts from the published v0.21.0 (`c7c9e15`).

| # | Status |
| --- | --- |
| A1 | Local checks on this branch are listed in the pull request; the native suites and installer lifecycle run in its CI, and the release workflow repeats everything on the tag |
| A2 | No known open data-loss, profile-corruption or crash defect |
| A3 | Pending CI: upgrade from the published 0.21.0; no schema change |
| A4, A5 | Not run for this candidate (public-site survey, 30-minute soak); the browser is unchanged from the 0.22.0 candidate |
| A6 | The owner checklist passed on the 0.22.0 candidate; 0.22.2 changes only the version, notes and two tests |
| A7 | Section B dispositions unchanged; unsigned builds and no updater remain accepted risks |
| A8 | README, CHANGELOG, RELEASING and `release-notes/v0.22.2.md` describe this candidate |
| A9 | To be recorded from the tag run |

The `v0.22.2` tag goes on this pull request's merge commit. Tagging and publishing remain the owner's decisions (see RELEASING.md). Nothing has been tagged or published.

## v0.22.0 release candidate (2026-11-22)

The owner asked on 2026-11-22 for `main` to become a release candidate. The candidate
is `main` at `c062982` (PR #70) plus this pull request: version 0.22.0, its release
notes, the vertical-tab row fix and an installer upgrade check that now starts from
the published v0.21.0 (`c7c9e15`). The library schema (5) and session format are
unchanged from 0.21.0, so upgrading runs no migration. Linux work is on a separate
branch and is not part of this candidate.

Criteria status for this candidate:

| # | Status |
| --- | --- |
| A1 | Partly passed locally on this branch: fmt, strict Clippy, every regular test (193, with the 59 native tests left to CI), the 27 workflow script tests, `cargo audit` (no advisories) and a clean release build reporting 0.22.0. The native suites and installer lifecycle run in this pull request's CI; the release workflow repeats everything on the tag |
| A2 | No known open data-loss, profile-corruption or crash defect |
| A3 | Pending CI: the installer test now upgrades from the published 0.21.0 Setup and payload and checks user data is kept; no schema change |
| A4 | Not yet run for this candidate (public-site survey) |
| A5 | Not yet run for this candidate (30-minute churn soak) |
| A6 | Pending: owner checklist on a bundle of the exact candidate, especially the new items 39 to 42 and the corrected item 33 |
| A7 | Section B dispositions unchanged; unsigned builds and no updater remain accepted risks |
| A8 | README, CHANGELOG, RELEASING and `release-notes/v0.22.0.md` describe this candidate |
| A9 | To be recorded from the owner bundle's `owner.json` and the tag run |

The `v0.22.0` tag goes on this pull request's merge commit, because the publish job
reads `release-notes/v0.22.0.md` from the tagged commit. Declaring the candidate
final, tagging and publishing remain the owner's decisions (see RELEASING.md).
Nothing has been tagged or published.

## v0.21.0 release candidate (2026-11-18)

The owner asked for a release candidate on 2026-11-18 and chose **v0.21.0 as the
first published release**. The candidate is `main` at `c41bf6e`. Technical
qualification is complete locally (see the first section of VALIDATION.md): every
gate of the release workflow's package job, the installer upgrade from 0.20, a
30-minute soak and the public-site survey with a burst close all passed, with no
engine exit, crash report or navigation error. Owner report, 2026-11-18: the owner
checklist passed in full ("all passed") on bundle `o-656195c3`, version 0.21.0, source
`c41bf6e`, executable SHA-256
`1010a6884773be5d0c60e8ef4f703735e2ffd83c410b48f2a2a4ab4676ca8ad9`. These results
cover that build only.

Criteria status for this candidate:

| # | Status |
| --- | --- |
| A1 | Passed locally on `c41bf6e`; the release workflow repeats it on the tag |
| A2 | No known open data-loss, profile-corruption or crash defect |
| A3 | Passed: upgrade from 0.20 keeps user data; schema 5 migration is one-way and documented in INSTALL_WINDOWS.md |
| A4 | Passed: 38 of 41 loaded, 3 intended failures, burst 9 of 9, zero exits and dumps |
| A5 | Passed: 30-minute churn soak, 178 cycles, no errors, clean shutdown |
| A6 | Passed by owner report (2026-11-18): every checklist item on bundle `o-656195c3`, including the clean install in a new Windows user account (item 30) and the new items 35 to 38 |
| A7 | Section B dispositions unchanged from RELEASE_CRITERIA.md; unsigned builds and no updater remain accepted risks |
| A8 | README, CHANGELOG, VALIDATION, INSTALL_WINDOWS and the release notes describe this candidate |
| A9 | Source commit, toolchain (Rust 1.99.1/MSVC), executable and Setup hashes are recorded in VALIDATION.md and the bundle's `owner.json` |

The `v0.21.0` tag goes on the merge commit of the pull request that adds these
release notes and records (the publish job reads `release-notes/v0.21.0.md` from the
tagged commit). That commit changes only documentation after `c41bf6e`, so the
qualified code is unchanged; the release workflow rebuilds and retests it from the
tag. Declaring the candidate final, creating the tag and running the publish
workflow are the owner's decisions after the owner checklist (see RELEASING.md).
Nothing has been tagged or published.

### v0.21.0 published (2026-11-19)

The owner published **Rust Browser v0.21.0**, the first release, with release run
44452459369 (workflow_dispatch, publish on) at 2026-11-19T03:54:05Z:
https://github.com/Niithursan/RustBrowser/releases/tag/v0.21.0. Tag `v0.21.0` points
at `c7c9e15990ea16de3d07e49c013141d294a78893`. Not a draft or pre-release; five assets.
Checked after publication: the release text is identical to
`release-notes/v0.21.0.md` at the tag; both downloaded files match their `.sha256`
sidecars and the metadata (ZIP
`275b44b7e81e599dc6d7144103cfe9bab03ddc3a3d880f453fe3a1f3c0c91f4e`, Setup
`c85ef1c990d428dcf8d21aa22d9ab8f4e2742b455088a32e98ac3175861c68ed`); the metadata
records version 0.21.0, source `c7c9e15`, unsigned; `verify-release.ps1` from the tag
passes. The publish run rebuilt from the tag, so its hashes differ from the tag run's
artifact below; each set matches its own metadata.

### v0.21.0 release package (tag run 44447568537)

The tag `v0.21.0` points at `c7c9e15`. Its release run 44447568537 passed the package
job (publish skipped). The downloaded artifact has exactly the five expected files;
both SHA-256 sidecars match (ZIP
`db82c5e5172ef78f3e742ddd02ea6f85cd970828bc981722341b7ad922c739cc`, Setup
`a5fe03444a23f650ac7b718ee4102961f9bbaf677933892f81baee17a6f25b33`);
`verify-release.ps1` from the tag passes; the metadata records version 0.21.0 and
source commit `c7c9e15`. Known inaccuracy, accepted by the owner for v0.21.0: the
metadata says `profile_library: 3` while the build uses library schema 5 (a value
hard-coded since v0.15 in both the producer and the verifier). Nothing reads it apart
from the verifier; it is fixed for the next release, with a test.

## Current milestone and exact prior-build owner results

**Release decision, 2026-11-12: v0.20.0 will not be tagged or released.** The first
release is postponed until the owner says it is ready. `release-notes/v0.20.0.md` and
RELEASING.md stay as prepared drafts. Nothing has been tagged or published.

Owner checklist on bundle `o-a47ce755`, version 0.20.0, built from PR #26 head
`0590beb` (EXE SHA-256
`edff5c726a13c67adbcc64728d4d11d88bc7e54d4f92dce7c3029224b0e1f1e1`), 2026-11-12:
bundle setup, step 28 (uBlock Origin Lite: checksum verified, ads blocked, ads back
with the extension off, a private window not filtered; site: YouTube) and step 29
(keyboard, mouse, Narrator, contrast theme) passed by owner report. Step 30 (a clean
install in a new Windows user account) was not tested. These results cover that build
only.

v0.20 branches from verified main `383a6de` (CI 43113399661 and security 43113399666
passed). It adds browser extensions (uBlock Origin Lite tested for ad blocking), a
new layout with card tabs and one menu, and prepares v0.20.0 as the **first
published release**: release notes, RELEASING.md and a release workflow that runs
every CI suite and publishes only the reviewed notes. Only the owner creates the tag
and approves publishing; nothing has been tagged or published. All 44 local gates
passed on `ea41c2f` and 14 mutation checks were caught; see VALIDATION.md. Owner
checklist steps 28 to 30 (including the clean install in a new Windows user account)
are new and not yet done. Builds remain unsigned. No release candidate is declared.

v0.19 branches from verified main `efd1a2a` (CI 43091111950 and security
43091111977 passed). It adds opt-in password saving and form autofill through
WebView2 (form autofill is now off by default), two clearing choices and bookmark
import from an exported HTML file, and fixes silent loss of windows when the session
is too large. All 39 local gates passed on `1336439` and 14 mutation checks were
caught; see VALIDATION.md. No release candidate is declared.

Owner checklist on bundle `o-b60800e5`, version 0.19.0, built from PR #25 head
`abdff7b` (EXE SHA-256
`6e59d53d49fbe99cbca28d5caaca8a0b2d0bc7aefaac12f5f4c67da755e445e9`), 2026-11-11:
steps 22 to 27 all passed by owner report, with the bundle's disposable profile.
Passwords used the public demo login at the-internet.herokuapp.com; autofill used
made-up details on httpbin.org/forms/post, submitted because autofill saves entries
on submit; a real Firefox export was imported. The validation install and its
profile were removed. Folder mapping on import is approved as it is. These results
cover that build only.

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
