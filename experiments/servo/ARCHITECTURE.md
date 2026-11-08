# Engine integration boundary

## Implemented in v0.7

`src/engine_api.rs` is an engine-independent library module. `ContentEngine` defines
validated page loading, native navigation actions, history availability, visibility
and focus. `WebLocation` accepts only bounded absolute HTTP(S) URLs without
credentials; internal blank loading requires a separate `LoadTarget::Blank` variant.
Trusted new-tab HTML remains a separate toolbar concern. No JavaScript execution or
IPC capability is offered through the shared trait.

`src/engine.rs` implements the interface for the **existing WebView2 adapter**.
Application load/navigation/focus/visibility paths use it. Native history remains
native; the refactor does not maintain a second URL-only Back/Forward stack. Existing
COM permission, download, popup, clearing, fullscreen and sleeping adapters remain
explicit platform integrations. The application still owns concrete WebView2 views;
this is not full engine interchangeability or a Servo implementation of the trait.

Owned `PageEvent` values move through the event proxy. Their navigation IDs are
opaque adapter-local sequences, not COM interfaces or required Servo identifiers.
`HostIntegration` distinguishes native-child presentation/native input from future
host-presented/forwarded input. It describes responsibilities; it does not claim
that an unimplemented renderer works. Views need not be `Send` or `Sync`. Host-owned
lifetime and Drop order release views before their context and native parent.

Regular tests cover privileged/ambiguous URL rejection, Unicode expansion limits,
URL/session-policy agreement and transfer of owned failure/completion events. The
existing native UX regression now invokes a `dyn ContentEngine` against a real view
for history, Back/Forward, Stop, visibility, focus and integration metadata. Existing
tests also exercise load, Home/blank and crash/Reload through the new adapter.

## Implemented in v0.8

The [standalone source example](embedder/main.rs) embeds one Servo view and
owns its winit event loop, window, and WGL surface. It stages into the pinned
Servo workspace and uses the upstream lockfile and keyboard translation.
It shares `NavigationCommand` with Rust Browser. Its two file fixtures use a
separate [policy](embedder/policy.rs) because the production `WebLocation`
validates HTTP(S) locations.

The Servo waker and delegates post small events to winit. The host pumps Servo
on the UI thread and requests redraw on frame notifications. On redraw it
makes the context current, paints the view, and presents. Native keyboard,
pointer, wheel, focus, size, and scale events flow to the view. On shutdown
the host drops the view, engine, surface, and window in that order. The
example has a bounded presentation-and-shutdown smoke check.

The embedder and the v0.7 servoshell launcher both use separate executables.
The production application keeps its WebView2 adapter, event loop, UI, and
profile formats. See [measured capabilities and limits](EMBEDDER_RESULTS.md).

## Future shared driver boundary

The [v0.9 threat model](isolation/THREAT_MODEL.md) evaluates a trusted broker
and an AppContainer worker. The standalone synthetic probe tests Windows
mechanisms independently of Servo. The current Servo executable continues
to host content in its own user-privileged process. The new numeric report
channel carries fixture results only and grants no native actions.

If both engines eventually share a host, keep the rendering driver separate
from per-page navigation.

| Direction | Interface/event responsibility |
| --- | --- |
| Engine thread → host | Wake token only; enqueue to the existing winit event loop. |
| Host UI thread → engine | Pump pending engine work after wake, input and navigation. |
| Engine → host | Frame-ready notification marks a specific view/surface dirty and requests redraw. |
| Host → rendering driver | On redraw: make the context current, paint the dirty view, then present. Native-child WebView2 does this internally and receives no duplicate paint calls. |
| Host → input driver | Logical key plus physical key/modifiers, committed text/IME, pointer position/button/wheel and focus; translate into Servo's pinned input types and physical/CSS coordinate spaces. Native child controllers keep their native route. |
| Host → surface driver | Physical size, scale, minimized/occluded state and parent lifetime. No COM HWND assumption in a Servo renderer. |
| Engine → application | Owned URL/title/load/history/error/close events mapped to tab IDs; separate typed permission/popup/download requests with cancellation/revocation lifetimes. |
| Host shutdown | Stop new requests, deny/cancel pending privileged operations, drop views, finish engine shutdown as supported, release contexts/surfaces, then profile lease and window. |

Extend the interface only when both engines demonstrate a need for the capability.
COM environments, WebView2 profile objects, TrySuspend, and native download operations
remain adapter-specific.

## v0.10 broker and rendering assessment

The [pinned rendering decision](SECURE_RENDERING_V010.md) reviews window,
software and offscreen paths. The locked WGL backend still needs native
window/graphics APIs. Both restricted synthetic workers return OpenGL
initialization error 1114 under the retained Win32k policy. No real Servo
renderer or frame transport is implemented inside that boundary.

The synthetic broker now delegates two pipe endpoints through a creation-time
handle list and verifies the suspended worker's token before resume. Fixed
numeric frames have strict instance, sequence, fixture and operation checks;
deadlines, a one-process job and a 64 MiB diagnostic cap bound failures.
This channel carries diagnostics only and is separate from browser IPC.
It supplies no website-origin authority, resource fetch or graphics command.

Next establish a supported graphics prerequisite under the restrictions or
an independently reviewed graphics-service design. Preserve fixture-only
navigation while network policy, frame/handle rights and real containment
remain unresolved. See [measured evidence](VALIDATION_V010.md).

## Earlier v0.9 next steps

1. Establish Windows process isolation and verify its filesystem and network
   boundaries with synthetic content before allowing wider browsing.
2. Exercise IME, mixed-DPI input, and late callbacks with repeatable fixture tests.
   Verify the lifetime of each observed descendant process.
3. Instrument equivalent presentation and input endpoints on both engines before
   comparing them. Keep the source build independent of production CI.
