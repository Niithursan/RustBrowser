# Releasing Rust Browser

Only the repository owner creates release tags and approves publishing. Automation
never creates a tag, and the release workflow publishes only when the owner runs it
by hand with **publish** turned on. These steps are for v0.23.0, the third published
release and the first with Linux packages; later releases follow the same steps with their own version.

Builds are **not signed**. Windows SmartScreen shows a warning for them; that is
expected until a signing certificate exists (see
[packaging/windows/UPDATE_SECURITY.md](packaging/windows/UPDATE_SECURITY.md)).

## Before you start

1. The release pull request is merged into `main`, and CI and Dependency security
   passed on the merge commit. Check on GitHub: **Actions**, filter by branch `main`,
   and open the newest **CI** and **Dependency security** runs for that commit. Both
   must show a green check. Do not continue if either failed or is still running.
2. On `main`, `Cargo.toml` says `version = "0.23.0"` and
   `release-notes/v0.23.0.md` exists. The release workflow refuses a tag that does
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
git tag -a v0.23.0 -m "Rust Browser v0.23.0"
git push origin v0.23.0
```

Use exactly `v0.23.0` (lower-case `v`, three numbers). The workflow refuses anything
else, a tag that is not on `main`, and a tag whose commit is not the one checked out.

## 2. Check the release workflow run

Pushing the tag starts **Release** automatically. This run builds and checks
everything but does not publish.

1. Open **Actions → Release** and select the run for `v0.23.0`.
2. Wait for all three build jobs to finish with a green check. They build without a
   cache, so expect them to take much longer than a CI run.
   - **package** (Windows; the v0.21.0 package job took 19 minutes, its limit is 40)
     runs a clean build, every regular and native test, three daily-driver runs and
     `cargo audit`, then builds and verifies the portable ZIP and Setup, including an
     upgrade from the published v0.22.2.
   - **linux** (limit 40 minutes) runs Clippy and the tests, builds the `.deb`,
     installs it in a clean Ubuntu 24.04 container and checks it.
   - **arch** (limit 45 minutes) builds the Arch package with its tests in an Arch
     Linux container, installs it there and checks it.

   If a job hits its limit or fails, report the run ID; do not publish.
3. Open the run's **Summary**, then **Artifacts**, and download all three:
   `windows-release-package`, `linux-release-package` and `arch-release-package`.
   Extract them into one empty folder. It must contain exactly nine files:
   - `rust-browser-v0.23.0-windows-x64.zip`
   - `rust-browser-v0.23.0-windows-x64.zip.sha256`
   - `rust-browser-v0.23.0-windows-x64-setup.exe`
   - `rust-browser-v0.23.0-windows-x64-setup.exe.sha256`
   - `rust-browser-v0.23.0-windows-x64-release.json`
   - `rust-browser-v0.23.0-linux-amd64.deb`
   - `rust-browser-v0.23.0-linux-amd64.deb.sha256`
   - `rust-browser-v0.23.0-arch-x86_64.pkg.tar.zst`
   - `rust-browser-v0.23.0-arch-x86_64.pkg.tar.zst.sha256`
4. Check the checksums in PowerShell in that folder:

   ```powershell
   foreach ($name in 'rust-browser-v0.23.0-windows-x64.zip', 'rust-browser-v0.23.0-windows-x64-setup.exe', 'rust-browser-v0.23.0-linux-amd64.deb', 'rust-browser-v0.23.0-arch-x86_64.pkg.tar.zst') {
       $expected = (Get-Content "$name.sha256" -Raw).Trim()
       $actual = (Get-FileHash $name -Algorithm SHA256).Hash.ToLowerInvariant() + "  $name"
       if ($expected -ceq $actual) { "OK   $name" } else { "FAIL $name" }
   }
   ```

   All four lines must say `OK`. Then, from a clone of the repository at the tag, run
   the full verifier against the folder (it checks the Windows files):

   ```powershell
   .\packaging\windows\verify-release.ps1 -MetadataPath <folder>\rust-browser-v0.23.0-windows-x64-release.json -ArtifactDirectory <folder> -CurrentVersion 0.0.0
   ```

5. Open `rust-browser-v0.23.0-windows-x64-release.json` and confirm `source_commit`
   is the tag's commit (`git rev-parse v0.23.0^{commit}`) and `version` is `0.23.0`.
6. Install the Linux packages the way the release notes tell users to: the `.deb` on
   Ubuntu (for example the Hyper-V VM) with `sudo apt install ./rust-browser-v0.23.0-linux-amd64.deb`,
   and, if you can, the Arch package on an up-to-date Arch with
   `sudo pacman -U rust-browser-v0.23.0-arch-x86_64.pkg.tar.zst`. In each, check that
   `rust-browser --version` prints 0.23.0, then open the browser from its launcher
   and load a page.

If anything fails, do not publish. Delete nothing; report the run ID. A fixed
release gets a new version number and a new tag; never move an existing tag.

## 3. Publish

1. Open **Actions → Release → Run workflow**.
2. Branch: leave `main`. **release_tag**: `v0.23.0`. Turn on **publish**.
3. Select **Run workflow**. The three build jobs run again from the tag (they do not
   reuse the earlier artifacts), then the **publish** job checks the nine files,
   their checksums and the Windows source commit again and creates the GitHub
   Release with `release-notes/v0.23.0.md` as its text.
4. When it is green, open **Releases → Rust Browser v0.23.0** and check that the nine
   files are attached and the notes are the reviewed ones.

## 4. After publishing

- Download `rust-browser-v0.23.0-windows-x64-setup.exe` and
  `rust-browser-v0.23.0-linux-amd64.deb` from the release page and compare their
  SHA-256 with the `.sha256` files shown there.
- Record the release run IDs and the tag commit in `validation/RELEASE_READINESS.md`
  on the next development branch.

## What the workflow checks (for reference)

| Check | Where |
| --- | --- |
| Tag is `vX.Y.Z`, matches `Cargo.toml`, is the checked-out commit and is on `main` | `release.yml`, "Validate release tag and ancestry" |
| Format, Clippy, all tests, every native suite CI runs, three daily runs, audit | `release.yml`, package job |
| Portable ZIP and Setup built, installed, upgraded from the previous version, verified | `tests/windows_package.ps1`, `tests/windows_installer.ps1`, `tests/release_metadata.ps1` |
| The Linux jobs build only the same approved tag | `packaging/linux/check-release-tag.sh` in the linux and arch jobs |
| `.deb` and Arch package built, tested, installed in a clean container and checked | `release.yml`, linux and arch jobs; `packaging/linux/check-installed.sh` |
| Exactly nine files, checksums, Windows source commit | `release.yml`, publish job |
| Notes come from `release-notes/<tag>.md` | `release.yml`, publish job; `validation/test_release_workflow.py` |
| Publishing only by hand with **publish** on, from an existing tag | `release.yml` (`--verify-tag`) |
