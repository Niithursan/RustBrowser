# Owner checklist (about 20 minutes)

Checks only a person can do. Automated tests cover the rest (see
[VALIDATION.md](../VALIDATION.md)). Do them on the build under review, note
**pass** or **fail** for each, and report back with anything you noticed. A fail
needs only one sentence about what you saw.

Use a normal window with a **fresh disposable test profile**, not your daily profile.
Browsing, searches, sign-in, sessions and permission choices can all save test data.
Keep the test profile after a failure so the result can be investigated; do not post
its contents or credentials in an issue.

Before starting, obtain a validation-identity installer and an isolated launch command
for the exact build under review. The ordinary installer in the CI review package
uses the production identity; do not run it over your real installation for this
checklist. Do not enable Windows Sandbox, create a VM or change Windows features
as part of these checks. Those require a separate owner decision.

Use the bundle prepared by `validation/prepare-owner-build.ps1`, following its
`OWNER_BUILD.md`. Check `owner.json` against the reviewed PR's commit and SHA-256.
Run `.\Launch.ps1 -VerifyOnly`, then `.\Launch.ps1` for the portable copy, or
`.\Launch.ps1 -Installed` after installing that bundle's validation-only Setup.
Use the same launcher for every restart. It checks the executable hash, installation
identity when applicable and explicit disposable profile. The bundle must remain at
its prepared location because Setup's destination is fixed. Technical acceptance
does not authorize declaring, tagging or publishing an RC.

## 1. Installer wizard (4 minutes)

Only use a **validation-identity installer** whose directory and shortcuts are
distinct from your real installation. Confirm that the launch uses the disposable
profile; the production profile is `%LOCALAPPDATA%\RustBrowser\v0.1` and must stay
untouched during this milestone.

1. Run the validation-identity Setup supplied for this build.
2. Read each page: licence, notice, the optional desktop shortcut task, ready,
   finish. There is no folder page; the folder is fixed per user.
3. Tick **Create a desktop shortcut** (it is off by default) and finish. Setup does
   not start the browser. Inspect the Start Menu and desktop shortcut icons without
   opening them. Start the installed executable only through the supplied
   isolated-profile launch command. The validation installer changes installation
   identity and destination, not the application's default profile path.

**Pass:** every page is readable and its buttons work; Windows warns about an
unknown publisher (expected, the build is unsigned) and nothing else; the browser
opens with the local new-tab page; the validation desktop shortcut and Start Menu entry show the
Rust Browser icon.

## 2. Taskbar and window identity (1 minute)

1. With the browser open, look at the taskbar button, the window title and Alt+Tab.

**Pass:** the Rust Browser icon appears in all three, and the title shows the
version.

## 3. Narrator over the toolbar (4 minutes)

1. Press **Ctrl+Windows+Enter** to start Narrator.
2. Click into the page, then press **Ctrl+L**. Narrator should say the address field.
3. Press **Tab** and **Shift+Tab** through the toolbar.
4. Press **Ctrl+Windows+Enter** to stop Narrator.

**Pass:** each control is announced with a name that says what it does (for
example "Back", "Reload", "Website address or search", "Default search engine");
the active tab is announced as selected; nothing is announced as an unlabelled
"button".

## 4. Contrast theme (2 minutes)

1. Open **Settings > Accessibility > Contrast themes**, pick **Night sky**, apply.
2. Look at the toolbar, tabs, address bar and the More (...) menu.
3. Set the contrast theme back to **None**.

**Pass:** all text and every control border is visible; the active tab is clearly
marked; the focus outline is visible when you press Tab.

## 5. IME text entry (2 minutes)

Needs any input method other than plain English, for example Japanese or Chinese
(Pinyin). Skip and say so if you have none installed.

1. Switch to the input method (Windows+Space), click the address bar.
2. Type a word that needs composition (for example `nihon` then Space for 日本).
3. Press Enter to confirm the composition, then Enter again to search.

**Pass:** the composition window appears next to the address bar, the first Enter
only confirms the text and does not search, the second Enter searches for the
composed word.

## 6. One real sign-in (3 minutes)

Use a site and account you control, preferably a test account, in the disposable
profile. Enter credentials yourself and do not include them in the report.

1. Sign in normally.
2. Close the browser window, then start it again through the same isolated-profile
   launch command.

**Pass:** the sign-in page works (including any second factor), you are still
signed in after the restart, and the site looks as it does in your usual browser.

## 7. One video call page (2 minutes)

1. Open a video-call page you control, for example <https://meet.jit.si/>, and create
   a fresh private test room. Do not join an existing room or invite anyone.
2. When the browser asks for camera and microphone, choose **Allow** once.
3. Leave the meeting and close the tab.

**Pass:** the prompt names the site you chose; your camera preview appears after you
allow it; the camera light goes off within a few seconds of closing the tab.

## 8. Two monitors with different scale (2 minutes)

Needs two available monitors with different display scales. If that setup is not
available, mark this item not tested. Record each original scale before changing it.

1. **Settings > System > Display**, select one display, set **Scale** to 150%.
2. Drag the browser window from one display to the other and back.
3. Set the scale back to its previous value (normally 100%).

**Pass:** on each display the toolbar is sharp (not blurry), correctly sized, and
the page area starts directly below it with no gap or overlap.

## Reporting

Report items 1 to 8 as pass, fail or not tested, with the exact version/commit.
For a failure, include one sentence about what happened. Share only screenshots
without credentials, private messages, meeting links or other sensitive data.
Leave unavailable IME/monitor/device checks as not tested. These owner checks have
not been completed merely because the automated suites passed.
