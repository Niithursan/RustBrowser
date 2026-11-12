# v0.11 daily-driver acceptance record

This is a verified development subset, **not production-readiness certification**.
Baseline: PR #13, squash `729198d023384103da0464f1148031aa6e681566`, production
0.8.0. Manual GUI rows used production code `58cdef9e9017e76a03c01baebbf9b44ad17dffcd`,
version 0.11.0; later production fixes (`c3ed142` navigation supersession, `99b7f2e`
minimized focus) are covered by native/automated rows, not a repeated manual GUI pass.
Measurement and soak rows use `d77d9c9277f968e58d9b01f1db7680e0eb8aeec7`.
See [architecture](../ARCHITECTURE.md), [performance](../benchmarks/V011_RESULTS.md)
and [Windows installation](../INSTALL_WINDOWS.md).

## Environment and evidence rules

Windows 11 Pro 10.0.26200 x64, Ryzen 5 5600/12 logical CPUs, 31.93 GiB RAM,
Rust 1.99.1/MSVC, Windows SDK 10.0.22621.0, WebView2 157.0.4341.42. GUI used
100% scaling and a copied shipping release executable in a fresh disposable
profile outside Git, on 2026-11-09 (Toronto). No account credentials or physical
camera/microphone grants were used. Public-site observations are point-in-time
smoke checks, not full-site/provider certification.

**Pass** means the stated limited assertion was observed. **Not tested** means
the broader scenario remains open. Native automation uses actual WebView2 with
test-only CDP/fixture hooks; it is separate from GUI inputs through Windows.
Historic v0.6 GUI results in [COMPATIBILITY.md](COMPATIBILITY.md) are not new
v0.11 manual results. No failing assertion was removed to obtain a pass.

## Acceptance checklist

| Scenario / repeatable assertion | v0.11 result | Evidence and limit |
| --- | --- | --- |
| Launch ordinary release executable with an empty isolated profile | Pass | Actual GUI new-tab page/window title 0.11.0; no elevation prompt. Not a clean-machine Runtime test. |
| Enter `https://example.com/`, wait for page and Ready state | Pass | GUI text rendered; HTTPS origin shown; loading progress appeared and stopped. No certificate bypass. |
| Open `https://www.rust-lang.org/` in a second tab | Pass | GUI completed redirect to `https://rust-lang.org/`, rendered content/links, correct origin and Ready state. Entire site/assets not certified. |
| Modern JS/module/fetch/worker/crypto/IndexedDB/cookie/local storage/SPA | Pass | New native loopback suite checks explicit results; two-port redirect/storage boundary and Back passed. Real application suites remain untested. |
| Create/close/switch tabs | Pass | GUI Ctrl+T, Ctrl+W and Ctrl+Tab with surviving pages; native stable-controller/overlap checks. |
| Reorder and pin | Pass | GUI drag swapped tabs, Ctrl+Shift+P pinned/moved tab; restoration retained identity/order/pin. |
| Address focus, draft protection and shortcuts | Pass | GUI Ctrl+L and literal address entry; native background-title/draft/selection checks; unit AltGr/repeat routing. Physical AltGr layout/OS key auto-repeat not tested. |
| Light/dark chrome and focus | Pass | GUI started Dark, selected Light through dropdown keyboard and saw coherent light controls; native field/color/layout checks. Contrast audit/spoken screen reader not tested. |
| Bookmark current page and inspect manager | Pass | GUI Ctrl+D saved Rust page; Ctrl+Shift+O found same bookmark after restart. Edit/folder/remove are automated model coverage, not new manual results. |
| History display | Pass | GUI Ctrl+H showed the two successful public visits. Duplicate/date/search/filter/removal/persistence have regular tests. |
| Download panel | Pass | GUI Ctrl+J showed an empty panel and destination/no-overwrite explanation. Not a manual file transfer. |
| Controlled HTTP downloads/uploads/media/fullscreen/popups | Pass | Preserved native suite completed synthetic downloads/collision handling and upload/media/popup/redirect assertions. Native folder selection and user gestures are fixture supplied. |
| Real HTTPS file transfer, native file picker, long/resumable transfer | Not tested | Historical manual picker evidence is retained separately. No new real-site upload/download or automatic restart-resume claim. |
| Real video/audio hardware, DRM/codec coverage, conferencing | Not tested | Controlled PCM and generated video tests do not prove hardware sound, camera/mic, STUN/TURN or provider compatibility. |
| Popup authentication/opener/cookies | Pass | Native synthetic opener/message/cookie/noopener/script-close suite rerun. |
| Personal/federated sign-in and payment sites | Not tested | No account credentials used; providers can reject embedded browsers/external callbacks remain blocked. |
| Private storage, permission decisions/revocation and data clearing | Pass | Native isolation/confirmed clearing regressions plus model tests; private UI opens no normal diagnostic store. No new manual permission/settings changes. |
| Untrusted HTTPS certificate | Pass | New native TLS server uses fresh self-signed key/cert outside Git; browser rejected certificate with specific advice. Trust store unchanged. Trusted local HTTPS positive path not tested. |
| Normal close/restart and profile preservation | Pass | Two GUI launches/normal closes; tabs, active selection, pinned state, Light and bookmark survived. Existing data paths/schema unchanged. Upgrade from a real old user profile not performed. |
| Renderer crash/reload | Pass | Existing native test deliberately crashed only its disposable renderer and recovered through Reload. Unsaved form state is not restored. |
| Host termination/session/SQLite and diagnostics | Pass | Owned child termination tests: committed snapshot/bookmark survives, SQLite uncommitted transaction rolls back, profile lock releases, next diagnostic run sees unclean exit. Not every power-loss timing. |
| Corruption, future schemas and I/O failure | Pass | Regular session/library/diagnostic tests preserve originals and report controlled failures. Real full disk/hardware failure not induced. |
| Profile locking/concurrent diagnostics | Pass | Existing profile lock plus new journal lock tests prevent a second writer/clear. No manual bypass/removal of live locks. |
| Sleeping-tab lifecycle and protected activity | Pass | Existing native state/controller/resume tests plus guard/race unit tests. Arbitrary activity cannot always be detected; Keep awake remains necessary. |
| Extended stability/resource sampling | Pass | Candidate `d77d9c9`, fresh disposable profile, 20 tabs, 1799 s synthetic churn soak: 178 cycles, zero navigation errors, normal shutdown, journal only start/clean-shutdown, all final switches responsive. See [results](../benchmarks/V011_RESULTS.md). Not an interactive real-site, media or download session; no leak-freedom claim. |
| Warm-profile tab close (corrected campaign failure) | Not reproduced | One `Tab closing failed` at `28b292a` during a locked-desktop period; 28 later runs (12 on the pre-fix binary) passed. Minimized-window focus rejection is fixed and regression-tested; the original root cause is unconfirmed. |
| High DPI/mixed monitors/IME/screen-reader/system sleep | Not tested | Native fixture synthetic scale/composition checks preserved. Physical configurations and Windows sleep/wake remain separate work. |
| Portable integrity/metadata/profile-output refusal | Pass | Two equal-input ZIP hashes, exact file allowlist, all manifest hashes and unsafe path/profile/reparse refusals passed. Unsigned x64/asInvoker resources and VC Runtime DLL imports inspected. |
| Clean-machine install/uninstall/rollback/signing | Not tested | MSI/MSIX evaluated; portable manual procedure selected. No installer/registry/auto-update change or signed artifact. |

## Native reproduction

Run production formatting, strict Clippy, regular tests, debug/release builds and
audit as in the README. Run the seven existing native commands in CI separately
(one winit event loop per process), then:

```powershell
$scratch = Join-Path $env:TEMP ('rust-browser-daily-' + [guid]::NewGuid().ToString('N'))
.\validation\windows-daily.ps1 -OutputRoot $scratch
```

This requires Python 3 with standard-library SSL and OpenSSL (PATH or Git for
Windows's `usr/bin/openssl.exe`, or explicit `-OpenSsl`). It generates a short-lived
synthetic key/cert, binds random loopback ports, installs no certificate and
terminates its retained owned TLS server in `finally`. Test keys, browser profiles,
downloads and raw output stay outside Git and must not be attached to public PRs.
Output inside the repo, known profiles or reparse ancestry is refused. Local
diagnostic clearing is asserted only through an explicit confirmed fixture command;
unconfirmed requests must fail and the synthetic bookmark must remain.

The initial Schannel in-memory certificate helper failed to acquire its key
(`SEC_E_NO_CREDENTIALS`) and caused a connection reset instead of a certificate
error. Replacing the fixture with OpenSSL/Python preserved the original TLS
rejection assertion. Native Stop initially returned status 9 and failed the test;
the host now associates it with a recorded Stop rather than suppressing errors.

Hosted CI also exposed intermittent loopback navigation failures: one old run
timed out at phase 2, and a later run reached phase 6 with native failure state.
The fixture now reports per-API stage failures and native navigation status
immediately. No navigation/storage assertions were removed, timeout increased,
or success-after-retry rule added. Production CI requires three independent
fresh-profile daily runs and fails on any one failure.

Identified cause (test infrastructure, not production): the in-process loopback
fixture server accepted connections from a non-blocking listener, and on Windows an
accepted socket inherits non-blocking mode. When the accept loop won the race
against WebView2's request bytes, the first read returned `WouldBlock`, the fixture
dropped the connection without a response, and WebView2 reported status 0 with no
HTTP code (push CI run 42679469407, phase 2). A standalone check and a new
deterministic regression test reproduced the empty response; restoring blocking mode
on accepted streams fixes it. All navigation assertions are unchanged.

**Correction - a second, production cause.** Main CI at `fef457a` (run
42710344722) failed after the fixture fix with WebView2 status 9 in the overlapping
navigation phase. Two host loads close together let the first load's
`NavigationStarting` arrive after it was superseded; the adapter only rejected late
`about:blank` starts, so the stale web start became current again and its abort
was shown as a connection error. A new native phase issuing two loads in one
event-loop turn failed 10/10 before the fix and passed 10/10 after it. The adapter
now rejects a late start only when its normalized URI matches a remembered
superseded host target (bounded), and any unmatched start is accepted as before.
Earlier phase-6/phase-9 hosted failures were not individually re-traced and may
belong to either cause. Not covered: loading the same URI twice in quick
succession, and Back/Forward/Reload issued over a pending host load.

## v0.14 update

v0.14 closed these gaps from the list below: complete loss of the session after an
engine-process exit (now recovered), and misleading failure text for server errors,
unreachable hosts, downloads and app links. See
[COMPATIBILITY.md](COMPATIBILITY.md) for the public-site survey and
[../VALIDATION.md](../VALIDATION.md) for measurements. Everything else in the
list remains open, together with: a second normal window per profile, recovery of
the toolbar's own engine process, and the unexplained WebView2 crash itself, which
is avoided by delaying view destruction rather than fixed.

## Remaining daily-driver blockers

Broader account/provider compatibility, device media/WebRTC/DRM, accessibility and
mixed-DPI acceptance, clean-machine Runtime/upgrade tests, signing/installer and
authenticated-update design remain open. Downloads are not restart resumable;
sessions restore URLs/settings, not forms or live script state. Diagnostics are
best effort and local; no minidump/upload or complete engine-crash reporting exists.
Public smoke checks and a synthetic soak do not establish general daily-use safety,
leak freedom, or replacement parity with Firefox/Chrome. Keep WebView2 updated and
Servo fixture-only. Prioritize these production validation gaps in the next milestone.
