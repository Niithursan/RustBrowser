# Security

This is an experimental single-tab browser. Report suspected vulnerabilities privately through GitHub private vulnerability reporting if enabled, or contact the repository owner privately. Do not include cookies, passwords, browsing profiles, or personal URLs in public issues.

Version 0.1 uses the installed Microsoft WebView2 Runtime. Keep that runtime and Windows updated. Remote content receives no application IPC handler; the local toolbar has a separate profile and restricted navigation. Website permission requests and downloads are denied in this version. HTTP is supported for development and does not provide HTTPS transport protection.

Dependency security runs on pushes to main, pull requests and a weekly schedule. Dependabot proposes Cargo and GitHub Actions updates; they still require review and passing checks. The release executable is unsigned. This project has not undergone an independent security review.
