# v1.0 release candidate criteria (proposed)

**Status: proposed acceptance baseline, not RC approval.** Owner direction after
PR #19 adopts this as the baseline for investigation, prioritizing Windows 11 x64.
Individual risk dispositions and acceptance results still require review.
Written in v0.15 (2026-11-10) from the state recorded in
[RELEASE_READINESS.md](RELEASE_READINESS.md) and [VALIDATION.md](../VALIDATION.md).

A release candidate is a build the owner would install on their own daily machine
and recommend to someone else. It is not the final 1.0.0: it is the build that
becomes 1.0.0 if nothing new is found while using it.

## A. Criteria every candidate must meet

These are checked again on the exact candidate commit.

| # | Criterion | How it is checked |
| --- | --- | --- |
| A1 | Every automated gate passes on the candidate commit, locally and in GitHub Actions | fmt, strict Clippy, all tests, every native WebView2 suite in its own process, three daily-driver runs, Python and PowerShell fixture tests, package and installer lifecycle, release metadata verifier, `cargo audit` |
| A2 | No open defect that loses user data, corrupts the profile or crashes the browser in normal use | Issue list and the latest survey; any such defect blocks |
| A3 | Profile path, session format and SQLite schema stay readable by the candidate, and an upgrade from the previous supported production version keeps the closed test profile byte for byte | Pinned-source installer upgrade test; distinguish a prior validated build from a published release |
| A4 | A public-site survey with burst close shows no engine exit and no crash dump, and every failure is explained | `validation/site-survey.ps1` on the candidate |
| A5 | A 30-minute soak passes without navigation errors, unbounded memory growth or an unclean exit | Existing soak procedure |
| A6 | The owner checklist ([OWNER_CHECKLIST.md](OWNER_CHECKLIST.md)) passes on the candidate | Owner's report |
| A7 | Every item in section B has the disposition the owner approved | This file, updated |
| A8 | README, CHANGELOG, VALIDATION and INSTALL_WINDOWS describe the candidate as it is, including known limitations | Review |
| A9 | Candidate evidence identifies immutable source, toolchain, build flags and executable hashes | Local qualification manifest, exact-head CI and reviewed evidence; no tag is needed to investigate or technically qualify a build |

Technical qualification and publication authorization are separate. Passing A1 to
A9 supports a technical recommendation; it does not authorize declaring an RC,
creating a tag, running a publication workflow or publishing a release. Those need
explicit owner approval after acceptance review. A separately approved publication
must use the reviewed `main` commit and `release.yml`; existing safeguards remain.

## B. Disposition of every "Not established" item

Each item from RELEASE_READINESS.md is sorted into one of three groups:

- **Blocker:** must be fixed or verified before a candidate.
- **Owner check:** only a person, or hardware this machine lacks, can verify it.
  The owner verifies it once per candidate (see OWNER_CHECKLIST.md where it fits).
- **Accepted risk:** shipped as is and documented, for the stated reason.

| Item | Proposed disposition | Reason |
| --- | --- | --- |
| Missing Visual C++ v14 runtime | **Blocker** for the portable ZIP; already handled by Setup | Measured in v0.15: the release executable imports `VCRUNTIME140.dll` and `VCRUNTIME140_1.dll`, which Windows does not include (the Universal CRT it also uses is part of Windows 10 and later). Setup already checks for both and refuses with a message naming the Visual C++ Runtime. The portable ZIP has no such check: on a machine without the Redistributable, Windows shows a missing-DLL error before any browser code runs. Proposed fix, needing approval: link the C runtime statically (`-C target-feature=+crt-static`) so the dependency disappears for both, then confirm with the import list and a clean-machine run. |
| Clean machine or virtual machine | **Owner check** (blocker until done once) | The installer, first launch and missing-prerequisite paths have only run on this development machine, where every runtime is present. Windows Sandbox is not enabled here, and enabling it is a system change the owner has to make. One clean run per candidate is enough. |
| Installer wizard pages used by a person | **Owner check** | All automated runs are silent. The wizard (licence, notice, shortcut task, ready, finish) needs one person to click through it. The per-user folder is fixed. Checklist item. |
| Application icon and taskbar identity appearance | **Owner check** | Resources are verified by script; how they look is not. Checklist item. |
| Standard-user account without administrator membership | **Owner check**, or accepted risk if no such account exists | Setup is per-user and requests no elevation, and the local v0.15 installer runs used a non-elevated token from an administrator account (hosted CI runners are administrators). That is partial evidence; it is not the same as an account outside the Administrators group. |
| Windows 10 x64 | **Deferred validation**; prioritize Windows 11 x64 for initial v1.0 consideration | Owner direction after PR #19 prioritizes Windows 11. No Windows 10 test is claimed and existing build minimums are unchanged. Wider support needs separate evidence. |
| Missing WebView2 Runtime | **Proposed accepted risk**, subject to owner approval | Runtime availability must not be assumed for every Windows installation. Setup checks registration and refuses if absent; the browser has a startup message, reviewed in code but not tested on a missing-runtime machine. No runtime is downloaded or installed. Documented in INSTALL_WINDOWS.md. |
| Interrupted installation | **Accepted risk** for power loss and a full disk only; the kill case is now tested | v0.15 killed Setup at 16 points during a fresh install and 16 during an upgrade from 0.14 (`validation/interrupted-install.ps1`). Before the v0.15 installer fixes, a kill during the copy left a folder that Setup and the uninstaller both refused (6 of 6 fresh cases), and later leftovers made the next installation refuse. After them, re-running Setup repaired every case, uninstall left nothing, and the profile was unchanged (see VALIDATION.md). Power loss and a full disk cannot be produced safely here. The profile lives outside the installation folder and Setup never deletes it. |
| Code signing and SmartScreen reputation | **Accepted risk** for the candidate (owner decision: no signing identity exists) | Users see an unknown-publisher warning. SHA-256 sidecars and reviewed metadata detect corruption but do not identify the publisher. Documented in INSTALL_WINDOWS.md and packaging/windows/UPDATE_SECURITY.md. Revisit before a wider public 1.0. |
| Authenticated updates | **Accepted risk** (owner decision: no updater, no invented keys) | Upgrades stay manual and reviewed. No automatic update channel exists to attack. |

## C. Other open areas

These are not in RELEASE_READINESS.md "Not established" but matter for daily use.

| Area | Proposed disposition | Reason |
| --- | --- | --- |
| Screen reader (Narrator) on the toolbar | **Owner check** | v0.15 adds automated UI Automation name and role checks; that is not a screen reader experience. |
| Windows contrast themes | **Owner check** | Forced colours are emulated in a fixture; a real contrast theme is not. |
| IME text entry in the address bar | **Owner check** | Cannot be automated honestly here. |
| Moving the window between monitors with different scale | **Owner check** | Both displays here are 1920x1080; changing display scale is a system setting. |
| One real sign-in and one video call page | **Owner check** | The survey never enters credentials and grants no permissions. |
| Single-instance hand-off of an address to a running window | **Accepted limitation** (owner decision 1: not approved) | The "already running" message stays. |
| Servo engine | **Out of scope** for 1.0 | Experimental only. |

## D. Approval

The owner reviews each disposition in B and C and the actual acceptance results.
Until that review is complete the build remains unapproved. Qualification evidence
is not permission to declare, tag or publish a release candidate.
