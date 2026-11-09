# Windows Servo isolation threat model

Assessed 2026-11-08 against Servo revision
`65d058b9c6543840c1d986f7cae4520605bf5b5c` and the Rust Browser v0.8
fixture host. Assume hostile HTML, JavaScript, URLs, fonts, images, and
network responses can exploit an engine defect and execute native code.
The renderer's own policy checks can then be bypassed.

## Assets and trust boundaries

| Asset or boundary | Threat | Required control | Current evidence |
| --- | --- | --- | --- |
| Normal browser profiles, credentials, and user files | Renderer reads or modifies data with the current user's access | OS access checks against a dedicated package identity, limited file grants, no profile sharing | Servo fixture host still runs with user access. Synthetic AppContainer tests cover only owned sentinel files. |
| Network, local services, and credentials | Exfiltration, loopback requests, intranet access, authentication | No renderer network capabilities, explicitly scoped broker, DNS/redirect/credential policy | Probe token has zero capabilities. TCP timeout and UDP nonreceipt after positive parent controls are inconclusive. LPAC WSAStartup fails 10107. DNS, IPv6 and remote endpoints are untested. |
| Broker process and Windows objects | Memory access, handle reuse, impersonation, unwanted child processes | Restricted token, explicit handle allowlist, null standard handles, child policy, scoped IPC | Synthetic worker cannot open its parent for memory/duplicate access or signal an unlisted inheritable sentinel. Diagnostics inherit exactly two pipe endpoints. Actual descendant creation is denied; elevation/UIAccess are off and enabled privileges checked. |
| Renderer-to-broker messages | Deserialization bugs, path traversal, replay, resource exhaustion | Bounded typed messages, per-instance identity, strict state machine, no arbitrary paths or execution | Fixed 48-byte diagnostic frames check sequence, run identity, fixture identifier, type, size and opcode. Twelve-field mutation, malformed reply, hang and live-IPC broker-crash tests pass. No browser operations or content-origin authorization exist. |
| Graphics and input | Malicious surfaces, shared-memory handles, GPU/Win32k attack surface | Separate graphics broker, narrowly scoped surface rights, validated dimensions and input routing | Pinned software/offscreen paths still use WGL. System OpenGL initialization fails 1114 under Win32k lockdown in both synthetic workers. No real Servo surface/renderer was created there. |
| Process and profile lifetime | Orphan workers, PID reuse, persistent data after broker crash | Job at creation, no inherited job handles, process identity checks, owned cleanup journal | Synthetic worker termination, last-job-handle closure, broker termination, and exact-profile recovery passed locally. |
| Privileged browser UI | Untrusted content invokes bookmark, filesystem, or application actions | Keep UI in another process/profile, expose no Rust Browser IPC to content | Servo remains a standalone fixture host with no application IPC. The test report parser accepts bounded numbers only. |
| Availability | Infinite loops, allocation bombs, process storms, broker starvation | Process count/resource budgets, deadlines, cancellation | Diagnostic job limits active processes to one and private commit to 64 MiB; 1 MiB allocation succeeds and 128 MiB fails. Fixed IPC counts and deadlines bound the synthetic workload. Real renderer/GPU budgets and long stress remain unverified. |

The threat model covers compromise of a renderer under an otherwise trusted,
updated Windows installation. It does not establish protection against a
compromised kernel/driver or a hostile administrator. Browser-level site
isolation, origin storage separation, downloads, and secure-origin device
permissions would need additional designs and tests.

## Candidate architecture

1. A trusted launcher creates a unique AppContainer identity and a small
   immutable worker deployment. It grants execute/read access only to that
   package and leaves normal profiles outside the grants.
2. The launcher derives a restricted LUA primary token from its own token,
   disables maximum privileges and the Administrators SID, and verifies
   non-elevation before CreateProcessAsUserW. It never falls back to the
   original token. The worker starts suspended with a minimal environment, no inherited
   handles except explicitly reviewed IPC endpoints, null standard handles, a creation-time Job Object,
   child-process restrictions, and compatible mitigations. The launcher
   validates the resulting token before resuming it.
3. A content worker receives controlled document bytes through a reviewed
   channel. It receives no normal browsing cookies, arbitrary local paths,
   unrestricted sockets, shell launch, or production UI bridge.
4. A broker validates every request with a typed allowlist, bounded payload,
   instance identity, and timeout. File selection, HTTP/TLS/cookies,
   redirects, and graphics remain separate audited capabilities.
5. Closing the broker kills the worker job. Cleanup uses an ownership
   journal and exact generated identity after process exit. Downloads or
   user-selected data would require a separate lifetime policy.

This is a candidate design. The synthetic probe verifies a subset of the
Windows mechanisms. A separate process alone provides no access restriction.
The existing Servo navigation delegate and CSP provide fixture policy, and
cannot contain a compromised engine.

## Decision and blockers

Keep Servo on the two owned fixtures. Arbitrary content is blocked pending
all of the following work.

- A supported Windows content sandbox in Servo, or an audited external
  worker/broker integration. The pinned Windows `create_sandbox` panics.
  The current host uses single-process defaults. Enabling multiprocess
  alone would not add a Windows sandbox.
- A supported headless/offscreen rendering and surface-transfer route
  compatible with selected process mitigations. The probe's Win32k
  lockdown prevents native UI creation and cannot simply be applied to
  the working WGL window.
  The [v0.10 source assessment](../SECURE_RENDERING_V010.md) also finds that
  the pinned software adapter is a low-power WGL adapter and offscreen
  rendering retains a parent window context. No verified alternate route
  has been substituted.
- A documented minimal dependency/capability inventory. The capability-free
  LPAC worker failed Winsock initialization with `WSASYSCALLFAILURE 10107`.
  The direct LPAC token query also returned error 87 in an initial attempt.
  Neither result justifies granting registry, networking, or broad file
  capabilities to get Servo running.
  Read-only v0.10 diagnostics find denied catalog access despite successful
  provider DLL loading. This supports investigation, not a unique causal
  explanation. No extra capability is added.
- Positive and negative tests for all network transports, allowed object
  access, profile persistence, broker message fuzzing, handle rights,
  renderer crashes, and resource exhaustion. A two-second loopback
  timeout does not prove network containment.
- Independent review of the native boundary and deployment/update policy
  before treating it as suitable for hostile web content.

## Primary references

- [Pinned Servo sandbox entry point](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/servo/servo.rs)
- [Pinned default options](https://github.com/servo/servo/blob/65d058b9c6543840c1d986f7cae4520605bf5b5c/components/config/opts.rs)
- [Microsoft AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)
- [Microsoft AppContainer and LPAC launch procedure](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)
- [Creation-time attributes, handles, mitigations and job assignment](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [Job Objects and process lifetime](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
- [Microsoft sandbox tooling and Win32k limitations](https://github.com/microsoft/SandboxSecurityTools/blob/main/LaunchAppContainer/README.md)
