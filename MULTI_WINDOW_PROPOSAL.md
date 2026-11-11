# Multiple normal windows: design and session-format proposal

**Status: implemented in v0.18** with option B and the two-step refactor, both
approved by the owner. The proposal below is kept as written for v0.17. What
v0.18 does differently from it:

- **Exit command.** More > Exit closes every window at once and keeps all of them
  for the next start. Closing windows one by one keeps only the last one closed,
  because closing a window removes it from the saved session.
- **Reopen closed window.** More > Reopen closed window brings back the most
  recently closed window's tab addresses and pins until the browser exits (up to
  8, in memory only). Titles, keep-awake choices and page history are not kept.
  Clearing browsing data empties the list and closes every other window first.
- **Focus.** A focus change only records which window is focused and which tab may
  ask for permissions; it does not move keyboard focus. Restored windows open
  behind the window that was focused last and do not take focus while they load.
- **Size limit.** If saving every window would exceed the existing 1 MiB session
  limit, the other windows are left out of that save rather than tabs of the
  focused window.

The design in step 1 is implemented as `Session` (profile services) holding a list
of `WindowState` (per-window state) in `src/app.rs`, rather than separate `Profile`
and `Window` types.

## What v0.17 does instead

- A second launch with no link (for example the Start Menu shortcut while the
  browser runs) is handed to the running window as a "new window" request. That
  window opens a new tab and comes to the foreground. It never opens a second
  window on the same profile.
- Ctrl+N and a "New window" menu item are not added, because they would have to
  do something other than what they say.
- Private windows are unchanged: each is its own process and profile.

## Evidence

**Session format 1 can restore exactly one window.** `storage::Snapshot` is
`{ version: 1, tabs, active, settings, pinned, titles, keep_awake }`: one tab
list, one active index, and per-tab arrays that `validate()` requires to match that
one list (at most 64 tabs). There is nowhere to put a second window's tabs or
which window was focused.

**Older versions ignore unknown fields.** `Snapshot` does not use
`deny_unknown_fields`, so 0.16 reads a file with an extra field and ignores it.
That makes a backward-compatible extension possible (option B below), but it is
still a change to what format 1 means, which owner decision 3 reserves.

**The application is built around one window.** `Session` owns the native window,
the toolbar view, the tab list and every profile service together. In `app.rs`
there are 83 direct uses of the single window, toolbar or tab list, and the native
fixtures have 152 more. Events are routed by `TabId` alone, each `Tabs` allocates
its own IDs, toolbar IPC commands carry no window, and the permission manager has a
single active tab. A second window is therefore a structural change, not a small
addition, and it could not be finished and fully validated in v0.17 without risk to
the twelve native suites that guard the current behaviour.

## Proposed design

1. **Split profile services from windows.** One `Profile` per process owns the
   profile lock, session store, settings, library worker, diagnostics, downloads,
   permission rules, the content `WebContext`, engine-restart and clearing state.
   Each `Window` owns its native window, toolbar view and retired toolbar, tab list,
   closed-tab list, panel, fullscreen state and notice.
2. **Route by window.** Tab IDs come from one process-wide allocator, so a `TabId`
   names one tab in one window. Each toolbar is built with its window's ID and every
   IPC command is delivered as `(WindowId, Command)`. Permission prompts record
   their window.
3. **Shared work waits for every window.** Engine restart rebuilds the views of all
   windows. Clearing browsing data starts only when no window has a retired view.
   Toolbar recovery stays per window. Downloads and history stay profile-wide.
4. **Closing.** Closing one window retires only its views; the process ends when
   the last normal window closes. The hand-off endpoint stays with the process.
5. **Ctrl+N and More > New window** open a window with the new-tab page; a
   "new window" hand-off does the same. At most 8 normal windows.

Delivery would be in two steps, each with all native suites green: first the
behaviour-neutral split (one window, same results), then the second window.

## Session format: options for approval

**A. Keep format 1, restore one window (no format change).** Save the most recently
focused window only. Before a window closes while others remain, nothing is lost
yet; when the browser exits with several windows, the user is told first that only
the focused window's tabs will be restored. Simple and downgrade-safe, but tabs of
other windows are not restored.

**B. Format 1 plus an optional field (recommended).** Keep every format 1 field for
the most recently focused window, so 0.16 and 0.17 restore that window exactly as
today. Add `"other_windows": [{ "tabs", "active", "pinned", "titles",
"keep_awake" }]`, each validated like the main window, at most 8 windows and 64 tabs
per window, written only when more than one window is open.
- Migration: none needed; files without the field are single-window.
- Downgrade to 0.16: it ignores `other_windows` and restores the main window;
  other windows' tabs are lost on the next save. The upgrade notes must say so.
- Version stays 1 because the meaning of existing fields does not change.

**C. Format 2.** `{ "version": 2, "windows": [...], "focused": n, "settings" }`.
Cleanest model, but 0.16 rejects version 2 as invalid, preserves the file and starts
with an empty session, so a downgrade loses every tab unless the file is converted
first. It would need a format-2-to-1 export and installer downgrade guidance.

**Decision needed:** choose A, B or C (B recommended), and approve the two-step
refactor.
