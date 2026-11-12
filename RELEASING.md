# Releasing Rust Browser

> **Prepared draft, not published.** The owner decided on 2026-11-12 that v0.20.0 will
> not be tagged or released; the first release is postponed until the owner says it is
> ready. This file is kept so it can be reused then.

Only the repository owner creates release tags and approves publishing. Automation
never creates a tag, and the release workflow publishes only when the owner runs it
by hand with **publish** turned on. These steps are for v0.20.0, the first published
release; later releases follow the same steps with their own version.

Builds are **not signed**. Windows SmartScreen shows a warning for them; that is
expected until a signing certificate exists (see
[packaging/windows/UPDATE_SECURITY.md](packaging/windows/UPDATE_SECURITY.md)).

## Before you start

1. The release pull request is merged into `main`, and CI and Dependency security
   passed on the merge commit. Check on GitHub: **Actions**, filter by branch `main`,
   and open the newest **CI** and **Dependency security** runs for that commit. Both
   must show a green check. Do not continue if either failed or is still running.
2. On `main`, `Cargo.toml` says `version = "0.20.0"` and
   `release-notes/v0.20.0.md` exists. The release workflow refuses a tag that does
   not match the Cargo version, and refuses to publish without that notes file.
3. Your owner checklist results for the release candidate build are recorded, including
   the clean-install check in a new Windows user account.

## 1. Create the tag on the merge commit

In a clean clone of the repository (no local changes):

```powershell
git fetch origin
git switch main
git pull --ff-only origin main
git log -1 --format="%H %s"      # must be the release pull request's merge commit
git tag -a v0.20.0 -m "Rust Browser v0.20.0"
git push origin v0.20.0
```

Use exactly `v0.20.0` (lower-case `v`, three numbers). The workflow refuses anything
else, a tag that is not on `main`, and a tag whose commit is not the one checked out.

## 2. Check the release workflow run

Pushing the tag starts **Windows release** automatically. This run builds and checks
everything but does not publish.

1. Open **Actions → Windows release** and select the run for `v0.20.0`.
2. Wait for the **package** job to finish with a green check. This workflow has not
   run before v0.20.0, and it builds without a cache, so expect it to take much longer
   than a CI run (its limit is 40 minutes). It runs a clean build, every regular and
   native test, three daily-driver runs and `cargo audit`, then builds and verifies the
   portable ZIP and Setup. If it hits the 40-minute limit, report the run ID; do not
   publish.
3. Open the run's **Summary**, then **Artifacts**, and download
   `windows-release-package`. Extract it in an empty folder. It must contain exactly
   five files:
   - `rust-browser-v0.20.0-windows-x64.zip`
   - `rust-browser-v0.20.0-windows-x64.zip.sha256`
   - `rust-browser-v0.20.0-windows-x64-setup.exe`
   - `rust-browser-v0.20.0-windows-x64-setup.exe.sha256`
   - `rust-browser-v0.20.0-windows-x64-release.json`
4. Check the checksums in PowerShell in that folder:

   ```powershell
   foreach ($name in 'rust-browser-v0.20.0-windows-x64.zip', 'rust-browser-v0.20.0-windows-x64-setup.exe') {
       $expected = (Get-Content "$name.sha256" -Raw).Trim()
       $actual = (Get-FileHash $name -Algorithm SHA256).Hash.ToLowerInvariant() + "  $name"
       if ($expected -ceq $actual) { "OK   $name" } else { "FAIL $name" }
   }
   ```

   Both lines must say `OK`. Then, from a clone of the repository at the tag, run the
   full verifier against the folder:

   ```powershell
   .\packaging\windows\verify-release.ps1 -MetadataPath <folder>\rust-browser-v0.20.0-windows-x64-release.json -ArtifactDirectory <folder> -CurrentVersion 0.0.0
   ```

5. Open `rust-browser-v0.20.0-windows-x64-release.json` and confirm `source_commit`
   is the tag's commit (`git rev-parse v0.20.0^{commit}`) and `version` is `0.20.0`.

If anything fails, do not publish. Delete nothing; report the run ID. A fixed
release gets a new version number and a new tag; never move an existing tag.

## 3. Publish

1. Open **Actions → Windows release → Run workflow**.
2. Branch: leave `main`. **release_tag**: `v0.20.0`. Turn on **publish**.
3. Select **Run workflow**. The package job runs again from the tag (it does not reuse
   the earlier artifact), then the **publish** job checks the five files, their
   checksums and source commit again and creates the GitHub Release with
   `release-notes/v0.20.0.md` as its text.
4. When it is green, open **Releases → Rust Browser v0.20.0** and check that the five
   files are attached and the notes are the reviewed ones.

## 4. After publishing

- Download `rust-browser-v0.20.0-windows-x64-setup.exe` from the release page and
  compare its SHA-256 with the `.sha256` file shown there.
- Record the release run IDs and the tag commit in `validation/RELEASE_READINESS.md`
  on the next development branch.

## What the workflow checks (for reference)

| Check | Where |
| --- | --- |
| Tag is `vX.Y.Z`, matches `Cargo.toml`, is the checked-out commit and is on `main` | `release.yml`, "Validate release tag and ancestry" |
| Format, Clippy, all tests, every native suite CI runs, three daily runs, audit | `release.yml`, package job |
| Portable ZIP and Setup built, installed, upgraded from the previous version, verified | `tests/windows_package.ps1`, `tests/windows_installer.ps1`, `tests/release_metadata.ps1` |
| Exactly five files, checksums, source commit | `release.yml`, publish job |
| Notes come from `release-notes/<tag>.md` | `release.yml`, publish job; `validation/test_release_workflow.py` |
| Publishing only by hand with **publish** on, from an existing tag | `release.yml` (`--verify-tag`) |
