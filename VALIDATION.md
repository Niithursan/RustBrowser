# Validation

## v0.18 multiple normal windows

Baseline `7a61d97` (PR #23); CI 43063434943 and security 43063435068 passed before
branching. Branch `feat/v0.18-multiple-windows`, draft PR #24. Version 0.18.0;
profile path, session format 1 (with the optional `other_windows` field) and SQLite
schema 3 unchanged; dynamic CRT. One Windows 11 Pro x64 machine, WebView2 157,
non-elevated session. Local results only in this section; hosted results for the
final head are recorded in the pull request.

### Reproduced defects and fixes

1. **Several windows traded focus forever.** Each window handled its Focused
   event by moving keyboard focus into its page. With three windows the requests
   chased each other; the event loop never went idle, pages stayed on
   "Connecting" and the routing suite timed out. Reproduced with a breadcrumb
   watchdog in the fixture (Focused(true) cycling 0, 1, 2 every few
   milliseconds). Fixed: a focus event only records the focused window and which
   tab may ask for permissions. The routing suite then passed.
2. **The wrong window ended up in front after restore.** Each restored window's
   toolbar took focus as it loaded (wry's `with_focused` default), so the last
   restored window became the most recent one; the next save would have swapped
   the main window and links would have gone to it. Reproduced by the restore
   suite (focus order [2, 1, 0]). Fixed: restored windows are created inactive
   with unfocused toolbars, and page focus is never moved inside a background
   window. The restore suite then passed.
3. **Exit saved the wrong main window (found by hosted CI).** CI 43085823286
   failed the restore suite on `10a51d8`: after More > Exit the third window was
   saved first (pins `[false, false]` instead of `[true, false]`). Hiding the
   windows lets Windows activate another one, and that focus event saved again.
   Local runs missed it because the background test process got no real focus
   events. The suite now delivers that focus event after Exit, which reproduced
   the same failure locally; focus events are now ignored once shutdown begins,
   and the suite passed twice. Removing the guard makes it fail again.
4. **Fixture only:** creating windows from `about_to_wait` wedged winit's event
   loop. Production creates windows only from events, so the fixtures send window
   requests through the event proxy as well.

### Automated evidence

All 36 local gates passed on `1aba4dc` and again on `335ba12` after the third fix
(`work/v18/production-gates.json`):
format, Clippy with `-D warnings`, 112 regular tests (the 24 native tests are
ignored there and run on their own below), debug and release builds,
16 native suites each in its own process (including the three new
`native_multiple_windows_*` suites), the second-launch suite (3 tests, including
the new killed-browser restore of two windows), three daily runs, process sampler,
launcher fixtures, experimental and production Python tests, dynamic imports,
portable package, installer lifecycle from a real 0.17 installation (pinned
`7a61d97`, executable SHA-256
`b27926cd71b9b18f31e0b7a3ebb3701b471672824513cefd75a6ca54e7f1b67c`), release
metadata, static-build restoration, owner bundle tests and `cargo audit`. The
installer suite checks that the real default-browser choice and registered
applications are unchanged.

New tests:
- `tests/session_windows.rs` (7): format 1 with and without `other_windows`,
  limits, invalid entries, size limit, and a 0.17-shaped reader loading a 0.18 file.
- `native_multiple_windows_routing_limit_close_reopen_and_handoff`: unique tab IDs
  and toolbars per window, navigation and real toolbar IPC stay in their window, a
  link goes to the focused window, a "new window" hand-off opens a window, the
  ninth window is refused with a notice, closing a window keeps the others and
  destroys its views only after the retirement delay, reopening restores its tabs,
  the save holds every window with the focused one first, Exit keeps all 8.
- `native_multiple_windows_engine_restart_toolbar_and_clearing`: an engine restart
  rebuilds every view of both windows and keeps tabs, order, pins and selection; a
  toolbar failure in the window without focus is recovered there only; clearing
  with retired views in both windows closes the other window and starts engine
  clearing only after every retired view and closed window is gone.
- `native_multiple_windows_restore_and_exit`: three saved windows return with their
  tabs, pins and selection, the main window focused, and Exit saves them again.
- `native_killed_browser_restores_every_window` (two processes): a killed browser
  with two windows returns with both at the next start, saved in the same order.

Mutation checks (one defect injected at a time, `work/v18/mutate.py`), all 10
caught: old focus handling, links to the last handled window, no window limit,
closed window destroyed at once, other windows not saved, engine restart of one
window only, clearing ignoring closed windows, toolbar failure handled in the
focused window, restored toolbars taking focus, and the v0.17 new-window hand-off.
The first run caught 8; the two survivors exposed weak checks in the engine suite,
which were strengthened (the toolbar failure now targets the window without focus;
the closed window is kept longer than the retired tabs, because clearing otherwise
retires both at the same moment). One check in that suite also compared view
addresses across tabs; the allocator reuses a freed address for another tab's new
view, so it now compares per tab, as the single-window restart suite does.

### Not tested here

Owner checklist steps 17 to 21 (three windows by hand, close and reopen, link to
the last-used window, two monitors, Narrator on a second window) are not done.
Drag between monitors, real taskbar grouping and Alt+Tab order were not observed
by a person. Downgrade to 0.17 with several windows was not run; it is documented
in INSTALL_WINDOWS.md from the format tests.

## v0.17 links from other apps, hand-off and default-browser registration

Baseline `96a70a2` (PR #22); CI 42945393777 and security 42945393781 passed before
branching. Branch `feat/v0.17-link-handling`. Version 0.17.0; profile path, session
format 1, SQLite schema 3 and dynamic CRT unchanged. One Windows 11 Pro x64 machine,
WebView2 157, non-elevated session. Local results only in this section; hosted
results for the final head are recorded in the pull request.

### Reproduced defects and fixes

| Defect | Before | After |
| --- | --- | --- |
| An argument that is not valid UTF-16 | Startup panicked (exit 101, "internal error") | Refused with a message, exit 2, no window or session |
| Unknown options such as `--disable-web-security` | Ignored; the browser started | Refused with a message, exit 2 |
| A bare word on the command line | Became a search | Only explicit http(s) links are accepted |
| Address field right-click | No menu at all (toolbar menus are disabled) | Cut, Copy, Paste and Select all for the address field only |

### Automated evidence

- Command line: 6 parser unit tests (registered form taken verbatim, quotes,
  backslashes, leading dashes, `javascript:`, `data:`, `file:`, UNC and drive paths,
  credential and deceptive user-info URLs, 16-link and 32,768-character bounds,
  invalid UTF-16) and `tests/command_line.rs`, which starts the real executable with
  hostile raw command lines and requires exit 2, the reason, and no session file.
- Hand-off library: 6 unit tests (format round trip, every malformed case, link
  re-validation, rate limit, trust rules, endpoint names).
- Hand-off transport: 8 tests on real pipes (accepted, malformed and wrong version,
  oversized, flood, hung browser, missing endpoint, squatted name by this process
  and by PowerShell, fake PowerShell server of the same user, DACL with exactly one
  entry for this user). Five mutants were caught: trusting a server without the
  executable check, no first-instance flag, no rate limit, accepting malformed
  messages, a DACL that also allows Everyone.
- Two real processes (`tests/single_instance.rs`, 3 of 3 runs): a second launch hands
  over a link and a new-tab request, a `javascript:` launch is refused before any
  contact, and the saved session is exactly the start tab, the link and the new tab;
  after the first browser is killed, the next launch takes over the profile and
  opens its link. Two mutants caught (no server; accepted without opening).
- Address-field menu (`native_address_field_edit_menu`, in-memory test clipboard):
  menu role and items, Cut and Copy disabled without a selection, keyboard focus and
  Escape, Copy, Cut, Paste of a hostile multi-line 9,000-character clipboard (one
  line, control characters removed, 8,192 characters), Select all, and WebView2
  default menus still off on the toolbar. Three mutants caught.
- Installer: lifecycle from a real 0.16 installation (baseline built with the pinned
  0.16 Setup script) checks no registration before the upgrade, the exact link
  command, http/https only, the RegisteredApplications value, nothing left after
  uninstall (the RegisteredApplications key kept) and the same after reinstall. The
  user's real http, https, .htm and .html choices, all RegisteredApplications values
  and the production registration were recorded before every Setup run and were
  unchanged after. A mutant that keeps the RegisteredApplications value was caught.
  The interrupted-installation sweep (16 cases) still repairs every case with no
  leftovers; the real profile (306 files) was unchanged.
- Settings deep link: `ms-settings:defaultapps?registeredAppUser=` with the
  RegisteredApplications value name, as Microsoft documents; covered by a unit test
  that also refuses any other URI. Automated runs never open Settings.

### Owner results

Owner checklist steps 12 to 16, 2026-11-11, bundle `o-4349a481`, version 0.17.0,
built from PR #23 head `25a94d0` (EXE SHA-256
`72b9a9f8a8806f829a8ca3a65b6424b26f22be8ba841396d289d16fa6c26a95c`). All passed by
owner report: the default was chosen in Windows Settings through the validation
entry; links from Discord and from a mail or PDF app opened as tabs in the running
window; a second launch handed off with no second window; Ctrl+N did nothing
unexpected; the address-field menu showed exactly Cut, Copy, Paste and Select all,
and paste was one line. Firefox was restored as the default and the validation
install was removed. These results cover that build only.

### Not tested here

- A second Windows user account (cannot be created here): the user rule is unit
  tested and the DACL is inspected.
- Remote pipe clients: protected by `PIPE_REJECT_REMOTE_CLIENTS` only.
- Choosing the default in Windows Settings, links from real chat, mail or PDF apps,
  the real clipboard and Narrator on the new menu: owner checklist steps 12 to 16.
- Multiple normal windows did not ship; see MULTI_WINDOW_PROPOSAL.md.

## v0.16 everyday browsing and toolbar follow-ups

Baseline `cb3d81dc4e5f7fca883fc561ff6216523a21dd2d` includes merged PRs #21,
#20 and #4. Windows CI 42931608985 and security 42931608910 passed before
branching. Branch: `feat/v0.16-everyday-basics`. The application is 0.16.0;
profile path, session format 1, SQLite schema 3 and dynamic CRT are unchanged.
RC status remains **unapproved**. See the
[reproductions, native tests, GUI checks, mutation results and Actions review](https://github.com/Niithursan/RustBrowser/blob/main/validation/V016.md).

Focused native toolbar/content tests pass in development. Eight compiled mutants
were rejected by their intended semantic assertions, including the old stale-event,
repeated-alert and ignored-hang behaviors. Native find requires a real query to
verify completion/matches; blank-dialog visibility is checked separately in the GUI.
All **31 local gates passed** on initial qualified head
`ed90da2a06711a9adeebb40c36350e28e69c33bd` before the first push:

- Formatting, strict Clippy, **81 regular tests**, debug and release builds.
- All **12 native suites** in separate processes, including normal/private page
  tools and extended toolbar recovery; **three independent daily-driver runs**.
- Process sampler and experimental launcher PowerShell fixtures; **18 production
  Python tests** (13 scope, 3 TLS, 2 icon) and **2 experimental Python tests**.
- Dynamic import inspection, portable package, v0.14 installer lifecycle/profile
  preservation, release metadata verifier, failed-static-build flag restoration,
  and owner-bundle identity/hash/path/startup/shutdown safety tests.
- `cargo audit --deny warnings`: **368 dependencies, 1,446 advisories**, with
  the unchanged three unsupported-platform exceptions.

The initial production EXE SHA-256 is
`1036cb448c83ca7144966f0faba02d9698a2cfa741e7dcba6f8d09b34ea94597`.
Raw local evidence: `<workspace>/work/v16/production-gates.json`, first-qualified.json
and gates-first.out/err. These artifacts are outside Git. The final evidence-only
head must run the same complete local gates before the second push; its exact
source/hash/hosted results belong in [PR #22](https://github.com/Niithursan/RustBrowser/pull/22)
and the delivery report after verification, without a third documentation push.
An earlier head's CI is never substituted for the final head.

Real Windows GUI checks on the 0.16 debug build confirmed disposable-session
restoration, correct page preview from More and Ctrl+P, Escape cancellation after
the reproduced focus fix, keyboard zoom and More reset, and normal window exit.
The broader development find/zoom checks and API limitations are in V016.md.
No actual paper/PDF output, new human/device acceptance, Windows 10 or clean-host
test is claimed. Neither this local result nor hosted success approves an RC.

The hosted docs-only skip remains unverified: this PR contains code and packaged
Markdown. The whole-PR classifier must run the Windows jobs even when its final
commit only updates evidence. No additional push is made just to test skipping.

### Owner report for the exact PR #21 build

On 2026-11-10 the owner reported **PASS for checklist items 1 to 8**: installer,
taskbar/window identity, Narrator, contrast theme, IME entry, real sign-in with
restart, video-call page, and two monitors with different scale. These are owner
results, not automated observations. Bundle `o-d8ee96f5`, source
`062c93dc7688ef296345f20ac7b1e39fd7c06fae`, static CRT experiment, executable
SHA-256 `bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58`.
It used the validation installation identity and only `Launch.ps1 -Installed`
with that bundle's disposable profile. The results apply only to those bytes.

They do not transfer to v0.16 or another candidate. Repeat the existing checklist
and its new find/zoom/print steps on the exact proposed candidate. A6 for that
candidate, clean-machine testing, risk-disposition approval and RC authorization
remain open. Static CRT remains opt-in. No Windows setting, real profile or
production installation was changed by this milestone.

## Post-merge v0.15 qualification investigation

PR #19 was squash-merged at `13ff603b84ba2b0c0429dc61bc338de738e9025b`.
Post-merge Windows CI 42898120216 and dependency security 42898120222 passed.
Work is isolated on `feat/rc-static-crt-validation`; main remains unchanged.
Windows 11 x64 is the initial qualification priority. RC status is **unapproved**;
technical acceptance is separate from declaration, tagging and publication.

Pinned static experiment: source `da054bf2b390b4df6dfd609b543a3ce6ff218962`,
Rust 1.99.1 x64 MSVC, `-C target-feature=+crt-static`, shipping EXE SHA-256
`bd9d660bf03baa54fdd85a4cd818e777d4b92dd3f382664aa590885d9b763f58`.
Its application sources/Cargo files/toolchain equal the merged baseline. Production
linking stays dynamic. The initial feasibility build is a different artifact;
gates were repeated on the pinned configuration rather than assuming equal hashes.
PE timestamps/CodeView GUIDs differ; reproducible executable builds are not claimed.

Pinned static gates passed: format, strict Clippy, 79 regular tests, debug/release,
ten native suites in separate processes, three independent daily runs, portable
packaging, v0.14 installer lifecycle and metadata verification. Owner-bundle tests
passed portable and validation-installed GUI startup/shutdown, exact identity/hash
refusal, foreign-profile/production-path refusal and caller environment restoration.
These are automated GUI checks, not human owner acceptance.

Default production verification also passed **29 local gates**: format, strict
Clippy, 79 regular tests, both builds, ten native suites, three daily runs, process
sampler/launcher fixtures, 2 experimental and 18 production Python tests (13 scope,
3 TLS, 2 icon), dynamic imports, portable/installer/metadata checks, failed-build
flag restoration, owner bundle and dependency audit. Audit scanned 369 crates and
1,446 advisories with the same three unsupported-platform exceptions.

The first production owner test read `Process.Path` before loader initialization.
A controlled suspended-start reproduction returned no path before resume and the
correct path about 520 ms later. `9dcff76` waits within the existing startup budget,
still rejects a wrong nonempty path, and adds deterministic cold-start, wrong-path
and exited-process checks. Fixed tests passed for static and dynamic builds. The
27 earlier gates on unchanged application source were retained; owner/audit passed
after the fix. Raw source records: `production-gates.json` (initial `84c81ad`,
follow-up `9dcff76`) and its saved pre-fix failure record. Final documentation-only
changes are packaged again; exact final-head hosted results belong in the PR.

MSVC import inspection reports 16 system DLL imports and no direct/delay external
CRT imports for the static EXE, versus 27 total/11 CRT imports for the dynamic
control. Static size is 4,443,648 bytes versus 4,311,552 dynamic. No installed DLL
was removed/hidden, and a missing-runtime host remains untested. Setup retains its
existing VC Runtime check; static production adoption has not been approved.

A fresh public survey completed: 41 visits, zero engine/renderer exits, crash dumps
or timeouts; three deliberate DNS/TLS failures diagnosed. A nine-site burst with
20 filler tabs also passed. OpenStreetMap returned HTTP 429; no bypass was added.
One contained download initiation was observed, not a verified completed transfer.

The fresh exact-source release-probe soak completed on 2026-11-10, 17:22 to 17:53
Toronto time. It ran the configured 1800-second soak with 20 stable tabs, at most
one scratch tab, suspension after 15 seconds and history disabled. Result: **178
churn cycles, zero navigation errors**, all 20 final switch responses successful,
normal root exit and exit of observed children. Diagnostics contain only starting,
started and clean_shutdown; no running marker remains.

| Observation | Value / limit |
| --- | --- |
| Soak sample span | 1798.489 s, 1792 samples; configured soak is 1800 s |
| Unavailable process readings | 24 soak samples; excluded from full-memory and CPU summaries |
| Aggregate working set | 930.23 to 1117.80 MiB during fully observed soak samples |
| Aggregate private bytes | 837.15 to 909.52 MiB |
| First/last five-minute private medians | 868.345 / 883.940 MiB; finite increase, not leak freedom |
| Observed CPU estimate | 0.8269% of 12-logical-processor capacity over 1748.682 valid seconds |
| After closing to one tab | 770.96 MiB working-set median / 432.805 MiB private median, 16 complete readings |
| Final switch response range | 20.645 to 28.607 ms, test callback endpoint |

Hardware: Ryzen 5 5600, 12 logical processors, 31.93 GiB RAM, Windows 11 Pro
10.0.26200 x64. Ordinary user browser activity stayed outside the sampled tree.
Working sets can double-count shared pages; private bytes are committed memory,
not physical RAM. CPU is an observed-process estimate, excludes unavailable
intervals and is not a GPU or whole-host measurement. This one controlled,
sleeping-tab run is not human browsing, sustained media/download testing, a leak
proof or a static-versus-dynamic performance comparison.

An earlier run stopped at 667.684 seconds/62 cycles when its process and exec
session disappeared; cause not established, no final shutdown result. It is
retained and excluded. One child-shell module conflict prevented a restart before
any profile/measurement began; the current PowerShell host resolved it without a
system/policy change. Only the completed fresh run is counted.

Sanitized numbers: `benchmarks/rc15-static-soak.json`. Raw evidence stays outside Git
in `work/rc15`, with source/flags/hashes in the pinned manifest. Final exact-head
production CI results belong in the PR delivery record; no extra workflow dispatch
or rerun is planned. The [investigation](https://github.com/Niithursan/RustBrowser/blob/main/validation/STATIC_CRT.md)
and [separate #4 review](https://github.com/Niithursan/RustBrowser/blob/main/validation/BASE64_REVIEW.md)
describe reproducible commands and limitations. No dependency changed; closed #2/#3
were left alone. Human owner checks, clean-machine/runtime tests and risk acceptance
remain open. No release candidate, merge, tag or publication is authorized.

## Version 0.15.0: release candidate preparation

Baseline: PR #18 squash `b4f0c2388a80853110baa9d43a8cec56a796d220`, version
0.14.0, with passing CI 42857529540 and security 42857529076. Work continues on
`feat/v0.15-release-candidate-prep`. No merge, tag, release publication, deployment,
production installation or real-profile write is authorized.

Recorded local environment: Windows 11 Pro x64, Rust 1.99.1/MSVC, WebView2 as
installed, 2026-11-10. Raw evidence is in workspace `work/v15`, outside Git.
Inherited results below were checked against the saved logs/JSON, not assumed
from the handoff. All **26 local gates passed** on source commit `5c8822e`;
the subsequent changes only record results and clarify the owner checklist.
Historical delivery: PR #19 at `c967735` passed CI 42896025986 and security
42896026042 before being squash-merged to `13ff603`. Those checks belong to the
v0.15 preparation delivery, not to this newer investigation branch.

### Final local gates (2026-11-10)

| Gate | Result |
| --- | --- |
| Formatting and strict Clippy | Passed; `--all-targets --all-features`, warnings denied |
| Regular Rust tests | 79 passed, zero failures; Windows-only/manual tests remain ignored in this command and are invoked separately below |
| Windows debug and release builds | Passed, locked dependency graph; release output isolated from the already running executable |
| Native WebView2 | All 10 standalone suites passed, each in its own process, including toolbar recovery and keyboard/layout |
| Daily-driver fixtures | Three independent fresh-profile runs passed, not retries |
| PowerShell fixtures | Process-tree sampler and experimental launcher safety passed; no Servo build/download/launch |
| Python | 2 experimental tests and 18 production tests passed: 13 CI-scope, 3 TLS-handshake and 2 icon tests |
| Portable package | Deterministic contents, hashes, metadata, offline links and unsafe-input refusal passed |
| Installer lifecycle | Fresh install, pinned v0.14 upgrade, repair/removal/reinstall, refusal cases, shortcuts and disposable profile preservation passed |
| Release metadata | Positive integrity/pin/repair checks and 36 malformed/downgrade/tampered/missing-input refusals passed |
| Dependency security | `cargo audit --deny warnings` passed: 369 dependencies, 1,446 advisory records; three existing unsupported-platform exceptions unchanged |

The first formatting check found the inherited keyboard test needed rustfmt;
`5c8822e` applies formatting only. The release build initially hit a running EXE
file lock; a separate output directory passed without stopping the user's browser.
Inno's baseline compiler then refused a 273-character scratch output path; the
same installer test passed under a shorter owned directory. Already-passed checks
on the unchanged source were retained, not rerun as retries. No product assertion,
profile path, security setting or dependency exception was weakened.

Raw records: `work/v15/current-final-gates.json`, its referenced `results.json`,
`final-gates.stdout.log`, `final-gates-resume.{stdout,stderr}.log` and
`final-gates-short.{stdout,stderr}.log`. They stay outside Git. Packaging is checked
again against the final documentation commit before delivery; exact-head hosted
checks cover the final committed tree. These are automated native Windows GUI
tests, not a human walkthrough of the owner checklist.

### Reproduced defects and before/after evidence

| Defect | Before | After / limits |
| --- | --- | --- |
| Toolbar browser process ends | Controls remained dead with `0x8007139F` while pages remained usable | Shared trusted toolbar rebuilt, notice shown; one recorded recovery 353 ms |
| Toolbar renderer ends | Silent dead controls with no recovery | Rebuilt; one recorded recovery 109 ms; same content views retained |
| Setup ends during copy | Six of six reproduced fresh mid-copy cases left a folder that both Setup and uninstall refused | Ownership recorded before copy; rerun Setup repaired every case in two 32-case sweeps |
| Stale Setup/uninstaller files | Temporary files and an unregistered old uninstaller could block later install/remove | Clean reserved installer state, retain registered uninstaller, remove empty folders; zero leftovers in both final sweeps |
| Parallel diagnostics tests | Temporary folder name collision | Per-test uniqueness; normal parallel suite retained |

Toolbar native recovery passed four runs. Five product mutations compiled but
failed the regression: missing dispatch, missing notice, no retirement, ignored
renderer failure and startup-only process observation. Restarts are limited to
three per two minutes, then native failure feedback; unsaved toolbar drafts can
be lost. This is not a recovery-latency guarantee or a new privileged bridge.

Installer sweeps each contain 16 fresh and 16 v0.14 upgrade timing cases,
including completed-process controls. They record 24 and 26 cases respectively
with killed owned processes. In both sweeps all 32 repairs succeeded, exact
candidate bytes and synthetic profile hashes/semantics were preserved, and no
uninstall refusal or leftover remained. A read-only real-profile metadata
fingerprint stayed unchanged (306 files, no production registration); this is
distinct from the synthetic profile's byte/semantic assertions. No real profile
was used for installation. Power loss, full disk and arbitrary kill timing remain
unverified. See `validation/interrupted-install.ps1` and the installation guide.

### Compatibility and accessibility evidence

- Full public-site survey: **41 addresses**, zero engine exits, renderer exits,
  crash dumps and timeouts. Three intended failures were diagnosed: DNS, expired
  certificate and self-signed certificate. OpenStreetMap's HTTP 429 also occurs
  on v0.14; site-side cause remains unknown. No bypass or user-agent workaround.
- Two independent nine-site burst runs with 20 filler tabs closed in one turn:
  zero exits, dumps and timeouts. No account credentials or real permission grants.
- Survey downloads now stay in its disposable profile; one download initiation
  was observed. Per-visit permission prompts are recorded. The separate geolocation
  demo's non-user-initiated request was denied without a prompt by existing policy.
- Native keyboard/layout regression: **three** passing runs; Tab/Shift+Tab, More
  via Enter and tab arrows. DPR 1/1.25/1.5/2 with CSS widths 1920/1536/1280/960;
  14 layout controls checked per configuration. Four compiled product mutants
  failed their assertions. This uses DevTools input/device emulation, not native DPI.
- Optional Windows UIA/input check: **three** passing foreground-guarded walks,
  24 initial/33 expanded controls named/typed, 29 distinct controls reached.
  Tab close buttons and panel heading are explicit exclusions. Not a screen reader,
  physical keyboard/IME or real Windows display-scale test.

Details: [compatibility](https://github.com/Niithursan/RustBrowser/blob/main/validation/COMPATIBILITY.md)
and [accessibility](https://github.com/Niithursan/RustBrowser/blob/main/validation/ACCESSIBILITY.md).

### CI and release status

Whole-PR, no-renames classification protects packaged documents and runs full
Windows checks for code, ambiguity or errors. Thirteen Python scope tests and two
caught mutations support it; the hosted docs-only skip path is **not exercised**
by this code-changing milestone and no saving is claimed. Dependabot version
updates are grouped/monthly with no automatic rebase; owner review of #2 to #4
is separate. `CI.md` records estimated costs and the delayed but passing run for
PR #18 head `36c8d2f`; the delay's cause is unestablished.

The pinned installer upgrade fixture is v0.14 with a new immutable cache key; its
first hosted build is cold. The public survey, interruption sweeps, UIA walk and
soak are manual procedures, not new recurring Actions jobs. Open one draft PR on
the final pushed head; do not dispatch/rerun to collect extra samples. Exact final
commit, run IDs and hosted results belong in that PR's delivery record so a
post-CI documentation push does not trigger another full packaged-doc run.

### Pending owner decisions and unverified scope

The [release criteria](https://github.com/Niithursan/RustBrowser/blob/main/validation/RELEASE_CRITERIA.md)
are proposals, not approvals. Portable EXE imports both VCRUNTIME140 DLLs; Setup
checks them, but the portable loader may fail before application code on a missing-
runtime machine. Static CRT is proposed and **not implemented without approval**.
Windows Sandbox is unavailable and was not enabled; no VM was created. Signing,
updater, new dependencies, second-launch hand-off and unrestricted Servo are absent.

The [owner checklist](https://github.com/Niithursan/RustBrowser/blob/main/validation/OWNER_CHECKLIST.md)
requires isolated profiles and validation identities. Clean-machine/missing-runtime,
standard-user, human wizard, Narrator, real contrast themes, physical IME/mixed DPI,
real accounts and camera/microphone acceptance remain unverified. No v0.15 30-minute
soak was repeated here; v0.14 measurements are historical, not a new candidate pass.
Profile path, session format 1 and SQLite schema 3 are unchanged. No RC is declared.

## Version 0.14.0

Baseline: verified PR #17 squash `ca7ac142031569279f1e1be0beffb53473af8cf7`, version
0.13.0. Main CI 42730890112 and dependency security 42730890455 passed before
branching. Work is on `feat/v0.14-daily-driver-reliability`. No merge, tag, release
publication, deployment or change to a real profile or installation.

Environment for everything below: one Windows 11 Pro x64 machine (10.0.26200),
Rust 1.99.1/MSVC, WebView2 Runtime 157.0.4341.42, 100% display scaling, 2026-11-09.

### Automated tests (local, all passed)

- Formatting, strict Clippy, **79 regular tests** (69 in v0.13; 10 new).
- **Eight** separate native WebView2 suites: the seven existing ones and the new
  `native_failure_reporting_tab_retirement_and_engine_restart`.
- **Three** independent daily-driver runs (TLS, modern APIs, accessibility, navigation).
- Debug and release builds. Dependency audit: 369 crates, 1,430 advisory records, no
  denied warnings, unchanged exceptions. No crate was added or updated.
- Process-tree sampler and launcher PowerShell tests, two experimental and five
  production Python tests.
- Portable package: deterministic ZIP, checksums, manifest, unsafe-output refusals and
  the new check that packaged documentation has no relative link to an unpackaged file.
- Installer lifecycle with Inno Setup 6.7.4 under a GUID validation identity: fresh
  install of the pinned **v0.13** build (`ca7ac14`, rebuilt from source in 35.9 s),
  refusal while that v0.13 process runs, **upgrade to v0.14**, exact installed file
  set and bytes, Apps & Features version, CLI launch, downgrade/future/invalid
  version refusal, running install and uninstall refusal, reparse and profile
  destination refusal, uninstall, reinstall with the optional desktop shortcut, and
  byte-identical profile and download data throughout. Release metadata verifier:
  36 refusal cases.

The new native regression was mutation-checked. Each of these changes made it fail:
follow-up suppression disabled; HTTP status ignored; immediate view destruction;
no engine recovery; download failure kept in diagnostics; download leaving the
failed state; blocked link reported as a page error.

### External website testing (manual, network-dependent)

`validation/site-survey.ps1`, production session, disposable profiles, no credentials.
Results and the defect list are in [COMPATIBILITY.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/COMPATIBILITY.md).
Final run: 29 addresses, 26 loaded (three of them HTTP error pages reported as
such), 3 intended failures correctly diagnosed, 0 timeouts, 0 engine exits, 0
crash dumps.

Engine crash measurements, Microsoft sign-in page, 3 s after load:

| Action | Before | After |
| --- | --- | --- |
| Close the tab (engine browser-process exit observed) | 6 of 8 runs | 0 of 12 runs |
| Close the window with the page visible (crash dump in profile) | 8 of 8 runs | 0 of 8 runs |
| Control: close the window on example.com, GitHub/Google sign-in, Wikipedia, YouTube | 0 of 6 runs | not repeated |

Burst close (`site-survey.ps1 -BurstCloseFillers n`, 2026-11-10): after the sign-in
page loads, it and `n` blank tabs are closed in one event-loop turn. The first fix held
at most 16 closed views and destroyed the oldest at once beyond that, which was the
sign-in page:

| Views closed at once | 16-view cap | 64-view cap (shipped) |
| --- | --- | --- |
| 11 or 16 | 0 of 8 runs | not repeated |
| 17 | 4 of 6 runs | 0 of 5 runs |
| 18 | 6 of 6 runs | 0 of 5 runs |
| 33 | not run | 0 of 3 runs |
| Control: example.com + 17 blank tabs | 0 of 3 runs | not repeated |
| Control: window exit with 19 tabs (18 sign-in pages) | 0 of 2 runs | not repeated |

The native regression now fills the window to 64 tabs and closes 62 in one turn; it
fails with the 16-view cap ("16 of 62 kept"). Clearing browsing data retires every
open tab at once and was subject to the same cap; that path was not measured.

Hiding the view for 0 or 100 ms before destroying it still crashed 8 of 8 times;
300 ms and 1 s did not crash in 8 runs each. The shipped delay is 1 s. Disabling
every application event handler on the view did not prevent the crash. These are
small samples on one machine, one Runtime version and one page; the crash is avoided,
its cause inside WebView2 is unknown, and other pages or Runtime versions may differ.

### Real GUI observations (release executable, disposable profile)

Scripted through real windows, not simulated input: the release build opened titled
"Rust Browser 0.14.0" and loaded `example.com`; starting it again for the same
profile showed the "already running" message box, exited with code 1 and left the
first window running; closing the window ended the process 1,089 ms later with exit
code 0; the diagnostic journal held only starting, started and clean_shutdown; no
running marker and no crash dump remained; restarting restored both tabs. Not
observed by a person: visual appearance, the new shortcuts on a physical keyboard,
the engine-restart notice outside the fixture, and any installer wizard page.

### Performance and soak

[benchmarks/V014_RESULTS.md](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/V014_RESULTS.md). 32 matched runs:
no difference between v0.13 and v0.14 can be established, and none is claimed.
One 30-minute churn soak of the v0.14 candidate completed: 1,799 s, 178 cycles, no
failed navigation, no engine or renderer failure, normal exit. It is a single
synthetic run and does not prove the absence of memory leaks.

### Not tested

- Windows 10, a clean virtual machine, a standard-user account, and machines without
  the WebView2 Runtime or the Visual C++ runtime. No virtual machine was available,
  and the real system was not altered to imitate a missing prerequisite.
- The interactive installer wizard, taskbar pinning and icon appearance, mid-copy
  termination, power loss and a full disk.
- Signing in to any website, popup sign-in with a real provider, public-site uploads,
  video playback, DRM, WebRTC, and permission or notification prompts on real sites.
- Screen readers, real contrast themes, Windows text size, display scaling other
  than 100%, multiple monitors, IME composition and non-US keyboard layouts.
- Engine recovery for failures other than process termination of the content
  browser process; GPU and utility failure handling is unit-tested classification
  only. Recovery of the toolbar's engine process does not exist.
- Physical sleep/wake, and browsing sessions by a person longer than the scripted checks.

### Remaining blockers for a daily-use release

Unsigned installer and no authenticated update; no clean-machine or Windows 10
evidence; one normal window per profile; no accessibility acceptance with assistive
technology; downloads do not resume; an engine restart reloads pages and loses
unsaved page state.

## Version 0.13.0

Baseline: verified PR #16 squash `71ff43eb9f4269c2047ab1fcc9965b8785d320a1`,
version 0.12.0. Main Windows CI 42721919432 and security 42721919854 passed before
branching. Work is on `feat/v0.13-windows-release-readiness`, [PR #17](https://github.com/Niithursan/RustBrowser/pull/17).
No merge, tag, release publication, deployment or real profile modification.

Local Windows 11 Pro x64, Rust 1.99.1/MSVC, SDK 10.0.22621.0, WebView2
157.0.4341.42, 2026-11-09 Toronto. Passed:

- Formatting, strict Clippy, **69 regular tests**. The new running-marker and rich
  profile tests are included; opt-in installer/diagnostic child helpers run through
  their parents/harnesses, separate from nine opt-in main native/benchmark tests.
- Seven separate preserved native WebView2 suites and **three** independent daily
  TLS/modern/accessibility/navigation runs; every invocation passed.
- Windows debug/release builds. Fresh audit: 369 crates, 1,430 advisory records,
  no denied warnings under the unchanged reviewed exceptions. No crate added/updated;
  existing Windows API feature gates and root Cargo.lock version changed.
- Sampler/launcher PowerShell regressions, two experimental Python tests and five
  production Python tests (including icon generation/frame bounds).
- Portable deterministic ZIP, metadata/hash/allowlist, version/x64 and unsafe-output tests.
- Actual Inno 6.7.4 controlled lifecycle: fixed validation identity/directory, v0.12
  binary → v0.13 upgrade, full installed contents, HKCU registration, Start Menu and
  opt-in desktop shortcut, exact executable/CLI launch, uninstall and reinstall.
  Hashes and typed readers preserve tabs/active/pins/settings, folders/bookmarks/history,
  permissions, opaque site data and downloaded bytes. Unowned app files survive removal.
- Invalid/future/downgrade metadata, profile destination, reparse path and running
  install/uninstall refusals. An actual running v0.12 binary without the new mutex
  is refused by the non-writing mapped-file probe; only its owned test process is stopped.
- Manual verifier: valid integrity/pin/explicit same-version repair plus **36**
  malformed/newline/type/path/duplicate/tampered/missing/downgrade refusal cases.
  It never executes an artifact or reports publisher authentication.

The same installer source is compiled with GUID test identity/destination. Production
registration is not exercised against this user's actual installation. v0.12 is
repackaged by that harness because no historical v0.12 installer exists. Separate
pinned-source baseline target reproduced a clean local build in 37.834 seconds.
First PR-head `694f4de` Windows aggregate and audit passed.

**Independent re-verification (Claude Code continuation, 2026-11-09).** These ran on
the application code at `cb7a4b1` and docs-only follow-ups:

- **Local gates, all passed:**
  - fmt and strict Clippy (0 findings)
  - 69 regular tests
  - all seven native suites and 3/3 daily runs
  - debug and release builds
  - sampler, five production and two experimental Python tests
  - portable package tests
  - `cargo audit`
- **Local installer lifecycle, all passed:**
  - the pinned v0.12 fixture rebuilt cold in 43.2 s
  - fresh install, then the 0.12 → 0.13 upgrade
  - exact payload allowlist and CLI launch
  - downgrade, invalid-version, running-instance, reparse-point and profile-directory refusals
  - uninstall and reinstall, with the optional desktop shortcut
  - hash and typed-reader profile/download preservation
  - the 36-case metadata suite
- **The user's real profile was untouched:** before and after, all 306 files under
  `%LOCALAPPDATA%\RustBrowser0.1` matched by path, size and write time. The test
  identity left no uninstall key, Start Menu folder or desktop shortcut, and no
  production installation exists.
- **CI review bundle (run 42726540504) checked independently:**
  - both `.sha256` files and the release JSON match the downloaded bytes
  - the ZIP holds exactly the seven manifest files plus `package.json`, every size
    and SHA-256 matching
  - no personal paths, profile paths, credentials or keys in shipped text
  - installer metadata reports 0.13.0 and Authenticode `NotSigned`
  - the manual verifier accepts a pinned 0.12.0 → 0.13.0 upgrade while reporting
    `PublisherAuthenticated: False`, and refuses same-version and wrong-pin input
  - the recorded source commit `973804d` is GitHub's merge of `cb7a4b1` into `main`;
    see [packaging notes](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/README.md)
- **Release GUI startup:** a disposable profile showed the 0.13.0 title and icon and
  rendered HTTPS. It closed cleanly with no remaining process, and `--version`
  prints `Rust Browser 0.13.0`.
- **CI:** cold and warm timings are in [CI.md](https://github.com/Niithursan/RustBrowser/blob/main/CI.md).
  Warm CI is not faster than v0.12; it adds the installer lifecycle.

Actual installed-application GUI smoke passed in a new disposable profile: 0.13.0
title and globe icon, new-tab chrome, address entry, public HTTPS rendering/loading,
Ctrl+T and Ctrl+Tab, normal Alt+F4 close and restart preserving two tabs/active URL.
The shipping executable hash was `3e787b4102a1e2ec1aa6786b04cd01c0e95ac2eccb0e9a45f9ecdc7153399989`.
No complete installer wizard interaction or clean-machine install claim is made.
Unverified: Windows 10, missing/obsolete runtime host, actual standard-user clean VM,
mid-copy kill/full disk/power-loss transaction, signed artifacts/update infrastructure,
physical mixed DPI/IME/screen reader and real-profile upgrade. Existing v0.12 browsing
limitations remain. See [installer design](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/README.md),
[install/recovery](INSTALL_WINDOWS.md) and [update security](https://github.com/Niithursan/RustBrowser/blob/main/packaging/windows/UPDATE_SECURITY.md).

## Version 0.12.0

Baseline: PR #14 squash `fef457a` (0.11.0). Work is on
`feat/v0.12-ui-compatibility-ci` (PR #16), built on the separate navigation-race fix
in PR #15. Main CI at `fef457a` failed in the daily native suite. The cause was a
production race (a superseded host load's late start), fixed in PR #15 with a
deterministic native regression: 0/10 before, 10/10 after.

Local checks on Windows 11 Pro x64, Rust 1.99.1, WebView2 157.0.4341.42, 100%
scaling, after the 0.12.0 version bump. **Passed:**

- Formatting and strict Clippy.
- 67 regular tests, including the new WCAG contrast test.
- All seven separate native WebView2 suites.
- Three independent daily-driver runs, including the new native assertions:
  - same-turn navigation supersession
  - assertive failure announcement
  - active-tab indicator
  - 150% zoom at 640 px
  - panel focus and item-specific action names
  - service worker, Cache Storage and modern APIs
- Debug and release builds.
- Sampler and Python fixture tests.
- Portable package metadata and safety tests.
- `cargo audit --deny warnings`.

Every new native assertion was mutation-checked: it fails when its feature is removed.
This found and fixed one vacuous zoom check. Forced-colors and reduced-motion styling
are asserted under DevTools media emulation. Real-window captures (PrintWindow, disposable profiles) confirmed the redesigned
Dark, Slate, Plum and Light themes, multi-tab, new-tab and 640 px error layouts. Accessibility scope and manual gaps:
[ACCESSIBILITY.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/ACCESSIBILITY.md). Compatibility additions:
[COMPATIBILITY.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/COMPATIBILITY.md). CI changes and cold/warm timings:
[CI.md](https://github.com/Niithursan/RustBrowser/blob/main/CI.md).

**Not tested:**
- Real screen readers.
- Windows contrast themes and the Windows text-size setting.
- Physical keyboard layouts/IME.
- Non-100% or mixed-DPI displays.
- Real accounts, DRM and devices.
- Clean-machine installation.

No performance claim is made for the application in v0.12; only CI timings were measured.

## Version 0.11.0

Verified baseline PR #13 squash `729198d023384103da0464f1148031aa6e681566`,
production 0.8.0, with passing merged Windows CI and production audit. Work is
isolated on `feat/v0.11-daily-driver`; main, release tags and existing profiles
are preserved. The package intentionally advances to 0.11.0 for production
diagnostic/navigation/UI/Windows metadata improvements. No Servo development.

Local Windows 11 x64 / Rust 1.99.1 / WebView2 157.0.4341.42 checks passed on
2026-11-09, production code `58cdef9e9017e76a03c01baebbf9b44ad17dffcd`:

- `cargo fmt --all -- --check`
- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- `cargo test --locked --all-targets --all-features`: **58 passed**, zero failures.
  Nine main tests are opt-in (eight native, one performance probe); one diagnostic
  child helper is ignored in the general suite and exercised by its parent test.
- `cargo build --locked` and `cargo build --locked --release` passed natively.
- `cargo audit --deny warnings`: **369 dependencies**, 1,422 current advisory
  records, no denied warnings under the unchanged three reviewed non-Windows
  exceptions in `.cargo/audit.toml`. No dependency/exception added; root lock
  version changed.
- All **eight native Windows regressions** passed as separate processes. The new
  daily suite checks modern APIs, cross-origin/SPA navigation, native navigation
  ID/Stop state, diagnostics, light/address/compact UI and untrusted TLS rejection.
- Existing PowerShell experimental launcher/output safety and **two Python tests**
  passed; no Servo source build, sandbox modification or new experimental result.
- Portable package tests passed equal-input ZIP hash equality, checksum/manifest
  hashes, exact file allowlist and version/PE/profile/reparse refusals. Actual PE
  reports 0.11.0, x64 and **NotSigned**; extracted manifest is **asInvoker**, no UIAccess.
  PE imports also show `VCRUNTIME140.dll`/`VCRUNTIME140_1.dll`; the x64 Visual C++
  v14 prerequisite is now disclosed alongside WebView2. No clean-machine loader
  or missing-DLL recovery claim is made.
- Actual shipping Windows GUI: HTTPS pages/redirect, loading feedback, Light,
  Ctrl+L/T/W/Tab/D/H/J/Shift+O/Shift+P, drag reordering, bookmark/history/download
  panels and normal restart with URLs/active tab/pin/theme/bookmark preserved.
  See [the acceptance matrix](https://github.com/Niithursan/RustBrowser/blob/main/validation/DAILY_DRIVER.md) for precise limits.

[Corrected matched measurements](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/V011_RESULTS.md) record two fresh/warm
repeats at 1/5/10/20 tabs, alternating order, against unchanged baseline using the
identity-validated sampler. Results are mixed; no performance improvement is
claimed. A fresh 30-minute 20-tab synthetic churn soak at `d77d9c9` passed with
178 cycles and zero navigation errors. True OS-cold startup, pixel presentation latency, real
accounts/devices/DRM, physical sleep/wake, mixed-DPI/screen-reader acceptance and
clean-machine installation remain unverified. No broad performance or readiness
claim follows from the native fixtures or public-site smoke checks.

The pushed code checkpoint's [Windows CI passed](https://github.com/Niithursan/RustBrowser/actions/runs/42566063522).
Final PR/head CI and final soak results will be recorded before review delivery.
**Correction:** that initial screening is withdrawn after a reproduced PID-reuse
bug in the sampler. A completed 30-minute workload also revealed 178 false
navigation failures on replacing new blank pages. Strict checks and 60 regular
tests now pass with proactive supersession/deferred-blank handling; the stronger
native regression and short workload pass with no errors and identity-correct
child accounting. Corrected matched measurements and the clean soak were completed
afterwards (see above). The single warm-profile `Tab closing failed` from the
stopped corrected campaign did not reproduce in 28 later runs; its root cause is
unconfirmed.
Release packaging control flow is reviewed and locally tested at the producer
level; no tag, workflow publication, installer or GitHub release was executed.

## v0.10 investigation

Based on verified PR #12 squash commit
`bba82c9b6a34c990f1e04f70c32abeda898f5e23`; production remains 0.8.0.
The [detailed record](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/VALIDATION_V010.md) includes commands,
counts, repeated numeric samples, GUI observations and failures.

- Production fmt, strict Clippy, 47 regular tests, seven native WebView2
  regressions, Windows debug/release builds and dependency audit passed.
- Launcher safety, two Python tests, seven separate Servo example tests,
  three smoke/reuse-refusal runs and three four-scale runs per engine passed.
- Standard AppContainer boundaries and both diagnostic collectors passed
  token/file/process/handle/resource/IPC assertions and live broker recovery.
  Actual malformed and hung workers were rejected in both modes.
- The original LPAC suite still fails WSAStartup 10107 with profile cleanup.
  The upstream Servo lock audit and strict minimal-feature Clippy still fail;
  their exact limitations remain visible and production gates are intact.
- Native Windows fixture key/pointer/wheel/focus/Unicode entry/resize and
  normal closure were observed. Native IME candidates, physical mixed-DPI
  transitions, display latency, long soak and real renderer containment
  remain unverified. No arbitrary content was enabled.

## v0.9 investigation

The branch is based on verified PR #11 squash commit
`469cddf2c080f7b1fa0a8dcd2c5c6993fbe3effa`. The
[detailed validation record](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/VALIDATION_V09.md) describes
the isolation decision, tests, script timing endpoints and remaining work.

- Formatting, strict production Clippy, 47 regular tests, debug/release
  Windows builds, and the production dependency audit passed locally.
- All six existing native WebView2 regressions passed, including the
  sleeping-tab race fix. The existing PowerShell safety suite and two Python
  server tests passed.
- The new native WebView2 fixture passed at four simulated rasterization
  scales. The separate Servo example's six unit tests passed. Its own host
  code compiled with compiler warnings treated as errors. Upstream Servo
  library warnings remain visible.
- The standard AppContainer probe passed repeated sentinel/lifecycle tests
  and actual broker-crash recovery. Its loopback timeout is inconclusive.
  The explicit LPAC attempt failed Winsock initialization with error 10107
  and reported profile cleanup. A Servo renderer sandbox remains unverified.
- Actual Windows IME service behavior, monitor DPI transitions, physical
  input-to-display latency, and compositor first-pixel timing remain
  unverified. The native fixtures exercise synthetic DOM events and
  engine scale overrides.

## Version 0.8.0

Windows 11 x64 validation on 2026-11-08. The branch starts at the
PR #10 squash commit `37037fc358d22a16af7ee7c1cc1a4635782cea94`.
Its [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42456476269)
and [dependency audit](https://github.com/Niithursan/RustBrowser/actions/runs/42456476396)
passed.

- Production formatting, strict all-target/all-feature Clippy, 46 regular
  tests, native Windows debug/release builds, and the production dependency
  audit passed.
- All six native WebView2 regressions passed individually, including the
  sleeping-tab late-callback regression. The v0.7 PowerShell launcher tests
  and both Python fixture-server tests passed.
- The pinned upstream Windows example compiled before the host was written.
  Its controlled local variant ran successfully. The standalone host compiled,
  its two fixture policy tests passed, and three fresh smoke runs reached
  presentation and exited cleanly. Reusing a run was refused.
- Actual Windows GUI checks exercised the local fixture, input, resize,
  navigation, popup refusal, and normal closure. The separate source
  build procedure and [detailed results](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/EMBEDDER_RESULTS.md)
  document the exact manual scope, upstream lockfile findings, and
  measurements. Production CI retains the existing WebView2 gates.

## Version 0.7.0

Local validation on 2026-11-07 and 2026-11-08, Windows 11 x64, Rust 1.99.1/MSVC and WebView2
Runtime 157.0.4341.42, on `feat/v0.7-servo-spike`. WebView2 remains the only
production engine; Servo is an isolated opt-in experiment with no Cargo dependency.

- `cargo fmt --all -- --check`, strict all-target/all-feature Clippy,
  `cargo test --locked --all-targets --all-features`, and debug/release builds passed.
  The regular suite has 46 passing tests. The six native tests and performance probe
  are selected separately.
- All six documented native WebView2 regressions passed when run individually:
  storage/engine deletion, application clearing, tab sleep/security, private new-tab,
  popup/upload/audio/fullscreen/redirects, and renderer crash/reload recovery.
- `tests/servo_tools.ps1` passed output-isolation, profile-preservation, reparse-point,
  digest-refusal, argument-quoting and PowerShell syntax checks. The two Python
  loopback fixture tests passed.
- The pinned Servo v0.7.0 release archive and executable matched their SHA-256 pins.
  Isolated Windows GUI checks verified local HTML and the fixed HTTPS example page,
  clicks, text entry, scrolling, and a loopback storage round trip. The fixture
  reported no `window.ipc`. The measured Servo process exited with code 0 after
  normal window closure. Its disposable profile retained files after exit.
- The sanitized one-run resource sample in
  [Servo results](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/RESULTS.md) was checked against its raw values
  and fixture digest. It is not a controlled engine-only or statistical comparison.
- A full Servo source build (`mach bootstrap`/`mach build`) was not performed. The
  required dedicated native-toolchain environment and pinned Servo toolchain are
  separate from Rust Browser's; feasibility constraints are in
  [Servo research](https://github.com/Niithursan/RustBrowser/blob/main/experiments/servo/RESEARCH.md). Real-site compatibility,
  in-process embedding, Windows OS isolation, daily-use security, and equivalent
  startup/input latency remain unverified. No Servo engine selector or profile
  migration is implemented.
- The initial [PR Windows run](https://github.com/Niithursan/RustBrowser/actions/runs/42435557721)
  failed the native sleeping-tab assertion. The probe now models a late callback
  separately from the real suspension check and preserves unique tickets across
  resets. The repaired native test passed 12 consecutive local runs. Final checks
  are recorded on [PR #10](https://github.com/Niithursan/RustBrowser/pull/10).

## Version 0.6.0

PR #8 was verified squash-merged at `a8b24fa79e18cefa8b8d64d6d9bf0788c3432140`,
package version 0.5.0, with a tree identical to the previously validated v0.5 branch.
Its [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42401923750)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42401923693)
passed, as did all 35 baseline regular tests locally. Development uses
`feat/v0.6-compatibility-reliability`; no merge, release tag or publication is authorized.

Local Windows x64 validation: Rust 1.99.1/MSVC and WebView2 157.0.4341.42.

- Formatting, strict all-target/all-feature Clippy, 42 regular tests, six separately
  selected native tests, Windows debug/release builds and dependency audit passed.
  The audit scanned 369 locked dependencies and 1422 advisories with the existing
  reviewed exceptions unchanged. Cargo.lock changes only the package version.
- Native loopback tests verify runtime-marked popup blocking, same-profile opener,
  shared cookie/message/script-close behavior, `noopener`, no remote IPC, POST upload
  protection, actual PCM media, fullscreen/redirects, two 1 MiB HTTP downloads with
  collision-safe completion and an independent quiet-tab active-download guard.
- A separate disposable renderer crash produced actionable feedback and recovered
  through Reload. Test-only CDP user gestures/crash calls are not in the product.
- Existing private isolation, permission/clearing, session retention and new-tab tests
  remain passing. The sleep test checks actual suspension and an injected long pause
  resuming the same native view/document; it does not simulate real hardware wake.
- Recovery regressions cover missing/corrupt main sessions, valid temporary promotion,
  empty/truncated invalid saves, preservation of originals, stale-temp privacy
  precedence, I/O refusal, live/abandoned staging leases and no payload deletion.
- A killed owned child holding session/SQLite state verified lock release, committed
  snapshot/bookmark retention and rollback of an uncommitted SQLite transaction.
  Five subsequent store open/close cycles passed. A helper test runs only in that
  child; the regular test count includes its no-op parent invocation.

Actual GUI automation on an isolated test profile verified the popup cookie/message
flow, native selection/upload of the supplied synthetic file (313 multipart bytes),
generated video, fullscreen with the origin toolbar visible, Escape exit, local
WebRTC connected/advancing frames, and continued loopback after backgrounding over
two minutes with Keep awake. The owned GUI process was intentionally terminated;
the committed two-tab session restored after restarting, then closed normally.
Accessible control/tab names and roles and all-tabs keyboard Home selection were
observed. Desktop access initially failed; after the user confirmed it was unlocked
and the automation session was reset, the GUI checks above were completed. No OS
privacy/security settings or device grants changed.

See [compatibility matrix](https://github.com/Niithursan/RustBrowser/blob/main/validation/COMPATIBILITY.md) for exact limits and
[test procedures](https://github.com/Niithursan/RustBrowser/blob/main/validation/README.md) for repeatable 30–60 minute interactive and
automated workloads. Physical sleep/wake, real accounts/devices, full-disk hardware
failure, 125/150/200% mixed-DPI/multi-monitor and spoken screen-reader acceptance
remain unverified. Historical v0.5 benchmark numbers below are not v0.6 measurements.
The 30-minute automated soak passed 178 churn cycles, all twenty final responsiveness
checks, close-to-one recovery and normal shutdown. It is distinct from the shorter
actual GUI checks, and includes documented concurrent activity and sampling limits.
See [SOAK.md](https://github.com/Niithursan/RustBrowser/blob/main/validation/SOAK.md) for numeric evidence and
[PR #9](https://github.com/Niithursan/RustBrowser/pull/9) for final-commit CI results.

## Version 0.5.0

Preservation baseline: PR #7 was confirmed squash-merged at
`8e97713fa9c006586e250f07fdc4af12003b192d`, with package version 0.4.0 and a tree
matching the previously verified v0.4 feature head. Its
[Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42335972778) and
[dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42335972788)
passed. All 28 regular baseline tests passed locally. Development uses
`feat/v0.5-performance-ux`; main and release tags remain unchanged.

The v0.4 measurements below used different pages and short samples. They are retained
as historical observations and cannot establish an improvement. The controlled v0.5
comparison uses an identical test-only synthetic workload on both versions; see
[benchmark procedure](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/README.md) and [results](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/RESULTS.md).

New regular coverage checks live tab identity/ordering/pinned groups, every explicit
protected-activity flag, generation races, native refusal/timeout/resume failure
states, additive legacy-session compatibility, invalid metadata/settings/favorites,
and exclusion of private/retention-disabled tab metadata. Existing privacy, database,
download safety and navigation tests remain in place.

Native tests run separately because winit permits one event loop per process:

```powershell
cargo test --locked --bin rust-browser native_private_storage_and_confirmed_engine_deletion -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_application_clear -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_tab_sleep_resume_and_local_page_security -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_private_newtab_settings_are_ephemeral -- --ignored --test-threads=1
```

The new native checks use real WebView2 controllers with disposable profiles. They
verify native suspension state, same-controller/document preservation after resume,
late successful callbacks racing tab selection, pin/Keep awake/global-disable guards,
actual bundled new-tab DOM rendering, theme/favorites persistence, address draft
preservation during background UI updates, forbidden remote schemes, and leaving
the local page through navigation/back. A native private session verifies normal
favorites are not read, private changes remain ephemeral, recent history cannot be
enabled and the normal session file stays byte-identical. The existing native
permission/isolation/confirmed-clearing checks also remain in Windows CI.

Local validation on Windows x64, Rust 1.99.1/MSVC and WebView2 157.0.4341.42:

- Formatting, strict Clippy across all targets/features, all 35 regular tests and
  all four explicitly selected native tests passed. Debug and release builds passed.
- `cargo audit --deny warnings` passed against 369 locked dependencies and 1422
  advisories, with the existing reviewed platform exceptions unchanged. No dependency
  was added; Cargo.lock changes only the application's package version.
- Manual GUI checks opened separate Alpha/Beta/Gamma loopback fixture pages. Search
  from the local page, Ctrl+T, direct tab selection, pointer drag ordering,
  Ctrl+Shift+PageUp and Ctrl+Shift+P worked. Pins and ordering survived normal restart.
  Remote content reported `IPC: undefined`; navigation correctly left the local UI.
- A background fixture changing its title exposed an address draft race. The fix
  was checked in the native DOM regression and manually: typed text stayed intact
  across background title updates and submitted successfully.
- The interface wrapped at approximately 570 pixels wide, with content positioned
  below the expanded toolbar; the local page remained scrollable and usable. The
  overflow selector opened and listed hidden tabs. Native popup targeting limited
  automated mouse selection of its menu; full popup interaction remains a manual
  follow-up. Existing Ctrl+Tab navigation and direct selection remain available.
- A synthetic favorite was entered and saved using the actual new-tab form. Theme,
  favorites/settings disk persistence and private exclusion were checked by the native
  tests rather than claimed as a full manual private/theme acceptance run.

No physical device access, fresh audio/download protection GUI test, true OS-cold
startup, hours-long browsing soak, screen-reader or multi-monitor/high-DPI result is
inferred from automated coverage. Protected-activity flags and native granted-document
protection are tested; arbitrary critical live-site work still needs Keep awake or
disabled automation. v0.4 download safety regressions pass; its recorded slow-transfer
GUI results below are historical, not repeated v0.5 observations. Final CI results
and benchmark findings are linked on the pull request.

Twenty successful native performance-probe runs measured 1/5/10/20 tabs across fresh
and warm profiles, plus a corrected 20-tab CPU comparison with a one-minute additional
soak. App and WebView2 descendants were included. Hidden timer ticks fell to zero;
corrected 20-tab idle CPU means were 15–24% lower in the two sampled conditions.
Private bytes were modestly lower, with no general physical-RAM or startup speedup
established. Some switching/all-loaded timings regressed. Initial CPU measurements
were excluded after discovering integer rounding in the PowerShell harness; corrected
data, numeric samples, exact methods and variability are in [RESULTS.md](https://github.com/Niithursan/RustBrowser/blob/main/benchmarks/RESULTS.md).

## Version 0.4.0

Baseline: PR #6 was confirmed squash-merged at `3117426840c0d4338e1537d25551529fb4ecf2f1`.
The merged tree matched the verified v0.3 feature tree, and all 21 baseline tests
passed locally. Its [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42330920256)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42330920275)
passed. Development used a feature branch; no release/tag was created or changed.

Local checks on Windows x64, Rust 1.99.1/MSVC and WebView2 156.0.4314.58:

- Formatting, strict Clippy, 28 regular tests and Windows debug/release builds passed.
- The dependency audit passed for 369 locked dependencies with the existing reviewed
  exceptions unchanged. No dependency was added.
- Tests cover private lease isolation and abandoned-directory cleanup, snapshot
  exclusion, queued-write barriers, exact-origin permissions/revocation, confirmed
  database clearing, invalid settings, migration/corruption and connection indicators.
- The native isolation test used actual WebView2 views and a test-only local fixture at a reserved origin.
  It verified two-way cookie/local-storage isolation, a successful clear callback,
  cleared normal storage with private storage intact, origin/capability correctness,
  cancellation denial, and an explicit synthetic notification grant.
- The application-level native test verified unconfirmed clearing is rejected,
  active tabs are replaced by a blank view, the native callback and database worker
  both complete, bookmarks and library recovery files remain, and saved session URLs
  and recovery files are removed. A separately injected failed engine callback
  verified visible partial-failure reporting and denial of further grants.

The two native tests open real windows and run separately because winit supports one
event loop per process. CI runs them explicitly after the regular suite:

```powershell
cargo test --locked --bin rust-browser native_private_storage_and_confirmed_engine_deletion -- --ignored --test-threads=1
cargo test --locked --bin rust-browser native_application_clear -- --ignored --test-threads=1
```

Actual Windows GUI and process checks used isolated test profiles:

- Normal cookie and local-storage markers were absent from a private window on the
  same origin. Remote content reported no application IPC object. Normal markers
  were still present after private shutdown. The private window created neither
  `session.json` nor `library.sqlite`, and its temporary directory disappeared on
  normal close. Ctrl+Shift+N opened a correctly labelled private window.
- A unique private navigation did not appear in the normal session or SQLite
  history. After intentionally terminating the private test process, a later
  launch removed its abandoned directory and kept the current live directory.
- A normal five-tab test process was terminated after its session was saved. On
  restart, all five tabs and the active URL restored; normal shutdown then succeeded.
  This does not prove recovery of changes lost during the write debounce or an OS crash.
- HTTP and HTTPS displayed their actual scheme/origin with distinct descriptions.
  A loopback notification request displayed the requesting origin and capability;
  switching tabs dismissed it. Permission decisions and data clearing were exercised
  in the controlled native tests above, not by granting devices or deleting user
  data through the desktop UI. A `target=_blank` HTTP link opened a third normal tab.
- A 4 MiB slow HTTP transfer showed live progress and cancelled at 88%; no staged
  or final file remained. A deliberately truncated response showed Failed after
  the two-minute stall guard, leaving no current staging files. Abrupt termination
  during a transfer can still leave a staging folder at the chosen destination.

Process-tree samples over five seconds after loading pages:

| Tabs | Processes | Summed working set (MiB) | Private bytes (MiB) | CPU seconds |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 14 | 792.0 | 352.8 | 0.812 |
| 5 | 17 | 915.9 | 427.5 | 0.031 |
| 10 | 22 | 1131.2 | 507.9 | 0.000 |

These include browser and WebView2 descendants. Summed working sets can count shared
pages more than once. The two-tab run included an HTTPS home page; five and ten tabs
used static loopback content. These brief samples do not characterize real websites,
long-term growth, or background media. No suspension or performance gain is claimed.

Limits: private cleanup is best effort; private mode does not provide anonymity,
secure erasure, or protection from Windows and network observers. Downloaded files
and metadata remain. Library recovery copies may retain history and permission rules.
Physical camera/microphone/location access, broader site/redirect compatibility,
mixed content, accessibility, long-duration soak tests and real OS crashes were not
comprehensively verified. CI results on the final revision are recorded on the PR.

## Version 0.3.0

Preservation baseline: PR #5 was verified squash-merged into `main` at `7c9c152`.
The merged commit's [Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42326479368)
and [dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42326479361)
completed successfully; its 11 baseline tests passed locally. No v0.2.0 tag or release
existed. The owner requested that v0.2.0 remain unpublished. No existing release was modified.

Local validation uses Windows x64, Rust 1.99.1/MSVC and the installed WebView2 Runtime.
Final local formatting, strict Clippy (`-D warnings`), all 21 automated tests,
debug/release builds, and `cargo audit --deny warnings` passed. The audit scanned
369 locked dependencies using the existing reviewed exceptions; no new exception
was added.
Automated coverage includes the existing v0.2 tests plus SQLite bookmarks/folders,
edit/search/remove/restart behavior, transactional v1→v2 migration, corrupt-file
preservation/recovery, newer-schema refusal, invalid inputs and SQL text, visit
deduplication, date ranges, query/credential exclusion, clear confirmation enforcement,
Windows filename/device/stream/traversal handling, atomic collision protection,
simultaneous finalization, partial cleanup, stalled-transfer tracking, and legacy
settings compatibility with the persisted recording toggle. CI results for the final
revision are recorded on the pull request, not assumed from local checks.

Actual Windows GUI checks performed using an isolated profile:

- Ctrl+D saved a bookmark; Ctrl+Shift+O opened its manager. A Research folder was
  created, the bookmark title was edited and assigned to it, and both survived restart.
  The saved bookmark opened in a new independent tab.
- Ctrl+H displayed recorded visits with titles and local timestamps. A nonmatching
  query produced an empty result. Clear history displayed a confirmation dialog;
  cancelling it preserved entries. Actual clearing/removal and date-boundary behavior
  are covered by automated tests rather than claimed as manually verified.
- A loopback HTTP fixture sent a traversal-style suggested name. Native folder
  selection, live byte/percentage progress, completion and a second collision-safe
  suffixed file were verified without altering the first file.
- Cancelling a slow transfer produced Cancelled status and removed its partial output.
  A truncated response exposed WebView2 retry behavior. The final implementation
  captured terminal state events, rejected duplicate active requests, and displayed
  Failed after the two-minute no-forward-progress safeguard. No current staging
  file remained for that failed transfer. Earlier forcibly stopped development runs
  left staging directories, consistent with the abrupt-exit limitation below.
- A real HTTPS Wry source archive from GitHub completed (2,303,056 bytes during this
  check). ZIP integrity passed, and Windows Zone.Identifier metadata was present
  after finalization. The archive was not executed. Open folder opened its destination
  in Explorer.
- Existing tab switching, separate addresses, native Back/Forward, search provider
  selection and address-bar DuckDuckGo search were exercised. Query-bearing search
  URLs were absent from the SQLite history.
- A final normal restart restored both tabs, their order, the active DuckDuckGo
  results page and the DuckDuckGo setting. Ctrl+J opened the empty downloads manager
  in the new session, as expected for session-only download records.
- With a separate library at unsupported schema version 99, the browser rendered
  its HTTPS home page and displayed an unavailable-library notice. Normal shutdown
  succeeded and the database remained at version 99.

GUI testing found and corrected Windows extended-path handling at the WebView2
download boundary, parent-window ownership for the destination dialog, transient
interruption handling, and stable progress-row rendering. The browser adapter keeps
native COM operations on the UI apartment; database and finalization work run off it.

Limits: flat folders; 200 results per query; 5000 bookmarks; 200 folders; latest 10,000
visits. History query matching has SQLite's default Unicode/case-folding limitations.
Date filters use UTC days while timestamps display locally. Titles/paths/bookmarks
and full session URLs can still contain sensitive data; storage is local plaintext.
Library history clearing does not clear native back/forward, cookies, cache, session
URLs or recovery backups. No private mode, encryption, history import/export, nested
folders or full compatibility/security audit was implemented.

Downloads: HTTP(S) only; 16 active transfers, one per source URL; 256 in-memory records.
Records and transfers are not restored after restart. No pause/resume, execution,
malware-scanner integration or certificate-policy bypass. Unknown lengths have no
percentage. Two minutes without forward progress cancels even a legitimately stalled
server. Closing/terminating during a transfer or failed finalization may leave staging
data. Native runtime security-policy behavior, mixed DPI, screen-reader operation and
long-running resource use were not comprehensively validated. Release packaging was
inspected but no tag/release was published.

## Version 0.2.0

Local Windows x64 validation with Rust 1.99.1/MSVC and the installed WebView2 Runtime:

- Formatting, strict Clippy, debug build and optimized release build passed.
- 11 unit/integration tests passed: existing URL behavior, tab IDs/selection/closure,
  tab limits, search encoding for all three providers, session/settings round-trip,
  exclusive profile lock/release, last-queued save, atomic replacement, invalid and
  oversized snapshots, corrupted-file recovery, and background write-error reporting.
- Live desktop checks verified Ctrl+T, Ctrl+W, next/previous-tab shortcuts, independent
  addresses/history, redirect URL updates, selection of DuckDuckGo, and actual search
  result rendering. Restart restored two tabs in order, active tab 2 and DuckDuckGo.
- A startup focus issue found during GUI testing led to a shared shortcut map for
  both WebView2 controllers and the parent window, plus focus after toolbar readiness.
  Ctrl+T then worked immediately after restart without a preceding content click.
  Ctrl+W closed exactly one tab, and closing the final tab left one working Home tab.
- Dependency audit passed using the repository's existing reviewed exceptions. No
  dependencies or audit exceptions were added. CI results are recorded on the PR.

The restricted execution environment denied filesystem rename operations during the
first persistence-test run. The same tests passed with normal Windows filesystem
access. A later test build encountered the running executable's Windows file lock;
closing the smoke-test browser allowed the build and tests to pass.

Limits: restoration saves URLs/order/active selection/settings, not history stacks,
form input, scroll position or JavaScript state. Session data is plain text in the
local profile. The write debounce can lose recent changes during an abrupt process
termination; normal shutdown flushes pending data. Every restored tab loads its URL.
There is a 64-tab limit, no suspension, no tab reordering and no private mode. Tabs
share cookies and site storage; separate navigation does not mean isolated identities.
Google/Bing query generation is tested automatically; their live results were not
separately verified. No broad compatibility, accessibility, mixed-DPI, crash/power-loss
durability or long-running resource review was performed. Tag-driven release
publication was inspected but not executed; no release tag was pushed.

## Historical Version 0.1 validation

Validated locally on Windows x64 with Rust 1.99.1 (MSVC), winit 0.30.13,
Wry 0.57.1, webview2-com 0.39.1 and the installed WebView2 Runtime.

## Passed

- `cargo build --locked`: compiled and linked the Windows executable.
- `cargo fmt --check`: passed.
- `cargo test --locked`: two URL-policy unit tests passed, including normalization,
  localhost ports, paths/query/fragments, invalid input, credentials and blocked schemes.
- `cargo clippy --locked -- -D warnings`: passed.
- Launched the actual native desktop window and inspected its toolbar and rendered page.
- Rendered `https://example.com/` and navigated through the address bar to
  `https://www.rust-lang.org/`; the redirected `https://rust-lang.org/` address appeared.
- Alt+Left returned to the first page and enabled Forward. Alt+Right returned to Rust.
- Ctrl+L selected the address while a webpage had focus, confirmed visually.
- Navigating to `https://rust-browser-smoke.invalid/` produced an error status in
  the toolbar and WebView2's DNS error page. Connecting feedback was observed.
- F5 attempted reload on the failed page and produced native error feedback.
- The Home button navigated back to the home address after the failed request.

The first restricted execution could not resolve external domains. The network-enabled
execution subsequently rendered real HTTPS pages. Runtime testing found and fixed a
toolbar initialization bug: the immutable embedded HTML URL must be allowed explicitly.

## Not established by these checks

No comprehensive website compatibility, long-running reliability, security audit,
certificate-interstitial behavior, authenticated flows, popup flows, download behavior,
permission behavior, screen-reader review, mixed-DPI/multi-monitor testing, persistent
site-storage restart testing, Windows ARM64 testing or clean-machine installation
testing was performed. Stop, Alt+D, F6, Ctrl+R and Ctrl+Tab are implemented but were
not separately exercised in this smoke test. Successful-page reload was not separately
verified. Current remote CI results are available in the repository's
[Actions history](https://github.com/Niithursan/RustBrowser/actions).

CI produces an unsigned Windows executable. Version tags configure portable ZIP
packaging and checksums; this is not a signed installer. Tabs, bookmarks, privacy
controls and Servo are future work.
