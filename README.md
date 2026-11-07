# Rust Browser 0.2.0

A multi-tab, single-window Windows browser starter using Rust, **winit 0.30** and
**Wry 0.57 / Microsoft WebView2**. Websites render in the installed Chromium-based
WebView2 Runtime. This is an early application shell, not an independent Rust web
engine or a daily-use replacement for Firefox/Chrome yet.

## Setup on Windows

1. Use Windows 10/11, preferably x64.
2. Install [Rust stable with rustup](https://rustup.rs/). Choose the default
   `x86_64-pc-windows-msvc` toolchain.
3. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with **Desktop development with C++**, MSVC and a Windows SDK.
4. Install the [Evergreen WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
   if it is missing. Microsoft Edge being installed is not a substitute for checking
   the Runtime. This project does not bundle or install it.
5. Restart your terminal after installing tools. Internet access is needed for
   Cargo's first dependency download and for browsing.

Open this folder in VS Code or PowerShell:

```powershell
cargo build --locked
cargo run --locked
# Optional initial website:
cargo run --locked -- https://www.rust-lang.org/
```

For an optimized executable:

```powershell
cargo build --release --locked
.\target\release\rust-browser.exe
```

CI stores an x64 Windows executable as a build artifact. Version tags generate a
portable release ZIP with a SHA-256 checksum. It still requires WebView2 Runtime.
It is unsigned and has no installer. Run it from PowerShell to see startup errors.
The console remains available intentionally for diagnostics in this initial version.

## Controls

| Action | Control |
| --- | --- |
| Open a website | Address field, then Enter or Go |
| Focus/select address | Ctrl+L, Alt+D, F6 |
| Back / forward | Buttons or Alt+Left / Alt+Right |
| Reload | Button, Ctrl+R or F5 |
| Stop | × button or Escape |
| Home | Home button → https://example.com/ |
| New tab | + button or Ctrl+T |
| Close active tab | Ctrl+W (closing the last tab opens Home) |
| Close a specific tab | Its × button |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab (wraps around) |
| Select a tab | Click its title |
| Focus webpage from toolbar | F7 |
| Move through toolbar | Tab / Shift+Tab |

Bare domains receive `https://`. Explicit `http://` supports local development.
Words and phrases are searched with the selected provider. The toolbar selector
chooses Google (default), DuckDuckGo or Bing and saves that choice automatically.
Queries are URL-encoded; active schemes and embedded credentials remain blocked.
Inputs resembling a domain or localhost address are treated as URLs. Single-label
intranet hosts require an explicit `http://` or `https://` prefix. HTTP is not upgraded automatically when typed
explicitly. The displayed URL includes its scheme; there is no certificate/security
indicator yet. Website navigation, redirects, fragments and History API changes
update the address field. Editing the field pauses updates until focus leaves it.
Back/forward use WebView2's actual history, rather than a separate Rust URL list.

The status line reports connecting/loading, completion, invalid input, blocked
schemes and native WebView2 navigation errors (including a numeric error status).
WebView2 also displays its built-in network and certificate error pages. No
certificate validation bypass is installed. HTTP error responses such as 404 may
render normally and count as completed navigation.

## Data and boundaries

Cookies, site storage and cache persist in `%LOCALAPPDATA%\RustBrowser\v0.1\content`.
The toolbar has its own `toolbar` profile folder. The existing v0.1 folder is reused
to preserve cookies on upgrade. There is no Rust history database,
password manager, profile management UI, clearing-data UI or private mode yet.
Close every instance before manually removing a profile folder.

`session.json` stores tab URLs, order, active tab and search settings in **plain text**
inside this profile. URLs may contain sensitive paths, queries or tokens. No URL log
or page content is saved by the Rust session layer. Keep the profile private and out
of Git; OS account permissions protect it, not application encryption. Session saves
are coalesced on a background thread, atomically replaced and flushed on normal exit.
A profile lock prevents two v0.2 instances from overwriting the same session.
Do not run v0.1 and v0.2 against the same profile simultaneously.

Restart automatically restores all tab URLs and the active tab. An optional command
line URL opens an additional tab. Back/forward history, form data, scroll position and
page JavaScript state are not restored. Up to 64 live tabs are supported; hidden tabs
retain their engines and may continue loading, playing media or using resources.
There is no tab suspension yet. Invalid, oversized or unsupported session files are
backed up as `session-invalid-*.json`, with a visible recovery notice. Disk-write
failures show a notice; check disk space and profile permissions before exiting.

Use a separate profile for testing:

```powershell
$env:RUST_BROWSER_PROFILE = Join-Path $PWD 'test-profile'
cargo run --locked
```

The toolbar is local embedded HTML in a separate child webview. Only it receives a
Rust IPC command bridge. Remote sites receive **no application IPC bridge or injected
application scripts**. Toolbar navigation is restricted to its exact bundled data
URL; remote content navigation is limited to HTTP(S). Address entry rejects
embedded credentials and active schemes such as `javascript:`, `data:` and `file:`.
WebView2 permission requests are denied and downloads are cancelled in this version.
Wry's default extra browser arguments are replaced with an empty argument string to
retain the Runtime's default security features. Media autoplay is disabled.

New-window requests open a new tab. This supports
basic target=_blank links but can break popup-based sign-in flows. There are no
extensions, download manager, printing UI, sync, full crash recovery or default-browser
registration. Permission UI, origin indicators, threat protections and compatibility
need a dedicated review before everyday browsing. Runtime behavior remains dependent
on Microsoft's installed WebView2 version and policy.

## Structure and next steps

```text
src/main.rs           Entry point and Windows platform gate
src/app.rs            Event loop, session lifetime, layout, profile and state
src/navigation.rs     URL normalization and scheme policy, with unit tests
src/tabs.rs           Engine-independent tab IDs, ordering and selection
src/settings.rs       Search provider settings and query encoding
src/storage.rs        Validated snapshots, profile lock and background persistence
src/lib.rs            Testable core shared by application and integration tests
src/engine.rs         Wry content view and Windows COM adapter
src/ui/mod.rs         Toolbar commands and serializable view state
src/ui/toolbar.html   Local accessible toolbar
```

Each tab owns one engine instance with its own navigation state. All content tabs
share the site's WebView2 context, while the toolbar remains separate. Native events
carry stable tab IDs, including background callbacks; late events from closed tabs
are ignored. Switching hides/shows views without recreation. The generic tab model,
settings and persistence do not depend on WebView2. Add bookmarks/history and explicit
privacy/profile services next. Add an engine
trait only when a second backend's actual requirements are known. Servo would need
its own embedding, rendering/input integration and compatibility testing; it cannot
simply be substituted into Wry. No Servo dependency is included.

## Development checks

```powershell
rustup component add rustfmt clippy
cargo fmt --check
cargo test --locked
cargo clippy --locked -- -D warnings
```

See [VALIDATION.md](VALIDATION.md) for checks actually performed and known gaps.
GitHub Actions runs formatting, Clippy with warnings treated as errors, tests and
an optimized Windows build. Dependency security runs `cargo audit --deny warnings`
on main, pull requests and weekly. Dependabot proposes Cargo and Actions updates.

Development uses feature branches and pull requests into `main`, with Conventional
Commits. See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and release steps.
Toolchain version is recorded in `rust-toolchain.toml`; commit `Cargo.lock` updates.
The private repository is [Niithursan/RustBrowser](https://github.com/Niithursan/RustBrowser).

Manual smoke checklist:

- Open HTTPS, follow a link, back, forward and reload; verify URL and button state.
- Focus a webpage and try every shortcut; edit the address without losing text.
- Try a fragment link or History API navigation and verify the address updates.
- Try a nonexistent domain and invalid/blocked schemes; verify readable feedback.
- Resize, minimize/restore and move between displays with different scaling.
- Restart with the same profile and check a non-sensitive test site's storage.

## Upstream references

- [Wry documentation](https://docs.rs/wry/0.57.1/wry/)
- [Wry Windows extension](https://docs.rs/wry/0.57.1/x86_64-pc-windows-msvc/wry/trait.WebViewExtWindows.html)
- [winit documentation](https://docs.rs/winit/0.30.13/winit/)
- [WebView2 documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/)

MIT licensed; see LICENSE. Dependencies retain their respective licenses.
