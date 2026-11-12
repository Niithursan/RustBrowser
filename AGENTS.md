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

Do not ask for approval for ordinary, reversible implementation decisions already covered by the milestone.

Request approval for destructive operations, major architectural changes, and changes to established project requirements.

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
- v0.19: Saved passwords, form autofill and bookmark import
- v0.20: Ad blocking and a new layout (done; the release prepared for it was not published)
- Then: daily use by the owner, followed by a v0.21 reliability and polish pass
- First tagged release: when the owner decides the browser is ready; not tied to a version
- v1.0: Daily-use release candidate

These milestones are planning targets, not guarantees of completed functionality.

Future releases may revise the roadmap based on technical findings.

**Always prioritize stability, maintainability, and security over release speed.**