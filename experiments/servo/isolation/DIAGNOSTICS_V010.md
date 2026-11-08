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
an actual malformed-worker reply, a worker hang, and the existing
broker-crash recovery test. Every diagnostic API target is compiled in.
No registry values, provider names, user files, credentials, or website
contents are included in reports.

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
| Commit 128 MiB under a verified 64 MiB job cap | Commitment limit, 1455 | Commitment limit, 1455 |
| Token elevation / UIAccess | 0 / 0 | 0 / 0 |
| Private file read/write with IPC enabled | Denied | Denied |
| ALL APPLICATION PACKAGES file read | Allowed | Denied |

LPAC's catalog access failure is consistent with its missing registry
capability and is a plausible explanation for Winsock startup failure.
Provider DLL loading itself succeeded. These observations do not prove
which internal Winsock call produces error 10107. The NSI query failure
also occurs in the working AppContainer control, so it is not evidence
that the NSI service is absent or stopped.

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
The profile creation journal and cleanup rules are retained.

## Measurements and open work

The collector reports creation-to-first-validated-reply time,
resume-to-exit time, process CPU time, job peak committed memory and
sixteen echo round trips. The pipe uses 1 ms polling requests; actual
Windows timer scheduling can be much coarser. These are synthetic
worker/IPC measurements and do not measure browser or display latency.

Repeatable final samples and rendering feasibility results are still
pending at this checkpoint.

## Primary references

- [AppContainer and LPAC access model](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)
- [Explicit creation-time handle lists and mitigations](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [WSAStartup returns its own error code](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsastartup)
- [Winsock error meanings](https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2)
