# Rust Browser: Codex Engineering Instructions

## 1. Project Overview

Rust Browser is a custom desktop web browser built using Rust.

**Long-term objective:** Develop a secure, fast, lightweight, customizable browser capable of eventually replacing Firefox or Google Chrome for everyday use.

The project began with Wry/WebView2 for web rendering. Servo is a potential future rendering engine.

Always inspect the actual repository before making assumptions about the current architecture.

### Core Principles

- Reliability over feature quantity.
- Security and privacy by design.
- Maintainable, idiomatic Rust.
- Modular architecture.
- Minimal unnecessary dependencies.
- Performance-conscious development.
- Incremental releases.
- Continuous testing and integration.
- Preserve working functionality.

## 2. Repository Rules

### Branching Strategy

- `main` is the stable branch.
- Never develop new features directly on `main`.
- Create feature branches from the latest verified `main`.
- Submit changes through pull requests.
- Never merge pull requests without my explicit approval.
- Never force-push or rewrite published history without authorization.

Branch naming conventions:

- `feat/feature-name`
- `fix/issue-name`
- `refactor/component-name`
- `docs/topic-name`
- `test/component-name`
- `ci/workflow-name`

### Commit Standards

Follow Conventional Commits.

Examples:

- `feat(tabs): implement tab management`
- `fix(navigation): resolve back button issue`
- `refactor(engine): separate rendering interface`
- `test(session): add session restoration tests`
- `docs: update installation instructions`
- `ci: add Windows build validation`
- `perf(browser): reduce unnecessary webview creation`

Keep commits focused and descriptive.

Leave AI attribution off commits, pull request descriptions and GitHub comments: no `Co-Authored-By` trailers for an agent and no "Generated with ..." lines, whichever agent did the work.

Do not commit:

- Secrets or API keys
- Browser profiles or personal browsing data
- Build artifacts
- Temporary files
- Local environment configuration
- Debug logs containing sensitive data

## 3. Architecture Standards

Maintain clear separation between:

1. Application lifecycle and state
2. Browser tabs and navigation
3. Rendering engine integration
4. User interface
5. Settings and preferences
6. Persistent storage
7. Security and permissions

### Rendering Engine

- Preserve the existing working rendering engine unless a milestone explicitly requires a change.
- Avoid unnecessary coupling between browser logic and WebView2.
- Design interfaces that could support Servo in the future.
- Do not introduce Servo or rewrite the rendering engine without approval.

Avoid premature abstractions and unnecessary complexity.

## 4. Rust Development Standards

Write clean, idiomatic, maintainable Rust.

Requirements:

- Follow Rust naming conventions.
- Prefer strong types and explicit error handling.
- Use `Result` and `Option` appropriately.
- Avoid unnecessary cloning and allocations.
- Avoid `unwrap()` and `expect()` in recoverable production paths.
- Minimize shared mutable state.
- Keep functions focused.
- Document complex or non-obvious logic.
- Minimize `unsafe` Rust.
- Explain and document any necessary unsafe code.

Use `cargo fmt` and Clippy consistently.

Avoid introducing new dependencies when existing libraries or standard-library functionality are sufficient.

## 5. Testing Requirements

Every significant feature or bug fix should include appropriate tests.

Prioritize:

- Unit tests for business logic
- Integration tests for interacting components
- Regression tests for resolved bugs
- Edge-case and error-handling tests
- Manual GUI verification where automation is impractical

Before submitting a pull request, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build
```

Adapt these commands to the actual workspace and supported features if necessary. Explain any deviations.

Never delete tests or suppress genuine errors simply to make checks pass.

Clearly distinguish tests that passed from tests that could not be run.

In native `winit`/WebView2 validation fixtures:

- Wait for observable browser/DOM state rather than fixed sleep delays.
- When calling `el.exit()` while a session is open, also set `ControlFlow::Poll` so `App::about_to_wait`'s `ControlFlow::Wait` does not block loop shutdown.
- Preserve the first recorded fixture error (`get_or_insert`) rather than overwriting it on later ticks.

## 6. CI/CD Standards

GitHub Actions is the project's CI/CD platform.

Pull requests should be validated through automated checks appropriate to the project.

Required checks:

- Rust formatting
- Clippy linting
- Automated tests
- Windows build verification

Where practical, also include:

- Dependency vulnerability checks
- Dependency caching
- Build artifact generation
- Release packaging

### CI/CD Rules

- Keep workflow permissions minimal.
- Use trusted, appropriately pinned GitHub Actions.
- Do not expose secrets in logs.
- Do not bypass failing checks.
- Do not claim CI is passing without verifying its status.
- Never publish a release automatically from an unreviewed feature branch.

Releases should use semantic versioning, such as `v0.2.0`.

## 7. Security and Privacy

Browser security is a fundamental project requirement.

Always:

- Treat web content as untrusted.
- Validate external inputs.
- Keep website content isolated from privileged native functionality.
- Restrict access to native APIs.
- Use HTTPS where appropriate.
- Handle permissions explicitly.
- Protect stored user data.
- Avoid recording sensitive information in logs.
- Store application data in appropriate user-specific directories.
- Avoid weakening browser security to fix compatibility problems.

Do not introduce insecure workarounds without explaining the risks and obtaining approval.

## 8. Performance Standards

Aim for efficient memory use, responsive navigation, and minimal unnecessary background activity.

Avoid:

- Unnecessary webview recreation
- Blocking operations on the UI thread
- Excessive disk writes
- Unbounded memory growth
- Unnecessary background tasks

Measure performance before claiming an optimization is successful.

## 9. Documentation Standards

Keep project documentation synchronized with implementation.

Update the README when changes affect:

- Installation
- Building and running
- Supported platforms
- Keyboard shortcuts
- User-facing functionality
- Configuration
- Known limitations

Maintain a changelog documenting meaningful changes.

Keep `PRODUCT.md` (product context) and `DESIGN.md` (chrome design system derived from shipped UI) aligned with the actual implementation.

Store all text files as UTF-8 without re-encoding, and never use em dashes in repository text files (`tests/source_text.rs` enforces both; use `" - "` in window titles).

Document important architectural decisions when needed.

Do not document planned functionality as though it already exists.

## 10. Development Workflow

For every new milestone, follow this sequence.

### Phase 1: Inspect

- Review the current repository.
- Check Git status and recent commits.
- Inspect relevant source files.
- Review CI/CD status.
- Identify dependencies and constraints.

### Phase 2: Plan

- Define the feature requirements.
- Identify affected components.
- Assess implementation risks.
- Define acceptance criteria.
- Identify necessary tests.

### Phase 3: Implement

- Create the appropriate feature branch.
- Make incremental changes.
- Follow existing conventions.
- Keep changes within the milestone's scope.
- Preserve existing functionality.

### Phase 4: Validate

- Run formatting checks.
- Run Clippy.
- Run automated tests.
- Build the application.
- Perform supported manual verification.
- Investigate regressions and failures.

### Phase 5: Deliver

- Update documentation.
- Create focused Conventional Commits.
- Push the feature branch.
- Open a pull request against `main`.
- Review CI results.
- Report outstanding issues.

Do not merge without approval.

## 11. Codex Operating Rules

When working on this repository:

1. Inspect existing implementations before modifying them.
2. Reuse working components rather than unnecessarily rewriting them.
3. Follow the current milestone specification.
4. Make reasonable implementation decisions autonomously.
5. Never perform destructive Git operations without explicit approval.
6. Never modify unrelated features without a clear justification.
7. Never claim functionality works without appropriate verification.
8. Report blockers and limitations accurately.
9. Prefer small, reviewable changes over large rewrites.
10. Keep `main` stable at all times.
11. Re-read `AGENTS.md` on every prompt and update `AGENTS.md` after each prompt whenever project state, milestones, workflows, or engineering rules change.

Do not ask for approval for ordinary, reversible implementation decisions already covered by the milestone.

Request approval for destructive operations, major architectural changes, and changes to established project requirements.

### Cross-Agent Continuity (Antigravity and Claude Code)

Development alternates between Antigravity and Claude Code on the same working tree:

- **Single instruction source:** Antigravity loads `AGENTS.md` directly; Claude Code loads `CLAUDE.md`, which imports `@AGENTS.md`. Put all repository rules and state updates in `AGENTS.md` so both agents stay synchronized.
- **Session handoff check:** At the start of a session after switching agents, inspect `git status`, `git branch -vv`, `gh pr list`, `AGENTS.md`, and `DEVELOPMENT_CHECKPOINT.md` (fetching `origin` when needed) rather than assuming local branches match remote PR heads.
- **Shared skills and artifacts:** UI design and audit skills (`impeccable` with its five `impeccable-*` subagents, and `hallmark`) are installed in `~/.claude/` for Claude Code and in `~/.gemini/config/plugins/rust-browser-skills/` for Antigravity. Both read and update the same repository files (`PRODUCT.md`, `DESIGN.md`, `.impeccable/`, and `docs/hallmark/`) and share `%USERPROFILE%\.impeccable`. Keep agent-local state (`.claude/`, `.impeccable/config.local.json`, `.impeccable/hook.*.json`) untracked.

## 12. Definition of Done

A feature is complete only when:

- [ ] Requirements are implemented.
- [ ] Existing functionality is preserved.
- [ ] Relevant tests are added or updated.
- [ ] Automated checks pass.
- [ ] Supported build validation passes.
- [ ] Documentation is updated.
- [ ] Changes are committed.
- [ ] Feature branch is pushed.
- [ ] Pull request is opened.
- [ ] CI status is reported.
- [ ] Known limitations are documented.

Do not mark unverified requirements as complete.

## 13. Reporting Format

At the end of each development task, report:

**Changes**
- Features implemented
- Files/components modified
- Architectural decisions

**Verification**
- Tests run and results
- Build status
- CI/CD status
- Manual testing status

**Git**
- Branch
- Commit hashes
- Pull request URL

**Remaining Work**
- Bugs
- Limitations
- Recommended next steps

## 14. Long-Term Project Direction

The project aims to evolve through incremental releases:

- v0.1: Basic browser functionality
- v0.2: Tabs, keyboard shortcuts, search, session restoration
- v0.3: Bookmarks, browsing history, downloads
- v0.4: Privacy, permissions, profiles
- v0.5: Performance and interface improvements
- v0.6+: Potential Servo experimentation
- v0.18: Multiple normal windows (done)
- v0.19: Saved passwords, form autofill and bookmark import (done)
- v0.20: Ad blocking and a new layout (done; the release prepared for it was not published)
- v0.21: Reliability and chrome polish pass (toolbar polish, shield counts, vertical tabs, library sidebar, new-tab tiles, bookmarks bar, dropdown menus, permission prompt overlay, unified Settings panel, menu groups, notice levels, library/confirmations PR #45, chrome polish PR #46, cross-agent rules PR #49, Windows x64 installer workflow PR #37, `/impeccable polish` chrome refinement PR #50, and combined `/impeccable` harden/audit/layout/clarify/polish pass PR #52, persistent bookmark site icons with merged tab strip and flush window caption controls PR #53, and unbolded selected tab title with the live page behind dropdowns PR #54 merged on `main`)
- v0.21 profiles: separate profiles (process per profile, `--profile=<id>`, profile button, Settings > Profiles, launch picker) PR #55, product record PR #56 and chrome critique follow-up PR #57 merged on `main`
- v0.21.0: **published 2026-11-19**, the first release (qualified in PR #58, tag `v0.21.0` on `c7c9e15`, tag run 44447568537, publish run 44452459369). Its metadata states library schema 3 instead of 5 (accepted; fixed for the next release by PR #59)
- After v0.21.0: second chrome critique follow-up PR #61 merged; third chrome critique follow-up (equal-width tab strip, quiet lock for encrypted pages, shield only once a blocking extension is added, Menu New group, profile button only with more than one profile) PR #63 squash-merged on `main` as `e92daae`
- Fourth chrome critique follow-up (badge keeps its shape while loading, middle-click and a tab menu with Duplicate, Pin, Move, Close others and Reopen, Menu routes to Profiles and Extensions, plain outline shield) PR #65 squash-merged on `main` as `4d365f1`
- Bookmark menus (right-click a bookmarks-bar bookmark or folder, or a row in a folder dropdown: open in a new tab, window or private window, open all in tabs, edit, rename, delete, copy link, add bookmark or folder, manage) PR #67 squash-merged on `main` as `a454e41`
- Resize from every edge (Windows' invisible borders on the left, right and bottom of a restored window via a `WM_NCCALCSIZE` subclass; the top keeps the toolbar strip) PR #68 squash-merged on `main` as `2046726`
- Page-only fullscreen (HTML fullscreen hides the toolbar, tab column and library; `Full::Bare` layout; a 4-second "<host> is now full screen" notice drawn by the clipped toolbar view; `native_window::set_fullscreen_frame` makes a maximized window cover the whole monitor, which winit otherwise trims to the work area) and the blocker's own popup under the shield (`native_extension_popup`: the extension's `default_popup` page in a profile view, with a `chrome.tabs.query` shim so it acts on the current page; right-click keeps the details dropdown) on `feat/fullscreen-and-blocker-panel`. Follow-up: extension pages such as a blocker's dashboard (`openOptionsPage`, `chrome-extension://` tabs) do not open yet
- v0.22.0 release candidate (owner request 2026-11-22): `docs/release-v0.22.0` from `main` at `c062982`, version bump, release notes, vertical-tab row fix, installer upgrade baseline moved to the published v0.21.0 (`c7c9e15`); owner checklist items 39 to 42 added. Not tagged or published
- v0.22.0 tag (`631a555`, run 45231208255) stopped at `cargo audit` by RUSTSEC-2026-0594 (`cookie` 0.18.2, published 2026-11-23); not published. The tag stays. `claude/new-session-90a06p` bumps `cookie` to 0.18.3 and the version to 0.22.1
- v0.22.1 tag (`cdf2bb6`): tag run 45236099610 passed; the owner's publish run 45239524988 stopped when `native_newtab_tiles_and_background` failed once (attempt 1; attempt 2 cancelled); not published. The tag stays. PR #73 (`test/native-flake-waits`) makes that test and `native_profiles_switch_add_edit_and_delete` wait for state; the tile failure was not reproduced locally (20 runs). `docs/release-v0.22.2` stacks on it: version 0.22.2, release notes renamed to `v0.22.2.md`. PR #74 merged as `a57caea`; its CI failed on the bookmarks-bar test's settle deadline, fixed by PR #75 (`b4536e7`)
- v0.22.2: **published 2026-11-23** (tag on `b4536e7`, tag run 45269553809, publish run 45274478007; the first publish run 45272247238 failed once in `native_launch_after_a_crash_takes_over_the_profile`, not reproduced locally). Follow-up: find out why that test's saved session once lacked the launched link after a crash takeover
- Linux parity on `feat/linux-basic` (on `main` at `bebb22d`, v0.22.2; published 2026-11-24 as PR #77, squash-merged on `main` as `2ed7534`): WebKitGTK through wry with the same toolbar page and command protocol as Windows; owner decisions 2026-11-22: keep wry on both platforms (no CEF or Servo), drive the shield with WebKit content blockers on Linux (WebKitGTK cannot run extensions); owner decisions 2026-11-24: Linux gets every Windows-only part, saved passwords live in the desktop keyring (Secret Service through libsecret; this reverses the earlier decision to skip them), and the shield click on Linux is a native per-site switch (extension popups cannot run). Done: basic front end, shortcuts inside website views, toolbar IPC (trusted views load a private `rust-browser://` scheme; wry drops IPC from pages whose URL exceeds 64 KiB), the shared `src/layout.rs` table with tab and library columns, dropdowns over the live page (XFixes shape on wry's X11 container window), a find bar (Linux-only `find_text`/`close_find`), address-field clipboard, X input focus on the web view's own window, downloads (`src/linux_downloads.rs`: WebKit's own download signals, a GTK folder chooser per download, the shared `downloads::Staged` lease folders and no-overwrite publish, progress, cancel, stall check, Open folder through GIO; wry gives every view it builds a default handler that saves into ~/Downloads unasked, so each download's earlier `decide-destination` handlers are disconnected before the browser's own), permission prompts (`src/linux_permissions.rs`: WebKit's `permission-request` answered from remembered rules or the shared toolbar prompt; camera, microphone, location and notifications only, from HTTPS or loopback pages in the selected tab; WebKit has no user-gesture flag, so a turned-down request is denied quietly in that tab until it navigates; a camera request that also wants the microphone asks for each; revoking in Settings forgets the rule and closes the site's tabs). Camera and microphone are untested in the container (no devices). Clearing browsing data: the same Settings section and rules as Windows (every tab closes first, closed tabs are forgotten, Settings stays open), history and permissions through the library, cookies, site storage and caches through WebKit's website data manager (never the HSTS list), saved tabs through `Store::forget_session`; saved passwords and contact details through the keyring; a successful clear shows as information (Linux now has the `info` notice Windows has). Windows: normal windows share one process as on Windows (`Browser` holds `Vec<Win>` and dereferences to the window being handled; commands carry a `Source` window or tab; up to 8 windows, all saved with `other_windows`, restored with the last focused in front; Reopen closed window), private windows run in their own `--private` process (links after `--`). A second launch hands its links, or a request for a window, to the running browser through `src/linux_handoff.rs` (a 0600 Unix socket in `$XDG_RUNTIME_DIR` or a private `handoff` folder in the profile, the shared `handoff` format; SECURITY.md has its threat table). A private window's website views are related to one hidden anchor view, so its tabs share one in-memory WebKit context (wry otherwise gives each incognito view its own, which split sign-ins between tabs and left later tabs' downloads to wry's unasked ~/Downloads handler). Sleeping tabs: WebKitGTK cannot freeze a page, so a sleeping tab's view is closed and its WebKit session state kept; selecting it restores the back/forward list (scroll and form text came back in the container check) and reloads the page; the shared `background::Lifecycle` rules decide, plus audio and camera, microphone or screen capture; Settings shows a Linux sentence for this (`LINUX_TEXT` in `src/linux.rs`). Bookmark import: a GTK file chooser, then the shared `bookmark_import::read_file` and parser off the UI thread and the library worker's import. Profiles: as on Windows, each profile runs in its own process and folder (`profiles::dir`, its own lock and link socket); `--profile=<id>`; the start decision (`profiles::choose_start`), the arguments that start a profile (`launch::profile_args`) and the picker's page and choices (`src/picker.rs`) are shared with Windows; `src/linux_picker.rs` shows "Who's browsing?" on a plain launch with more than one profile (its own `rust-browser-picker://` scheme in an in-memory context, then starts the chosen profile and exits); the profile button, Settings > Profiles (open, add, edit, delete) and the profile name in the title; a private window is of its profile; checked under Openbox in Xvfb (picker, two profiles side by side, hand-off, refused unknown profile, add and delete). Shield: Add from folder takes the same unpacked extension as on Windows (such as uBlock Origin Lite) but uses only its declarative blocking rules: `src/content_rules.rs` translates the rulesets it turns on by default into WebKit content-blocker JSON (block, and allow as `ignore-previous-rules`, written in rising priority; anything WebKit cannot express exactly is skipped, never widened; Chromium's default types become one rule per filter followed by a per-level top-document exemption, which cut uBO Lite's compiled filter from 169 MB to about 60 MB), and `src/linux_shield.rs` compiles it through WebKit's `UserContentFilterStore` (C API via `webkit2gtk::ffi`) into the profile's `content-filters` and adds it to every normal website view; the shared manifest checks moved to `src/extension_manifest.rs`; the extension ID is Chromium's path-derived one; Linux keeps on/off in `SavedExtension.disabled` (WebView2 keeps it on Windows); a kept filter that cannot load is prepared again from the folder; no count (WebKit reports none), so the count switch is hidden and the shield and Settings texts say what Linux does (`LINUX_TEXT`). Checked in the container with the real uBO Lite (an `ad.doubleclick.net` image loads without it, is blocked after it is turned on, after being prepared at start, and after a restart) and the test blocker (added through the GTK folder chooser, turned on live, removed). Xvfb GUI checks must click into the page before pressing keys: without focus, F5 never reached the window and looked like a filter failure. Window menu: a GTK menu at the pointer on right-clicking the caption space (Restore, Minimize, Maximize, Close; winit has none on X11). Diagnostics: the shared local record in normal windows (start, failed loads, stopped pages, session, library, download and clearing failures, tabs that could not wake, unclean exit), shown in Settings > System and from a problem notice; a page whose web process ends says so, and a trusted view whose process ends reloads its local document. Per-site shield switch: "Block requests on <site>" in the shield panel; paused hosts in `Settings.shield_paused_sites` (Linux only) get no content filters, decided again when each page starts loading. Passwords and form data: `src/linux_forms.rs` and `src/linux_forms.js` (a form helper in its own WebKit script world in the top frame; page scripts cannot see it; messages bounded and from the view's own origin; HTTPS or loopback only), `src/linux_keyring.rs` (libsecret 0.4, asynchronous, items per profile digest), a key button and password panel in the toolbar (offer to save: Save, Never for this site, Not now; fill a login or contact details only when chosen there), `Settings.password_never_sites` (Linux only); checked in the container with GNOME Keyring (save, `secret-tool` shows the item, fill after reload, contact details saved and filled, clearing removes them, a probe page cannot see the helper). Linux clippy now passes with `-D warnings` (no unused toolbar command), and the Linux CI job installs `libsecret-1-dev`. Default browser: Settings > System > Make default browser writes `rust-browser.desktop` (http and https only, `%U`, New window and New private window actions) to the user's applications folder and sets it as the default for web links through GIO (`src/linux_default.rs`); checked in the container (`mimeapps.list` and `gio mime` name it, and `gio open` handed a link to the running browser as a new tab). Every toolbar command now has a Linux handler (no catch-all arm). README has a "Linux (from source)" section and CHANGELOG an Unreleased entry. Next: owner review of PR #77; packaging (an icon and a system-wide install) does not exist yet. Build and test Linux in an Ubuntu 24.04 container with `libwebkit2gtk-4.1-dev`, `libxkbcommon-x11-0`, `x11-utils` and Xvfb (WSL has no sudo); GUI checks must also run under a window manager (Openbox in Xvfb), because Xvfb alone hid a startup crash (winit counted WebKit's `GLXBadWindow` errors as its own; fixed by an Xlib error hook); the owner can run the container's build on the Windows desktop through WSLg (`-v /run/desktop/mnt/host/wslg/.X11-unix:/tmp/.X11-unix -v /run/desktop/mnt/host/wslg:/mnt/wslg -e DISPLAY=:0`); the image also needs `libsecret-1-dev`, and GUI checks of passwords run `gnome-keyring-daemon --unlock --components=secrets` inside `dbus-run-session`
- Windows fixes from daily use (owner report 2026-11-26) on `fix/tab-close-and-home` from `main` at `2ed7534` (PR #78, squash-merged on `main` as `2e3a3b2`): a click on a tab's close icon (the SVG, not the button padding) took pointer capture for dragging, so the click selected the tab (fixed with `close.contains(e.target)`; `native_one_click_on_the_close_icon_closes_the_tab` clicks the icon through CDP mouse input); Home took the view's old address on the next page event while the blank load was still starting, so the blank view showed (fixed with `Page::leaving_for_home` and `adopted_address`; `native_tab_sleep_resume_and_local_page_security` now checks the new-tab page after Home). Both tests failed before the fixes
- Built-in ad blocker on `feat/built-in-blockers` from `main` at `2e3a3b2` (PR #79, squash-merged on `main` as `36bbcd9`; owner request 2026-11-26): Windows embeds the official uBlock Origin 1.77.2 Chromium ZIP unchanged (`third_party/ublock-origin`, SHA-256 pinned and tested; `src/bundled.rs` reads it with `miniz_oxide`, the one new dependency, and unpacks it into `<profile>/built-in/ublock-origin`), added on first start and recorded as `SavedExtension.built_in`; Linux downloads hBlock's domain list (`src/hblock.rs`, `src/linux_blocker.rs`, libsoup through `soup3`; never shipped because some hBlock sources are non-commercial) weekly into three WebKit filters. `Settings.built_in_blocker` is on by default for new and existing profiles; turning it off or removing it keeps it off. Owner decisions 2026-11-26: uBlock Origin (classic, MV2) on Windows and hBlock on Linux; Windows private windows run extensions (the built-in one and the user's, re-added each time), which required giving up WebView2 InPrivate for private website views: they are now an ordinary WebView2 profile in the window's `private-contexts/private-<id>` folder (on disk while open, removed on close or at the next start), checked by `verify_private_storage`; the toolbar stays InPrivate. Measured: WebView2 adds but never runs an extension in an InPrivate profile; WebKit compiles hBlock's 419,949 domains in about 15 s into about 130 MB. Native fixtures run the built-in blocker only when they set `TEST_BUILT_IN`. Tests: `native_built_in_ublock_origin_blocks_and_turns_off`, `native_private_windows_run_extensions_and_the_built_in_blocker` (replaces `native_private_windows_never_run_extensions`), Linux container check (blocked in normal and private windows)
- Bug-hunt fixes (owner request 2026-11-26) PR #81, squash-merged on `main` as `16da6cd` (its first CI run failed once in `native_private_windows_run_extensions_and_the_built_in_blocker` after its 20-second wait; passed on re-run and 10 of 10 locally): Linux content rules use `if-frame-url`/`unless-frame-url` for Chromium's initiator domains on subresources (WebKit's `if-domain` checks the top document) and keep `if-domain`/`unless-domain` for frame documents (WebKit's frame URL for a frame document is the frame itself, measured), so rules with initiators are no longer merged, and a named host must end at the real host's end (`HOST_END`, `HOST_REST` in `src/content_rules.rs`), so a user name such as `allowed.test@` cannot satisfy an allow rule; both checked in WebKitGTK 2.52 by the ignored `linux_shield::tests::webkit_applies_frame_and_host_conditions` (run under Xvfb in the container; it fails on `main`'s translator). `PrivateLease::sweep` skips leases younger than `SWEEP_GRACE` (60 s), closing the window between a lease's folder and its lock. `bundled::unpack` keeps a complete copy of another release that is in use instead of failing. Known, not fixed: a request-domain rule's URL filter cannot match at the first character of the path (pre-existing narrowing), and `||host*` filters still accept a user name
- Linux packages (owner request 2026-11-26: a `.deb` for Ubuntu/Debian and a PKGBUILD for Arch, which the owner also uses) on `feat/linux-packaging` from `main` at `16da6cd`: `[package.metadata.deb]` in `Cargo.toml` (cargo-deb 3.8.0; `$auto` plus `libxkbcommon-x11-0` and `libx11-xcb1`, which winit loads at run time), `packaging/linux/arch/PKGBUILD` (`rust-browser-git`, builds the latest main; `build-from-checkout.sh` builds the current commit), both installing `/usr/bin/rust-browser`, `packaging/linux/rust-browser.desktop` (equal to `linux_default::desktop_entry("/usr/bin/rust-browser")`, tested) and the Windows SVG icon in `hicolor`; Make default browser uses an installed launcher that runs the same program instead of writing a copy, and otherwise also writes the icon. CI: the Linux job builds the `.deb` and installs it in a clean `ubuntu:24.04` container; a new `arch` job (`archlinux:base-devel`, updated, not pinned) builds, tests, installs and checks the Arch package; `ci_scope.py result` judges it. Both run `packaging/linux/check-installed.sh`. Checked locally: the `.deb` in a clean Ubuntu 24.04 container, and the Arch package built (with its tests), installed and checked in `archlinux:base-devel`; the PKGBUILD needs `!lto` (makepkg's GCC LTO breaks linking the bundled SQLite). Not done: packages attached to releases (the release workflow is Windows-only), an AUR upload, Flatpak. The owner tested the Linux build in a GNOME session in the Hyper-V VM over GNOME Remote Login (RDP); YouTube first stayed blank (no GPU in the VM); after `WEBKIT_DISABLE_DMABUF_RENDERER=1` was suggested the owner reported it working (which setting fixed it was not recorded)
- Linux focus highlight (owner report 2026-11-26: no ring when clicking the address field on Linux) on `fix/linux-focus-highlight` from `main` at `4d1808c` (PR #83, squash-merged on `main` as `6064566`): wry hosts each view in a GTK window around a foreign X window, which GTK gets no focus events for, so no view was ever "active" and WebKit matched no `:focus`/`:focus-within` styles and hid the caret. `activate` in `src/linux.rs` sends the view's GTK window a synthetic GDK focus-in (and the previous one a focus-out) on every click into a view and every `focus`; the winit window's focus changes follow. Checked under Xvfb with Openbox (click, Ctrl+L, new-tab field) and by the ignored test `the_view_that_takes_the_keyboard_is_in_the_only_active_window`. Owner decisions the same day: address-field suggestions get both inline completion and a suggestion list; the new-tab page redesign gets top sites tiles, a clock and greeting, recent pages as cards and background images (separate PRs)
- Address suggestions (owner request 2026-11-26: inline completion and a list) on `feat/address-suggestions` from `main` at `4d1808c` (PR #84, squash-merged on `main` as `8a18bfa`): `src/suggestions.rs` ranks candidates (visits weighted by age, bookmarks +3, addresses starting with the text doubled) and picks the inline completion (a site by the summed weight of its pages, or a page once `/` or a scheme is typed); `Database::suggest` gathers the first word's matches (300 visited addresses, 100 bookmarks); `Action::Suggest` (serde-skipped) runs on the library worker and answers in `Reply::suggestions` for the asking window (`revision` 0, so pending library replies are not dropped); toolbar `suggest` command, `ui::suggestions_script`, `#suggest-panel` listbox under `#location` reusing the dropdown machinery (backdrop kept clear). On Windows, opening a dropdown moves WebView2 focus to the toolbar, which blurs the whole document for a moment: the list closes only when focus moves elsewhere in the toolbar (`document.hasFocus()`). Also fixes Linux visits recorded with an empty or previous title (WebKit's title at load finish, and a title arriving within 20 s updates the visit). Tests: 11 ranking unit tests, `address_suggestions_come_from_history_and_bookmarks` (SQLite), native `native_address_suggestions_complete_and_list` (CI and release lists); Linux checked under Xvfb with local `*.test` sites
- New-tab redesign (owner request 2026-11-26; owner choices: "Centred clock" layout, "Drawn + your own" wallpapers, DESIGN.md "Merge") on `feat/newtab-redesign` from `main` at `4d1808c` (PR #85, squash-merged on `main` as `2b726d2`), built with the `impeccable` skill (code-led, finish review "fix" then six of eight fixes resolved, the card truncation it found fixed after): clock with greeting and date, the search pill with an in-field button, site tiles (favorites then `Data::top_sites`, the 10 most visited sites of 90 days, with bookmark icons), three recent cards on a 640px measure, drawn wallpapers Aurora, Dunes, Horizon, Contours (inline SVG, dark and light renditions, about 23 KB) and Your picture (`src/wallpaper.rs`: PNG, JPEG or WebP up to 6 MB, checked by its bytes, copied to `<profile>/newtab-image`; `ChooseWallpaper` and `RequestWallpaper` commands on both platforms; Windows `native_downloads::choose_picture`, Linux GTK chooser). The new-tab page asks the library to refresh when it shows. Tests: `wallpaper` unit tests, `top_sites_rank_recent_visits_by_site_with_front_page_names_and_icons`, the toolbar policy test now admits only self-contained SVG and checked picture URLs; screenshot tool `newtab_screenshots`. Not captured: the Customize panel open, private window, hover/focus, high contrast, narrow window
- Address draft fix (`main` CI on `2b726d2` failed in `native_tab_sleep_resume_and_local_page_security`: "Background render overwrote address draft") on `fix/address-draft-blur` from `main` at `2b726d2`: since PR #84, typing opens the suggestion list, and opening a dropdown on Windows blurs the whole toolbar document for a moment; the address field's `onblur` ended editing on that blur, so a render before focus returned (a title, a notice) put the page's address over the draft. `onblur` now keeps editing while the suggestion list is open and the document has no focus, and `hideDropdown` ends editing when the suggestion list closes without the field holding the keyboard (a press on the page). Tests: the sleep test waits for the list and records `editing` at each blur (fails 3 of 3 without the fix, passes with it); `native_address_suggestions_complete_and_list` also checks that a press on the page, after `MENU_FOCUS_GRACE`, closes the list and shows the page's address. Also seen on 2026-11-27 and not reproduced locally: `native_shield_panel_fits_a_narrow_window` once found no extension added after 20 s (6 of 6 locally), and the owner-build launch in the release job (`tests/owner_build.ps1:58`, no window and saved session in 20 s) failed 3 times across PRs #84 and #85 and passed on re-run (startup about 1 s locally). PR #86 squash-merged on `main` as `6c4ddcc` (CI 46037780516 green)
- v0.23.0 release candidate (owner request 2026-11-27: release candidates for Windows, Linux and Arch once `main` was green) on `docs/release-v0.23.0` from `main` at `6c4ddcc`: version 0.23.0, `release-notes/v0.23.0.md`; `.github/workflows/release.yml` (renamed Release) gains `linux` (ubuntu-24.04, uncached: Clippy, tests, `.deb` installed in a clean `ubuntu:24.04` and checked) and `arch` (`archlinux:base-devel`: `build-from-checkout.sh`, `pacman -U`, `check-installed.sh`) jobs, each running `packaging/linux/check-release-tag.sh` (the Windows job's tag rules in POSIX sh) and uploading `rust-browser-v<version>-linux-amd64.deb` or `rust-browser-v<version>-arch-x86_64.pkg.tar.zst` with a `sha256sum` file; `publish` needs all three jobs and checks exactly nine files and four checksums before `gh release create`; `validation/test_release_workflow.py` covers the new jobs. The Arch package keeps the name `rust-browser-git` (from the PKGBUILD). The installer upgrade baseline moved from v0.21.0 (`c7c9e15`) to the published v0.22.2 (`b4536e7`), with a new CI cache key. Owner checklist items 43 to 47 added. PR #87 squash-merged on `main` as `aaa7786`
- v0.23.0: **published 2026-11-27** (tag on `aaa7786`, tag run 46043510948, publish run 46045961398; the first release with Linux packages, nine files; the owner published without recording checklist items 43 to 47). The release `.deb` needs `libc6 (>= 2.39)` (built on Ubuntu 24.04), so it does not install on Debian 12 or Ubuntu 22.04; the README says so (PR #88, `docs/deb-glibc-note`). Follow-ups: the owner checklist on the published build; building the `.deb` on an older base would widen it
- Owner-build launch check (owner request 2026-11-27, after it failed for the fourth time, on `main` at `05875dd`) on `test/owner-launch-diagnostics` from `main` at `2857e2b`: `tests/owner_build.ps1` waited 20 s for the launched browser's window and `session.json` and said only "No owner window and persisted disposable session". The budget is now 60 s (`$startupSeconds`, also passed to `Assert-OwnerProcessImage`, whose default stays 20 s for `tests/owner_process.ps1`); every launch logs when the window appeared and the session was saved; a failure names the launch (portable or installed) and reports the window, `session.json`, whether the process exited (with its exit code), the profile's top-level entries and `local-diagnostics/journal.json` (codes and times only). The installed launch reuses the portable launch's profile, so its `session.json` already exists: a failure there means the window did not appear. Not reproduced locally (window 0.1 s, session 1.4 s); the PR's CI logged the portable launch at window 0 s, session 1.6 s and the installed at window 0.1 s, session 0 s. The same PR's first CI run failed in `native_imported_bookmarks_show_on_the_bookmarks_bar` ("invalid type: null, expected a string"): the native fixtures' script answers share `Probe.result`, and a click's late `null` answer overwrote the folder report asked for after it. `answer_slot`/`forget_answer` in `src/windows_validation.rs` now number the scripts asked for, so only the latest one's answer fills `result` (all eight sites)
- Then: daily use by the owner
- First tagged release: when the owner decides the browser is ready; not tied to a version
- v1.0: Daily-use release candidate

These milestones are planning targets, not guarantees of completed functionality.

Future releases may revise the roadmap based on technical findings.

**Always prioritize stability, maintainability, and security over release speed.**