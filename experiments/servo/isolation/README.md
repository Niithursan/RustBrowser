# Synthetic Windows boundary probe

This probe investigates Windows process boundaries using two trusted native
executables and synthetic objects. It accepts no renderer, URL, real file,
credential, or browser profile argument. Servo remains outside this probe.
Read the [threat model and isolation decision](THREAT_MODEL.md) before use.

The v0.10 [diagnostic mode](DIAGNOSTICS_V010.md) adds fixed-target read-only
queries, scoped pipes, resource controls, malformed/hung-worker tests and
live-IPC broker recovery for both AppContainer and LPAC. It is a separate
collector; the original full LPAC suite below remains a failing validation
when Winsock cannot start. See [measurements and actual GUI scope](../VALIDATION_V010.md).

## Run

Use Windows x64, Visual Studio C++ tools, and the Windows SDK. The script
finds the installed MSVC environment and compiles with `/W4 /WX /MT`.
It creates a fresh scratch deployment and per-user AppContainer profiles.

```powershell
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rust-browser-boundaries') -Repeats 3
```

The script rejects repository paths, recognized browser profiles and reparse
points through the existing Servo tools. Only new staged objects get ACL
grants. Each profile name is derived from its generated run GUID.
Windows security settings, firewall policies and loopback exemptions are
left to Windows. Scratch executables and sentinel files remain for inspection.

An explicit LPAC investigation uses the same protections and no fallback.

```powershell
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rust-browser-lpac') -Mode LPAC
```

On the tested machine this command fails at the child's Winsock startup.
The failure remains a failed validation result. The script reports the
run directory and cleanup status. The separate experimental GitHub workflow
uses the standard AppContainer control for pull requests and exposes LPAC
as an explicit manual investigation.

## What is checked

Before resume, the launcher checks AppContainer membership, the expected
package SID, zero capabilities, low integrity, Win32k lockdown, and the
child-process mitigation. Job assignment occurs at process creation.
The ordinary worker has no inherited handles and explicit null standard handles.
Its environment contains only SystemRoot and LOCALAPPDATA, which Windows
uses to locate the isolated profile.

The standard AppContainer worker verifies denied read/write access to a
private synthetic file, denied parent-process memory/duplicate access,
and inability to signal an inheritable parent sentinel event. A second
file is deliberately readable through ALL APPLICATION PACKAGES. Standard
AppContainer admits that read, and the LPAC branch requires it to fail.
This control shows why standard AppContainer and LPAC need separate evaluation.

The parent first reaches a listening loopback socket as a positive control.
The child records explicit WSAEACCES or a bounded connection timeout.
Only explicit WSAEACCES represents a measured policy denial. A timeout is
reported as inconclusive. Unexpected connection success fails the test.

The suite also tests a missing executable, worker termination, last-job-handle
closure, and actual broker termination while a worker exists. After the
broker crash, the supervisor checks the recorded PID and creation time,
then calls the fixed `--cleanup` mode in that owned run. Cleanup requires
the run marker, a GUID filename, and the journal created after successful
profile creation. It uses DeleteAppContainerProfile for that exact generated
name. API failure is reported, and scratch data is retained.

## Local results on 2026-11-08

- Windows 11 Pro 10.0.26200 x64, MSVC 19.44, Windows SDK 10.0.22621.
- Three initial AppContainer runs passed. Two further runs with the final
  lifecycle checks passed, including broker-crash recovery.
- Every network attempt in those runs timed out. General network isolation
  remains unverified.
- LPAC reached Winsock startup and failed with child code `70107`,
  representing startup error `10107`. The worker had already passed the
  private-file, ALL APPLICATION PACKAGES, and parent-process probes.
  This run did not complete the full boundary suite.
- Early reproductions omitted LOCALAPPDATA and failed CreateProcessW with
  error 203. An initial direct TokenIsLessPrivilegedAppContainer query
  returned error 87. Console/runtime startup needed explicit null standard
  handles. These observations are recorded to make failures reproducible.

These tests cover owned sentinels and lifecycle under this configuration.
They do not validate a Servo sandbox, a production renderer broker, general IPC,
filesystem confinement, all network paths, or hostile-content resistance.
Keep arbitrary content blocked until the threat model's acceptance gates pass.
