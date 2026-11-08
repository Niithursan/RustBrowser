# v0.9 isolation and event validation

The branch starts at the verified PR #11 squash commit
`469cddf2c080f7b1fa0a8dcd2c5c6993fbe3effa`. The merged
[Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42504774058)
and [dependency audit](https://github.com/Niithursan/RustBrowser/actions/runs/42504774037)
passed before branch creation. Production package version remains 0.8.0.
The investigation has not established a contained Servo renderer.

## Isolation findings

The [threat model](isolation/THREAT_MODEL.md) defines the candidate broker,
renderer, network, filesystem, IPC, graphics, and lifetime boundaries.
The [synthetic probe procedure](isolation/README.md) records the actual
AppContainer/LPAC observations and the blockers. It uses separate native
executables compiled by the Windows SDK and introduces no Rust dependency.

The standard AppContainer suite verifies token identity and mitigations,
owned file/process/handle tests, failed startup, worker termination,
last-job-handle closure, and broker-crash recovery. The loopback timeout is
inconclusive. The explicit LPAC suite fails at Winsock initialization with
error 10107 and reports profile cleanup. Servo remains restricted to its
two generated file pages. No renderer is launched inside this prototype.

## Shared controlled fixture

[validation.html](validation.html) runs the same script on both engines.
The hosts parse a bounded numeric title report, and the page exposes no
native command channel or network callback. WebView2 permits only blank
and the exact base64 representation of the built-in document. Servo uses
its existing two-file resource/navigation policy.

| Check | WebView2 | Pinned Servo |
| --- | --- | --- |
| Synthetic keyboard event and DOM focus | Passed | Passed |
| Synthetic pointer coordinates and transformed hit test | Passed | Passed |
| DOM composition event sequence and Unicode payload | Passed | Constructor unavailable with pinned default preferences |
| Engine raster/device scales 1, 1.25, 1.5 and 2 | Passed | Passed |
| Viewport dimensions match physical size divided by engine scale | Passed within 2 pixels | Passed within 2 pixels |
| Report parser rejects nonfinite, oversized and malformed values | Passed | Passed |
| Native IME translation unit regressions | Engine handles input natively | Unicode update/end, preedit, start and stale dismissal tests passed |
| Actual Windows IME candidate UI, physical input and monitor transitions | Unverified | Unverified |

The Servo host now handles input-method enable/dismiss requests and forwards
winit composition events using the pinned servoshell translation. That route
has unit coverage. The pinned `dom_composition_event_enabled` preference is
false. The test reports unsupported composition explicitly and keeps the
default preference. Converted wheel distances now use pixel units, matching
the pinned servoshell's unit convention.

## Reproduce

Run the standard production checks and all six existing native regressions.
The production Windows workflow additionally runs the shared WebView2 fixture.
This fixture can be invoked separately.

```powershell
cargo test --locked --bin rust-browser native_servo_comparison_fixture -- --ignored --test-threads=1 --nocapture
```

For Servo, follow [SOURCE_BUILD.md](SOURCE_BUILD.md), including staging
`input.rs`, `validation.rs` and `validation.html`. Compile and run the six
example unit tests, then invoke the native validation and existing smoke checks.

```powershell
.\experiments\servo\embedder\check-validation.ps1 -Executable $exe -OutputRoot (Join-Path $scratch 'validation') -Repeats 3
.\experiments\servo\embedder\check-smoke.ps1 -Executable $exe -OutputRoot (Join-Path $scratch 'smoke')
```

Use independent disposable profiles and run one engine at a time. Complete
builds and other tests before collecting timing observations. Record
hardware, runtime and executable digests. Retain failed runs and avoid
mixing them with the measured set.

## Timing endpoints and measured scope

- `ready_raf_ms` measures from the script after its DOM markup to resolution
  of the second requestAnimationFrame callback.
- `pointer_raf_ms` and `keyboard_raf_ms` measure script dispatch of one
  synthetic DOM event through the next animation-frame callback.
- `composition_raf_ms` uses the same callback endpoint after the synthetic
  composition/input sequence when that constructor is supported.

These endpoints use the document's performance clock. They exclude process
startup and physical device input. Animation-frame callbacks occur before
compositor presentation. Physical first-pixel, physical-input-to-display,
and OS IME latency remain unavailable.

Three fresh-profile runs per engine completed locally on 2026-11-08, on
Windows 11 Pro 10.0.26200 x64, Ryzen 5 5600, 31.93 GiB RAM and WebView2
157.0.4341.42. WebView2 ran first, followed by Servo. Each run covered
four scales in ascending order. Both builds were unoptimized; WebView2 used
the Rust test build and Servo used the separate dev build without debug info.
No builds or other test suites ran during the measured set.
Raw numeric observations and fixture/executable hashes are in
[validation-v09.json](validation-v09.json).

The scale-1 medians and ranges illustrate the observed variability.

| Script endpoint | WebView2 median [range], ms | Servo median [range], ms |
| --- | --- | --- |
| Ready to second frame callback | 108.0 [106.4–113.9] | 21.06 [20.59–55.87] |
| Pointer dispatch to frame callback | 0.9 [0.5–3.6] | 69.87 [17.44–117.04] |
| Keyboard dispatch to frame callback | 3.2 [3.2–3.3] | 41.85 [41.34–43.22] |
| Composition sequence to frame callback | 114.2 [109.3–117.3] | Unavailable |

Three trials per engine, engine-specific scheduling, timer precision, separate
test hosts, file versus in-memory document origins, conditional composition
support, and uncontrolled desktop/system activity limit comparison. These
measurements establish shared endpoints and expose variability. They do not
establish which engine delivers faster physical input or visible presentation.

## Manual acceptance still required

On a disposable Windows desktop, validate native key down/up and repeat,
shortcuts, pointer press/release outside the view, wheel direction, focus
loss, and resize. For each installed IME, validate preedit replacement,
candidate selection, cancellation, commit, focus changes and Unicode text
without duplicated characters. Record installed input service versions.

Use 100%, 125%, 150% and 200% Windows display settings and multiple monitors
to validate actual WM_DPICHANGED delivery, cursor location, IME candidate
position, minimizing and restoring, and physical input hit-testing.
The automated engine-scale tests do not change those Windows settings.
These manual configurations were not verified in this milestone.

The next security milestone should resolve the documented LPAC service
requirements, validate a sandbox-compatible rendering/broker path, and
test explicit network denials before expanding the Servo content policy.
