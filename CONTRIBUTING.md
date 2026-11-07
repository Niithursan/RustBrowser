# Contributing

Use short-lived feature branches from an updated `main`:

```powershell
git switch main
git pull --ff-only
git switch -c feat/short-description
```

Use Conventional Commits: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`, or `ci:`. Keep each commit focused; never commit credentials, profiles, or build output. Commit Cargo.lock for repeatable application builds.

Push the branch and open a pull request to `main`. Describe behavior and validation using the PR template. Review the diff and wait for CI and Dependency security to pass before squash merging. Use a Conventional Commit PR title as the squash message. Do not force-push `main`.

Recommended repository rules: require pull requests and the `Windows quality and build` and `audit` checks; block deletion and force pushes. These recommendations are not enforced merely by this file. A solo maintainer can review their own diff without requiring an impossible second approval.

## Releases

After merging a version bump and changelog entry, tag the tested main commit with the matching Cargo version (for example `v0.1.0`) and push that tag. The release workflow verifies the tag, repeats Windows checks and security audit, builds a portable ZIP with SHA-256 checksum, and creates a GitHub release. It does not deploy a website, publish to crates.io, or sign the executable. Do not tag an unverified build.
