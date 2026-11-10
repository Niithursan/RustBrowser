# Compatibility matrix

## v0.14 public-website survey and controlled fixtures

Windows 11 Pro x64 (10.0.26200), WebView2 Runtime as installed on the test machine,
2026-11-09. `validation/site-survey.ps1` drives the **production session** (test
executable, disposable profile): a new tab per address, wait for completion, read
the tab state and a small page probe, close the tab. No credentials were entered, no
permission was granted, and no personal account was used. These are observations of
third-party sites on one day and one network, not a compatibility guarantee.

Final run on the v0.14 candidate, 29 addresses, 0 timeouts, 0 engine or renderer
exits, 0 crash dumps:

| Category | Addresses | Result |
| --- | --- | --- |
| Search engines | Google, DuckDuckGo, Bing result pages | Loaded |
| Documentation and news | Rust book, MDN, Wikipedia, BBC News, Hacker News | Loaded |
| JavaScript-heavy applications | GitHub repository, react.dev, Excalidraw | Loaded |
| JavaScript-heavy applications | OpenStreetMap | Answered HTTP 429 to this client; shown as such |
| Video and embedded media | YouTube and Vimeo home pages | Loaded; playback was not exercised |
| Sign-in pages, authentication redirects | GitHub, Google (3 redirects), Microsoft (3 redirects) | Sign-in page loaded; nothing was entered |
| Scheme upgrade, redirect chains | `http://github.com`, `http://example.com`, httpbin 5-hop and absolute 3-hop | Loaded at the final address |
| Service-worker sites | web.dev, Squoosh, Excalidraw, YouTube (controller present after load), x.com | Loaded |
| Server errors | httpbin 404 and 500 | Reported as the website's HTTP answer |
| Unresolvable name | `.invalid` host | Failed once, with name-resolution advice (status 13) |
| Certificate errors | expired and self-signed (badssl.com) | Failed with certificate advice; no bypass |

`window.ipc` was undefined on every surveyed page.

Defects found by the survey, all Rust Browser integration defects except where noted:

| Finding | Classification | State in v0.14 |
| --- | --- | --- |
| WebView2 browser process ends with an access violation when a visible view showing the Microsoft sign-in page is destroyed (tab close 6/8, window exit 8/8). No application event handler is involved (each was disabled in turn). | WebView2 engine defect, triggered by destroying a visible view | Avoided: views are hidden for one second before destruction (0/12 and 0/8 after). A burst of 17+ closes still destroyed one early under the first 16-view cap (10/12); up to 64 are now held (0/13) |
| After that exit every tab failed with `0x8007139F` | Integration defect: no handling of browser-process exit | Fixed: views are rebuilt and reloaded |
| HTTP 404/429/500 shown as "page could not be loaded (status 0)" | Integration defect | Fixed |
| Unreachable host shown with the wrong reason (status 9) | Integration defect: the engine's follow-up navigation was treated as a new failure | Fixed |
| Download navigation marked the page failed | Integration defect | Fixed |
| `mailto:` link marked the page failed | Integration defect | Fixed |

Controlled fixtures added to the native suite (loopback only): 404 and 503 pages,
a refused connection and its Reload, a blocked app link, a download started from the
address bar, tab retirement timing, engine-process termination and restart, reopen
closed tab and select-by-position. Popup checks (script-written `about:blank`
popup, `noopener` popup, link download inside a popup tab) were observed once with a
temporary probe and behaved correctly; they are not new automated assertions.

Not tested in v0.14: signing in to any account, popup-based sign-in with a real
provider, real file uploads to public sites, video playback and DRM, notification
and permission prompts on public sites, WebRTC calls, multiple windows (not
supported), and any site behind a login.

Current v0.11 results are in [DAILY_DRIVER.md](DAILY_DRIVER.md): the new native
modern-site/certificate/navigation/diagnostic suite and all seven previous native
regressions passed, with actual release GUI HTTPS/tab/bookmark/theme/restart checks.
Real provider/device/accessibility limitations remain open. The historical matrix
below is retained with its original v0.6 evidence dates and scope; it must not be
interpreted as freshly repeated v0.11 manual testing.

## v0.12 additions (controlled native fixtures)

Windows 11 Pro x64, WebView2 157.0.4341.42, loopback fixtures only. No public
sites, accounts or credentials were used for these rows.

| Area | Verified evidence | Limits |
| --- | --- | --- |
| Service workers | The modern fixture registers `/sw.js` on loopback (a secure context), waits for `ready`, round-trips a `MessageChannel` message, then unregisters. Strict page CSP retained (`worker-src 'self'`). | No fetch interception, offline navigation, push or background sync is exercised. Registrations of real sites persist in the profile like other site storage. |
| Cache Storage | `caches.open/put/match/delete` round trip. | Quota and eviction behaviour are not tested. |
| structuredClone, AbortController, BroadcastChannel | Map/typed-array clone, aborted fetch rejects with `AbortError`, same-origin channel delivery. | Feature presence only. |
| Rapid successive navigations | New native phase: two host loads in one event-loop turn. The superseded load's late start no longer becomes current or reports a false error (fix PR #15; 0/10 before, 10/10 after). | Same URI loaded twice in quick succession, and Back/Forward/Reload over a pending host load, are not covered. |
| Existing coverage | fetch, modules, workers, WebCrypto, IndexedDB, cookies, localStorage, SPA history, two-origin redirect/storage isolation, popups/opener/auth, upload, audio, fullscreen, downloads, renderer crash recovery, untrusted TLS rejection: all still pass. | As recorded in DAILY_DRIVER.md. |

Known limitation found during v0.12 GUI checks: an unresolvable `.invalid` host is
reported by WebView2 as status 9 (connection aborted), so the toolbar shows "The
connection was interrupted" instead of name-resolution advice. Unmodified `main`
(`fef457a`) behaves identically. This is not a v0.12 regression, and it has not been fixed.

## v0.6 historical matrix

This is a measured working subset, not certification as a daily-use browser. The
local test environment is Windows 11 Pro x64, Rust 1.99.1/MSVC and WebView2
157.0.4341.42. GUI observations used an isolated profile at 100% scaling. No personal
accounts, external credentials, camera or microphone grants were used.

| Area | Verified evidence | Limits and follow-up |
| --- | --- | --- |
| Popup-style authentication | Native and actual GUI fixture: user-marked popup, opener present, shared synthetic cookie, message delivery, script close and return to opener. | Real identity providers, cross-origin OAuth/COOP combinations, federated accounts, external-protocol callbacks and named-window reuse are not comprehensively tested. Providers may reject embedded engines. |
| Popup boundaries | Native page-originated timer blocked when the Runtime reports non-user initiation; `noopener` yields null opener; remote IPC absent in popup/redirect. | The Runtime flag and native browser policies decide what counts as user initiated. Transient activation can cover delayed work; this is not a promise that every programmatic popup is blocked. Popup dimensions are ignored. |
| Cookies/session compatibility | Native cookie/local-storage isolation and confirmed clearing tests remain passing; linked same-profile popup cookie round trip passed. | SameSite, third-party-cookie restrictions, partitioning and provider-specific policies remain WebView2 behavior. Opener/JS/form state is not restored after application restart. |
| Native file upload | Actual GUI file picker selected only `upload-fixture.txt`; loopback receipt showed 313 multipart bytes. Native POST activity prevented suspension. | Large/resumable/multipart/chunked uploads, cross-origin frames and service-worker uploads need broader coverage. No universal upload detection is claimed. |
| Downloads | Native HTTP test completed two 1 MiB fixture downloads, preserved the first file with a suffixed second output, and protected an otherwise quiet background tab while active. | Native folder selection in this regression is supplied by the fixture harness. Long transfers/restarts and every network/protocol failure are not certified; application restart does not resume downloads. |
| Quiet audio | Native PCM playback and media-request protection passed with actual WebView2; native audio query was exercised. | Hardware output, every codec, DRM and all background-audio site policies are not certified. |
| Generated video/fullscreen | GUI synthetic canvas video showed advancing frames. Video fullscreen expanded the window with origin toolbar retained; Escape returned to the normal window. | Muted/generated/MSE streams may not produce observable native media requests. Use Keep awake; no codec/site-wide guarantee or frametime claim. |
| WebRTC | GUI loopback connected two local peer connections and showed matching advancing video frames. With Keep awake on, it remained connected after backgrounding for over two minutes. | This uses synthetic canvas tracks and no STUN/TURN or real devices. Real calls, conferencing providers, device permissions, NAT traversal and automatic WebRTC activity detection remain unverified. |
| Critical background work | Native POST/media classification, active-download guard, granted-document permissions, pending prompts and linked popup guards; pure policy tests cover every explicit v0.5 guard. | Guards are intentionally conservative and can persist until tab closure. Worker events may protect multiple views in the same environment. WebSocket/worker coverage depends on Runtime events; arbitrary site work remains unobservable. |
| Sleep/resume | Actual native `IsSuspended`, same controller/document after resume, callback races and a simulated 31-second event-loop pause recovery passed. | Physical machine sleep/wake, hibernation and device reconnection were not performed. They cannot be inferred from a simulated timing gap. |
| Renderer failure | A disposable renderer was deliberately crashed through test-only CDP; failure feedback and native Reload recovery passed. | Browser-process/OS/device failures can require restart. Unsaved forms, in-flight work and every crash timing are not recovered. No automatic replay of POSTs is added. |
| Host termination/storage | Killed owned child: lock release, committed snapshot restoration, committed bookmark retained, uncommitted SQLite row rolled back; five repeated store lifetimes. Actual GUI test process was killed and its two tabs restored after restart. | Recovery is for committed URL/settings data, not unsaved page state or power-failure guarantees. |
| Invalid data/I/O | Empty, truncated and malformed temporary snapshots preserved; valid temporary recovery when main is missing/corrupt; valid committed privacy preference wins over stale temp; access/write failures remain visible. Existing database corruption/new-schema regressions pass. | A real full disk, hardware write failure and every ACL/filesystem configuration were not induced. Tests use controlled failures and rollback, not a destructive disk-fill experiment. |
| Download staging recovery | Unit/integration tests preserve live leases, payloads, corrupt/unrecognized markers and legacy directories; remove only recognized empty unlocked staging. Existing no-overwrite/collision tests pass. | Scan is bounded to 256 immediate entries in a selected folder. Nonempty and legacy staging need manual review; no global startup scan or automatic payload deletion. |
| Keyboard/accessibility | Actual Ctrl+Tab, native file picker entry, Escape/fullscreen, accessible tab/control names/roles and all-tabs keyboard Home selection observed. v0.5 shortcut/model regressions remain passing. | Full mouse interaction with every native popup, focus/announcement ordering and spoken screen-reader acceptance need wider testing. UIA metadata is not a screen-reader certification. |
| Display/scaling | Actual 100% display, normal wide window and 1920×1080 borderless fullscreen observed; logical scaling code and v0.5 responsive layout are retained. | 125%, 150%, 200%, mixed-DPI monitor moves and multi-monitor acceptance remain untested. Historical v0.5 compact-layout results are not new v0.6 DPI results. |

The extended automated run and its resource observations are recorded in
[SOAK.md](SOAK.md). The reproducible interactive procedure is in [README.md](README.md).

Primary API references: [native popup deferrals and profile requirements](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2newwindowrequestedeventargs),
[UI-thread/reentrancy model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/threading-model),
[request source filters](https://github.com/MicrosoftDocs/webview2-win32-reference/blob/main/1-0-2651-64/icorewebview2_22.md),
[best-effort suspension](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_3).
