# Servo feasibility assessment

Assessed 2026-11-07. Rust Browser baseline is PR #9 squash commit
`b765fa99f5e7ec981fbbf9146adbd66e8209c215`, version 0.6.0. Its
[Windows CI](https://github.com/Niithursan/RustBrowser/actions/runs/42429801650) and
[dependency security](https://github.com/Niithursan/RustBrowser/actions/runs/42429801648)
passed; the baseline tree matches the verified v0.6 feature head.

## Decision before dependencies

Use the user-authorized **servoshell fallback**, outside the application, and extract
only the shared navigation/event boundary that the existing WebView2 adapter can
exercise now. No Servo library dependency, feature-enabled engine selector or normal
profile migration is introduced. A custom embedder is technically possible; it is
not a drop-in replacement for a Wry child view or verified sufficiently here.

The current Servo source inspected was
[`139e0663a398598c7f780c2dd2b7f070ea55bfe5`](https://github.com/servo/servo/tree/139e0663a398598c7f780c2dd2b7f070ea55bfe5).
Its crate/API documentation identifies 0.8.0 and its toolchain pins Rust 1.99.1.
For a reproducible executable, this spike instead pins the latest published Windows
release found during assessment: **Servo v0.7.0**, released 2026-10-05, source
`aac43a3f31a259f04a574f5ec4e959c943ad7cc7`. This upstream version is separate from
Rust Browser's version. Moving API docs, nightly builds and release sources must not
be assumed identical. Some indexed API pages even show older versions; pinned source
is the authority for the prototype. [Official release](https://github.com/servo/servo/releases/tag/v0.7.0),
[current toolchain](https://github.com/servo/servo/blob/139e0663a398598c7f780c2dd2b7f070ea55bfe5/rust-toolchain.toml).

## Windows and build feasibility

Official Windows instructions require x64 MSVC, Windows SDK >= 10.0.19041, latest
v143 C++ tools and ATL, uv/rustup, then `mach bootstrap` and `mach build`. The native
build also involves LLVM/Clang, CMake/Ninja, SpiderMonkey, and optional media native
libraries. The inspected upstream CI installs LLVM 20.1 and bootstraps GStreamer;
the pinned release bootstrap's package list includes CMake, LLVM, Ninja and WiX 7.
[Windows guide](https://book.servo.org/building/windows.html),
[upstream Windows workflow](https://github.com/servo/servo/blob/139e0663a398598c7f780c2dd2b7f070ea55bfe5/.github/workflows/windows.yml),
[release package list](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/python/servo/platform/windows/winget.json).

Local preflight found VS 2022 Community 17.14.42 and Swift's Clang 21.1.6, but did
not find the required ATL component, uv, standalone CMake/Ninja, or GStreamer at the
checked standard locations/PATH. Swift Clang alone is not proof of a complete Servo
build environment. No full source compile was attempted after this failed preflight;
its elapsed time, peak memory and disk cost are **unavailable**, not estimated.
The pinned source uses Rust 1.97.1 plus components such as rustc-dev/llvm-tools;
the project's 1.99.1 toolchain is left intact. Native library/header/resource setup
must follow Servo's own tooling in a separate checkout.
[Release toolchain](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/rust-toolchain.toml),
[native bootstrap](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/python/servo/platform/windows.py).

The verified prebuilt archive is 103,179,362 bytes; it contains native runtime
dependencies and resources, not a reusable Wry backend. Upstream supports Windows
CI, but replicating its setup, source build, cache and compilation budget in the
existing 30-minute application job has not been validated. Keep that job intact;
it tests the boundary and launcher/fixture without downloading or linking Servo.
The separate repeatable source-build and package-smoke procedures are in [README](README.md).

## Integration responsibilities and blockers

| Area | Supported upstream mechanism | Work required before an in-process adapter |
| --- | --- | --- |
| Event loop | `EventLoopWaker` schedules `Servo::spin_event_loop`; servoshell uses winit. | Add a UI-thread wake event/driver; avoid nested loops and busy polling. Match pinned dependencies and pump after input/navigation. |
| Surface/rendering | `RenderingContext`, `WindowRenderingContext`, new-frame delegate, `WebView::paint`, context `present`. Current default interface is OpenGL/GLES. | Own a surface/context, redraw/animation scheduling, resize/DPI and correct destruction order. Wry's HWND child is not this surface. No verified D3D/WebView2 texture interop. |
| Input | `notify_input_event`, wheel/scroll notifications, focus and scale APIs. | Translate key/text/IME, mouse/touch, coordinate units, focus and accessibility exactly once. The minimal example only demonstrates part of this. |
| Navigation/events | WebView load/back/forward/reload; delegate URL/title/history/load/crash/close callbacks. | Map asynchronous events and per-view navigation sequences; enforce scheme, popup and origin policy before allowing requests. |
| Cookies/storage | Servo storage/network managers; release CLI supports config directory and temporary storage. | Build independent profile/data-clearing adapters and verify completion. A directory is not an OS security boundary. Do not import WebView2 cookies/databases. |
| Permissions | Delegate receives typed requests; embedder supplies decisions and native UI. | Implement default-deny origin-specific policy, revocation, devices, downloads and native dialogs. Existing COM permission adapters cannot be reused. |
| API stability | Builders, delegates and feature/native-library dependencies evolve. | Pin a source revision and matching example; compile before generalizing. No compatibility guarantee across Servo releases. |

Primary API references: [embedding overview](https://doc.servo.org/servo/index.html),
[WebView](https://doc.servo.org/servo/struct.WebView.html),
[rendering context](https://doc.servo.org/servo/trait.RenderingContext.html),
[delegate](https://doc.servo.org/servo/trait.WebViewDelegate.html),
[site data](https://doc.servo.org/servo/struct.SiteDataManager.html),
[official minimal example](https://github.com/servo/servo/blob/139e0663a398598c7f780c2dd2b7f070ea55bfe5/components/servo/examples/winit_minimal.rs).

## Security boundary

The pinned release's `create_sandbox` **panics on Windows**, explicitly reporting
unsupported sandboxing. We do not pass a disabling switch, claim a sandbox, or
substitute this engine for WebView2. A separate ordinary process still runs with the
user's OS permissions; a fresh profile does not protect the filesystem from an engine
vulnerability. This is a blocker to promotion as a daily-use engine.
[Pinned sandbox implementation](https://github.com/servo/servo/blob/aac43a3f31a259f04a574f5ec4e959c943ad7cc7/components/servo/servo.rs#L1417).

The launcher accepts no arbitrary flags and enables no devtools, WebDriver, user
scripts or native IPC bridge. It uses a newly allocated profile and temporary-storage
flag, rejects normal-profile/repository/reparse output paths and verifies the pinned
binary digest. Preparation first verifies the entire official archive. TLS defaults
are retained, with no certificate overrides. The fixture has no device grants,
network callbacks, credentials or remote assets. HTTPS testing is limited to the
fixed public example page. This does not certify TLS failure handling or arbitrary
site safety. Config/data files remained after normal exit; owned run directories are
retained for review and are never reused automatically. No secure erasure is claimed.

The prototype cannot validate Rust Browser's permission UI, private mode, downloads,
bookmarks/session restoration or toolbar isolation inside Servo: these belong to
servoshell's own shell today. See [architecture and next gates](ARCHITECTURE.md).
