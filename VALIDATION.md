# Validation

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
First PR-head `694f4de` Windows aggregate and audit passed; final-head evidence and
CI timing analysis will be recorded before marking ready.

Actual installed-application GUI smoke passed in a new disposable profile: 0.13.0
title and globe icon, new-tab chrome, address entry, public HTTPS rendering/loading,
Ctrl+T and Ctrl+Tab, normal Alt+F4 close and restart preserving two tabs/active URL.
The shipping executable hash was `3e787b4102a1e2ec1aa6786b04cd01c0e95ac2eccb0e9a45f9ecdc7153399989`.
No complete installer wizard interaction or clean-machine install claim is made.
Unverified: Windows 10, missing/obsolete runtime host, actual standard-user clean VM,
mid-copy kill/full disk/power-loss transaction, signed artifacts/update infrastructure,
physical mixed DPI/IME/screen reader and real-profile upgrade. Existing v0.12 browsing
limitations remain. See [installer design](packaging/windows/README.md),
[install/recovery](INSTALL_WINDOWS.md) and [update security](packaging/windows/UPDATE_SECURITY.md).

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
[ACCESSIBILITY.md](validation/ACCESSIBILITY.md). Compatibility additions:
[COMPATIBILITY.md](validation/COMPATIBILITY.md). CI changes and cold/warm timings:
[CI.md](CI.md).

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
  See [the acceptance matrix](validation/DAILY_DRIVER.md) for precise limits.

[Corrected matched measurements](benchmarks/V011_RESULTS.md) record two fresh/warm
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
The [detailed record](experiments/servo/VALIDATION_V010.md) includes commands,
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
[detailed validation record](experiments/servo/VALIDATION_V09.md) describes
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
  build procedure and [detailed results](experiments/servo/EMBEDDER_RESULTS.md)
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
  [Servo results](experiments/servo/RESULTS.md) was checked against its raw values
  and fixture digest. It is not a controlled engine-only or statistical comparison.
- A full Servo source build (`mach bootstrap`/`mach build`) was not performed. The
  required dedicated native-toolchain environment and pinned Servo toolchain are
  separate from Rust Browser's; feasibility constraints are in
  [Servo research](experiments/servo/RESEARCH.md). Real-site compatibility,
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

See [compatibility matrix](validation/COMPATIBILITY.md) for exact limits and
[test procedures](validation/README.md) for repeatable 30–60 minute interactive and
automated workloads. Physical sleep/wake, real accounts/devices, full-disk hardware
failure, 125/150/200% mixed-DPI/multi-monitor and spoken screen-reader acceptance
remain unverified. Historical v0.5 benchmark numbers below are not v0.6 measurements.
The 30-minute automated soak passed 178 churn cycles, all twenty final responsiveness
checks, close-to-one recovery and normal shutdown. It is distinct from the shorter
actual GUI checks, and includes documented concurrent activity and sampling limits.
See [SOAK.md](validation/SOAK.md) for numeric evidence and
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
[benchmark procedure](benchmarks/README.md) and [results](benchmarks/RESULTS.md).

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
data, numeric samples, exact methods and variability are in [RESULTS.md](benchmarks/RESULTS.md).

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
