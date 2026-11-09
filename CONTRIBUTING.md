# Contributing

Use short-lived feature branches from an updated `main`:

```powershell
git switch main
git pull --ff-only
git switch -c feat/short-description
```

Use Conventional Commits: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, or `ci:`. Keep each commit focused; never commit credentials, profiles, or build output. Commit Cargo.lock for repeatable application builds.

Push the branch and open a pull request (a draft is fine) to `main`; CI runs on pull requests, not on feature-branch pushes. Describe behavior and validation using the PR template. Review the diff and wait for CI and Dependency security to pass before squash merging. Use a Conventional Commit PR title as the squash message. Do not force-push `main`.

Recommended repository rules: require pull requests and the `CI result` and `audit` checks (`CI result` passes only when both Windows jobs pass); block deletion and force pushes. These recommendations are not enforced merely by this file. A solo maintainer can review their own diff without requiring an impossible second approval.

## Releases

Obtain explicit approval before creating or pushing a release tag. After the
version bump and changelog are merged and main is verified, an approved matching
tag (for example `v0.11.0`) can trigger package validation. The workflow checks
the exact tag commit and main ancestry, repeats production/native checks and
dependency audit, then creates portable ZIP/checksum Actions artifacts.

Tag push alone does **not** publish a GitHub Release. Publication requires an
explicitly approved manual Windows release workflow invocation with the existing
stable `release_tag` and `publish` enabled. It rebuilds/validates that tag, verifies
the downloaded artifact checksum and refuses a missing tag. Review the package
and CI before requesting publication. This milestone does not invoke either
release action. See [Windows installation](INSTALL_WINDOWS.md) for packaging,
unsigned-executable, upgrade and removal limits. There is no deployment,
crates.io publication, signing or automatic updater.
