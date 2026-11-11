# Owner checklist (about 55 minutes)

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

## 9. Find in a website (2 minutes)

Use a public documentation page or controlled fixture without sensitive text.
Press Ctrl+F, enter a word present several times, and use the native find arrows
or Enter to move between matches. Close its bar with Escape. Repeat using
**More > Find in page** while the toolbar has focus.

**Pass:** the native find bar appears over the website, highlights the expected
matches, accepts text and closes. Browser controls are not searched or changed.

## 10. Zoom (2 minutes)

1. On the website press Ctrl+plus, Ctrl+minus and Ctrl+0. Try Ctrl+wheel with the
   pointer over content. Use **More > Zoom + / Zoom - / Reset zoom (100%)** too.
2. Away from 100%, use the toolbar percentage/reset button. Open a second tab
   to the same website and switch between them.

**Pass:** page size and the percentage agree; the reset returns to 100% and hides
the percentage row. The toolbar's own text size stays unchanged. Each open tab
keeps its controller's zoom, and a new tab starts at 100%. Zoom is not restored
from Rust Browser's session after closing/recreating a controller.

## 11. Print preview and cancellation (2 minutes)

On the same nonsensitive page press Ctrl+P, inspect preview, then cancel with
Escape. Repeat using **More > Print**, starting with toolbar focus. Inspect the
selected output without changing system printer settings. No paper print or PDF
save is required by this checklist; report actual output separately if you test it.

**Pass:** preview contains the selected website, no browser controls or profile
contents, and Escape/Cancel returns to the usable page. Opening preview does not
silently print, save output, execute a file or request elevation. Output explicitly
created by a person can remain on disk, including from private browsing.

## 12. Make the validation build your default browser (3 minutes)

Use the bundle's validation-only Setup (`Launch.ps1 -Installed` flow). Its links
open the bundle's own disposable profile, never your daily one.

1. Start it with `.\Launch.ps1 -Installed`, open More (⋯) → Privacy & security,
   and select **Make default browser**.
2. In the Windows Settings page that opens, choose the entry named
   "Rust Browser validation ..." for HTTP and HTTPS.
3. Afterwards, set your usual browser back the same way in Settings → Apps →
   Default apps.

**Pass:** the button says Windows asks you to choose and opens Default apps on the
validation entry's page (or the general page on older Windows 11 builds); Rust
Browser changed nothing by itself; your usual browser can be chosen back.

## 13. Links from two other apps (3 minutes)

While the validation build is the default:

1. Click an https link in a chat app (for example Teams or Discord).
2. Click an https link in a mail app or a PDF reader.

**Pass:** each link opens as a new tab in the running validation window, which comes
to the front; no second window appears; no "already running" message.

## 14. Launch it twice (1 minute)

1. With the window open, run `.\Launch.ps1 -Installed` again.

**Pass:** no error; a second browser window opens on the new-tab page and comes to
the front. (Before 0.18 this opened a new tab instead.)

## 15. Second window (1 minute)

1. Press Ctrl+N, then use More (⋯) → New window.

**Pass:** each opens one new window on the new-tab page, with its own taskbar and
Alt+Tab entry. Close the extra windows with their close buttons; the browser stays
open until the last one closes.

## 16. Right-click paste in the address field (1 minute)

1. Copy two lines of text from Notepad.
2. Right-click the address field and choose Paste. Then try Cut, Copy and Select all.

**Pass:** the menu shows exactly Cut, Copy, Paste and Select all; the pasted text is
on one line; right-clicking elsewhere in the toolbar shows no menu.

## 17. Three windows with tabs (3 minutes)

1. Open two more windows with Ctrl+N, so three are open.
2. In each window open two or three websites in separate tabs. Use Ctrl+Tab,
   Ctrl+T and Ctrl+W in each.

**Pass:** tabs, shortcuts, the address field and notices act only in the window you
are using; each window's title and taskbar entry show its own page.

## 18. Close and reopen the browser (2 minutes)

1. With the three windows from step 17 open, use More (⋯) → Exit.
2. Start the browser again with the same launcher.

**Pass:** all three windows return with their tabs, pinned tabs and selected tabs;
the window you used last is in front.

## 19. A link lands in the last-used window (2 minutes)

1. With two or more windows open, click one window to use it, then click a link in
   another app (as in step 13).

**Pass:** the link opens as a new tab in the window you used last, and that window
comes to the front.

## 20. Drag windows between monitors (2 minutes)

Needs two monitors; otherwise report not tested.

1. Drag one window to the other monitor and back, and leave another window on each
   monitor.

**Pass:** each window redraws sharply at each monitor's scale, its toolbar fits and
its page stays usable.

## 21. Narrator on a second window's toolbar (3 minutes)

1. Open a second window, start Narrator (Ctrl+Windows+Enter) and move through the
   second window's toolbar with Tab and Shift+Tab.

**Pass:** Narrator reads that window's buttons, tabs and address field, not the
first window's. Turn Narrator off afterwards.

Steps 22 to 27 use the bundle's disposable owner profile only. For step 22 use a
throwaway test account you created for this check, never a real account or a
password you use anywhere else.

## 22. Save a password (4 minutes)

1. More (⋯) → Privacy & security → turn on **Offer to save passwords**.
2. Sign in to your throwaway test account on its website and accept WebView2's
   offer to save the password.
3. Sign out, then go back to the sign-in page.

**Pass:** the save offer appears after signing in, and on the sign-in page the saved
user name and password are offered.

## 23. A private window never offers to save (2 minutes)

1. Open a private window (Ctrl+Shift+N) and sign in to the same test account there.

**Pass:** no offer to save the password appears, the saved password is not offered,
and the two switches are greyed out in that window's Privacy & security.

## 24. Fill in a form (3 minutes)

1. Turn on **Fill in forms automatically**.
2. On a test form such as https://httpbin.org/forms/post, type a made-up name and
   address and **submit** the form (autofill saves entries when a form is submitted).
   Never enter real details.
3. Open the same form again and click the name field.

**Pass:** the earlier entry is suggested and fills the form. With the switch off
again, nothing is suggested.

## 25. Clear saved passwords (2 minutes)

1. Privacy & security → Clear browsing data → check only **Saved passwords** →
   Clear selected data.
2. Go back to the test account's sign-in page.

**Pass:** the saved password is no longer offered. Bookmarks and history are still
there.

## 26. Import Firefox bookmarks (4 minutes)

1. In Firefox: Bookmarks → Manage bookmarks → Import and Backup → Export Bookmarks
   to HTML, and save the file.
2. In Rust Browser: More (⋯) → Bookmarks → **Import from file…** and choose that
   file.

**Pass:** a notice says how many were added, skipped and refused; the bookmarks and
their folders appear in the bookmark manager. Importing the same file again adds
nothing.

## 27. Remove the test data (1 minute)

1. Clear **Saved passwords** and **Autofill data** again, and delete the throwaway
   test account if the site allows it.

**Pass:** nothing from steps 22 to 26 remains outside the disposable profile.

When you finish, set your usual default browser back (step 12.3) and uninstall the
validation entry from Apps & Features.

## Reporting

Report items 1 to 11 as pass, fail or not tested, with the exact version/commit
and executable SHA-256. The historical 2026-11-10 report covered only items 1 to 8
on bundle o-d8ee96f5, not the new items or a later candidate.
For a failure, include one sentence about what happened. Share only screenshots
without credentials, private messages, meeting links or other sensitive data.
Leave unavailable IME/monitor/device checks as not tested. These owner checks have
not been completed merely because the automated suites passed.
