# v0.10 rendering feasibility decision

Assessed on Windows 11 x64 against Servo
`65d058b9c6543840c1d986f7cae4520605bf5b5c`, its unchanged Cargo.lock, and
Surfman 0.14.0 from that lockfile. WebView2 remains the production engine.
The production version remains 0.8.0.

## Decision

Keep the working Servo host on its two owned fixtures. The synthetic
AppContainer/LPAC worker demonstrates selected OS boundaries and scoped
IPC. It does not execute Servo. No real Servo renderer has been isolated.
The retained Win32k restriction conflicts with the investigated WGL route.
No mitigation was disabled and no broad capability was granted.

The source review and measured DLL failure justify stopping before a real
restricted rendering prototype. They do not prove every possible Windows
rendering architecture impossible. ANGLE, D3D/WARP, a genuine whole-page
CPU renderer and a separate graphics service remain unverified alternatives.

## Pinned rendering paths

| Path | Source evidence | Consequence |
| --- | --- | --- |
| Existing fixture host | Owns a winit window and `WindowRenderingContext`; makes GL current, paints, presents | Works with ordinary user privileges, outside the restricted worker. |
| `SoftwareRenderingContext::new` | Creates a Surfman connection, software adapter, GL context, generic surface and swap chain | The name does not establish a Win32k-free CPU rasterizer. |
| Locked Surfman Windows WGL backend | `create_software_adapter` calls `create_low_power_adapter`; context/device setup calls `RegisterClassA`, `CreateWindowExA` and `GetDC` | Native window and graphics dependencies remain even for an offscreen generic surface. |
| `OffscreenRenderingContext` | Holds `Rc<WindowRenderingContext>`; construction and parent rendering use the same native GL context/framebuffer | This API does not supply a cross-process surface transport or independent restricted worker. |
| Vello CPU canvas backend | Implements DOM canvas drawing in `components/canvas` | It is not a replacement for the whole-page WebRender compositor. |
| Restricted DLL probe | System32 `opengl32.dll` is readable but initialization returns 1114 in both workers with Win32k disabled | A prerequisite fails in the tested configuration; no Servo surface was subsequently created there. |

The decisive sources are the pinned
[Servo rendering context](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/shared/paint/rendering_context.rs)
(software constructor near line 295 and offscreen context near line 729),
[canvas implementation](https://github.com/servo/servo/tree/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/canvas),
and the locked Surfman
[WGL connection](https://docs.rs/crate/surfman/0.14.0/source/src/wgl/connection.rs),
[context](https://docs.rs/crate/surfman/0.14.0/source/src/wgl/context.rs), and
[device](https://docs.rs/crate/surfman/0.14.0/source/src/wgl/device.rs).
The OpenGL return code is runtime evidence; the rendering incompatibility
assessment combines that evidence with these exact source paths.

## Assessed broker/worker boundary

| Responsibility | Implemented evidence | Required before real content |
| --- | --- | --- |
| Browser UI, tabs, permissions and normal profiles | Existing WebView2 application is unchanged | Keep untrusted Servo parsing and script execution outside its process. |
| Restricted launch | Restricted LUA primary token with Administrators deny-only and maximum privileges disabled; suspended lowbox process, expected package SID, zero capabilities, low integrity, non-elevated token, UIAccess off, enabled privilege allowlist, Win32k and child restrictions, creation-time one-process Job | Validate a real engine deployment and dependencies under the same controls. |
| Inherited authority | Ordinary probe inherits none; diagnostic worker receives exactly two pipe endpoints through an explicit handle list; null standard handles | Audit every future duplicated handle and its rights. Never inherit a token, job, window, profile or unrestricted file handle. |
| IPC | Fixed 48-byte numeric frames, strict header/size/kind/sequence/run identity/fixture identifier/opcode, bounded counts and read deadlines | A compromised worker can lie about results. No reported origin or successful diagnostic may authorize browser operations. A real broker needs separate host-owned navigation/permission generations. |
| Files and storage | Fresh generated identity and ownership journal; package-only staged read/execute; synthetic private read/write denial | Broker-approved immutable document/resource bytes, bounded storage quotas and per-origin lifetime. No normal browser cookies or profiles. |
| Networking | Parent TCP/UDP positive controls, standard-worker timeout/nonreceipt, LPAC startup error | DNS, IP families, redirects, TLS, credentials, cookies and all transports need their own policy and positive/negative tests. General containment is unresolved. |
| Resource lifetime | 64 MiB diagnostic commit cap, 1 MiB allocation control, rejected 128 MiB allocation, denied descendant, malformed/hung worker termination, live-IPC broker crash and exact-profile recovery | Real renderer budgets, late messages, GPU work and longer stress tests. The 64 MiB diagnostic cap is not a proposed Servo budget. |
| Presentation | Existing user-privileged host still paints its fixtures | Select and verify a graphics path compatible with the restrictions before implementing transport. No native GL commands or arbitrary worker handles should execute in the trusted browser. |
| Input/resize | Existing fixture host forwards native winit events; synthetic and manual coverage expanded | Bounded input with host-owned focus/view IDs, coordinate space, resize generation and cancellation; real cross-process route remains unimplemented. |

The implemented channel carries echo and diagnostics only. It cannot read a
requested path, fetch a URL, open a shell, grant a permission or operate on
the production browser. Anonymous endpoints bind it to the exact process
created by the broker. The random run identifier detects stale frames; it
is not website authentication. The sender can forge numeric payloads, and
the broker grants no authority on their basis.

If a CPU frame route becomes feasible, review maximum dimensions, checked
stride/length arithmetic, format, immutable snapshot ownership, bounded
in-flight frames, resize generations and crash cancellation before adding
it. A shared-memory or GPU handle is an authority transfer; the current
numeric IPC protocol deliberately has no such operation. These are design
requirements, not implemented rendering features.

## Acceptance gates for the next milestone

1. Obtain an upstream-supported Win32k-compatible compositor path, or a
   separately reviewed graphics-service design with a minimal rights model.
2. Reproduce only that surface prerequisite in the restricted worker before
   integrating Servo parsing, JS or real navigation.
3. Verify surface ownership, frame validation, failed startup, worker/host
   termination and cleanup with synthetic bytes and fixed workloads.
4. Resolve the network-policy gaps and review IPC independently before
   considering arbitrary websites. No current result authorizes that step.

The [threat model](isolation/THREAT_MODEL.md),
[diagnostic evidence](isolation/DIAGNOSTICS_V010.md) and
[validation record](VALIDATION_V010.md) define the remaining scope.
