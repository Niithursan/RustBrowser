# v0.10 Windows diagnostics

This is an investigation on trusted synthetic workers. The production
version and default engine remain unchanged. The original LPAC suite
still treats Winsock startup failure as a failed boundary validation.
The new collector records that failure as diagnostic data.

## Reproduction

```powershell
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-ac') -Diagnostics
.\experiments\servo\isolation\check-boundaries.ps1 -OutputRoot (Join-Path $env:TEMP 'rb-v010-lpac') -Diagnostics -Mode LPAC
```

The script uses the same pinned mitigation configuration and fresh-profile
rules as v0.9. It additionally compiles protocol tests. It runs a collector,
an actual malformed-worker reply, a worker hang, and a broker crash while
the scoped IPC worker is alive. These cases retain the selected mode's
AppContainer or LPAC controls. Every diagnostic API target is compiled in.
No registry values, provider names, user files, credentials, or website
contents are included in reports.

The first hosted Windows Server 2025 run rejected an elevated-token
indication before worker resume and cleaned the generated profile. The
launcher now uses CreateRestrictedToken with DISABLE_MAX_PRIVILEGE and
LUA_TOKEN, explicitly makes Administrators deny-only, verifies non-elevation,
and calls CreateProcessAsUserW with the existing lowbox attributes. No
fallback or security-setting change was added. The corrected
[hosted boundary job](https://github.com/Niithursan/RustBrowser/actions/runs/42559913981)
passed on commit `acbf217`. That Server 2025 collector also reported LPAC
catalog denial, WSAStartup 10107 and OpenGL initialization 1114. The
updated local samples use the filtered launch; hosted timings are not
mixed into the local benchmark dataset.

## Initial evidence on Windows 11 10.0.26200

| Operation | AppContainer | LPAC |
| --- | --- | --- |
| Open Protocol_Catalog9 for query | Success | Access denied, 5 |
| Open NameSpace_Catalog5 for query | Success | Access denied, 5 |
| Open service manager for connect | Success | Access denied, 5 |
| Query the NSI service | Access denied, 5 | Access denied, 5 |
| Read ws2_32.dll and mswsock.dll | Success | Success |
| Load mswsock.dll from System32 | Success | Success |
| Read opengl32.dll | Success | Success |
| Load opengl32.dll with Win32k lockdown | DLL initialization failure, 1114 | DLL initialization failure, 1114 |
| WSAStartup return value | 0 | WSASYSCALLFAILURE, 10107 |
| Create a synthetic descendant | Child process blocked, 367 | Child process blocked, 367 |
| Commit 1 MiB / 128 MiB under verified 64 MiB cap | Success / commitment limit, 1455 | Success / commitment limit, 1455 |
| Token elevation / UIAccess | 0 / 0 | 0 / 0 |
| Private file read/write with IPC enabled | Denied | Denied |
| ALL APPLICATION PACKAGES file read | Allowed | Denied |

LPAC's catalog access failure is consistent with its missing registry
capability and is a plausible explanation for Winsock startup failure.
Provider DLL loading itself succeeded. These observations do not prove
which internal Winsock call produces error 10107. The NSI query failure
also occurs in the working AppContainer control, so it is not evidence
that the NSI service is absent or stopped. A separate read-only query from
the normal test session reported the NSI service running. No service,
registry ACL, loopback exemption or firewall setting was changed.

AppContainer's UDP send reported eight bytes accepted, but its datagram
was not observed within the 200 ms receive window after a successful
parent send/receive control. LPAC could not initialize Winsock, so its
send was unavailable. Neither observation proves general network isolation.
The original TCP timeout remains inconclusive.

No extra registry, filesystem, network, device or process capability was
granted. No mitigation was removed to obtain these results.

## IPC boundary

Two anonymous pipe endpoints are delegated through an explicit
PROC_THREAD_ATTRIBUTE_HANDLE_LIST. All other handles remain excluded,
including a deliberately inheritable sentinel. The standard handles stay
null. The ordinary v0.9 launch path still inherits no handles.

The kernel endpoint capabilities bind the connection to the suspended
worker created and token-verified by the broker. The instance identifier
also prevents accepting a stale report from another run. It is not
website-origin authentication.

Frames contain twelve fixed little-endian 32-bit words. The broker checks
magic, version, exact 48-byte size, message kind, sequence, synthetic
fixture-origin identifier, instance identifier and allowed operation.
There are no paths, strings, pointers, arbitrary operations, or
renderer-provided handles in the protocol. The synthetic origin identifier
cannot authorize future website permissions.

Read deadlines and fixed message counts bound protocol work. Total
broker writes fit below the pipe capacity even if the worker stops
reading. The worker has a 64 MiB committed-memory cap and a one-process
job. Malformed replies and hangs are terminated through that job.
The profile creation journal and cleanup rules are retained. In the
intentional broker-crash case the supervisor verifies the exact child's
PID/creation time after job closure and recovers only the generated profile.
The launcher also verifies non-elevation, UIAccess off and an enabled
privilege allowlist containing only change-notify. This is not a claim that
every Windows object accessible to the worker has been enumerated.

## Measurements and open work

The collector reports creation-to-first-validated-reply time,
resume-to-exit time, process CPU time, job peak committed memory and
sixteen echo round trips. The pipe uses 1 ms polling requests; actual
Windows timer scheduling can be much coarser. These are synthetic
worker/IPC measurements and do not measure browser or display latency.

Three final samples per mode, the original LPAC failure, actual Windows GUI
scope and metric definitions are in [v0.10 validation](../VALIDATION_V010.md).
The [sanitized numeric data](measurements-v010.json) retain all sixteen echo
times per run and typed operation results without profiles or identifiers.
The [rendering decision](../SECURE_RENDERING_V010.md) documents why a real
Servo worker was not launched under these restrictions.

## Primary references

- [AppContainer and LPAC access model](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)
- [Explicit creation-time handle lists and mitigations](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [WSAStartup returns its own error code](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsastartup)
- [Winsock error meanings](https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2)
- [Restricted primary tokens and privilege removal](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken)
- [CreateProcessAsUser with the caller's restricted token](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw)
