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

## Designed extension, not implemented

A future Servo host driver should be separate from per-page navigation. Its minimum
interfaces and lifecycle are:

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

Do not add unused trait methods for all of this now. A compiled embedder must first
establish exact pinned API shapes, callback ownership, surface constraints, shutdown
completion and handling of late callbacks. In particular, a COM environment, WebView2
profile object, TrySuspend, or native download operation is not a universal engine
capability. Future unsupported features must fail visibly and remain opt-in.

The servoshell prototype is a **separate process**, launched only by scripts under
this directory. It does not implement `ContentEngine`, share the normal application
event loop/UI, or permit selecting Servo in normal tabs. No foreign engine object
is injected into the working application, and no profile format/schema changes occur.

## Recommended next milestone

1. Provision a dedicated Windows source-build environment and compile the pinned
   upstream minimal example before writing a custom adapter. Record cold/warm build
   time, peak memory, disk use and exact toolchain/native dependencies.
2. Implement a separate minimal window with full keyboard/text/IME/pointer forwarding,
   frame scheduling, DPI/resize and graceful engine shutdown. Continue using only
   controlled content and disposable storage, without privileged IPC.
3. Require explicit default-deny delegates, verified profile isolation/deletion,
   popup/scheme policy and a Windows process-isolation/security design before allowing
   wider untrusted browsing. Unsupported Windows sandboxing is not solved by a profile.
4. Only then connect that verified driver to the navigation boundary and expand it
   with capabilities proven necessary by both adapters. Keep production CI and the
   default WebView2 path unchanged throughout.
