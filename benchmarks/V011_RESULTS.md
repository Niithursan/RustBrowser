# v0.11 Windows performance findings

This is a small matched synthetic comparison, **not a speedup or memory-saving
claim**. It replaces the withdrawn first screening (see the end of this file).

- Baseline: PR #13 production `729198d023384103da0464f1148031aa6e681566` (0.8.0)
  plus the identical test-only probe module.
- Candidate: branch head `d77d9c9277f968e58d9b01f1db7680e0eb8aeec7` (0.11.0),
  including the superseded-blank navigation fix and minimized-window focus fix.
- Probe source SHA-256 (identical in both builds):
  `d25da3e7547716964f1a33cf589f6d51b09a82071e56bfe3228099c513a82ebe`.
- Corrected process-tree sampler (creation-time identity validation) SHA-256:
  `e5364acc8cc565f3f1a4ef0b18315760ef788b3daa710b3481fd2cb9f55dae03`.

Release test executables (built once, hash-checked before every case):

```text
baseline  7f54d2bb7d3e30451041073bdca61c382e998980ba020bcdb6ee8b7192909a37
candidate 576c6702abb18284e890b4a5114178ec15858df52e906b3e73c4ba9517f88094
```

## Environment and method

2026-11-09, Toronto: screening 15:51–16:06, soak 16:06–16:37. Ryzen 5 5600,
12 logical processors, 31.93 GiB RAM, Windows 11 Pro 10.0.26200 x64, Rust
1.99.1/MSVC, 100% scale, WebView2 **157.0.4341.42**. Power plan and thermal state
were not changed or recorded. No local build, native test or GUI test ran during
measurement; unrelated OS activity (for example Windows Search's own WebView2)
was uncontrolled. The desktop was unlocked, but lock state was not instrumented.

Each version ran 1/5/10/20 tabs × two repeats × fresh and same-reopened warm
profiles (32 runs). **Order alternated**: repeat 1 ran baseline first, repeat 2
ran candidate first. Parameters: 15-second experimental sleep delay (shipping
default 120 s), 15 s settle, 5 s idle sample, no soak, 5 s recovery after closing
all but one tab. All 32 runs reached `done`, recorded zero navigation errors and
finished normal shutdown; the harness verified sampled children exited by
identity before the next run. See [metric definitions](README.md).

Sanitized per-run numbers: [v011-corrected.json](v011-corrected.json). It excludes
process IDs, profiles, paths, page contents and raw logs. Cells below are median
**[min–max]** across the two repeats. Two repeats per cell cannot establish a
noise distribution; differences smaller than the min–max spread are noise here.

First usable is an instrumented proxy (native security + toolbar ready + active
page complete), not pixel presentation. Fresh profile is **not OS-cold startup**.
Private MiB is committed private bytes across the root and all WebView2
descendants; summed working set double-counts shared pages. CPU is one-core
percent (divide by 12 for whole-machine). Switch time is a selected-document
JavaScript callback, not input-to-display latency.

## Startup proxy, milliseconds

| Tabs | Profile | Baseline first usable | Candidate first usable |
| ---: | --- | ---: | ---: |
| 1 | Fresh | 715 [659–771] | 677 [651–704] |
| 1 | Warm | 663 [625–700] | 630 [627–632] |
| 5 | Fresh | 914 [879–949] | 967 [960–973] |
| 5 | Warm | 873 [838–907] | 941 [903–979] |
| 10 | Fresh | 1285 [1204–1365] | 1167 [1091–1243] |
| 10 | Warm | 1157 [1112–1202] | 1149 [1067–1231] |
| 20 | Fresh | 1655 [1646–1665] | 1746 [1745–1746] |
| 20 | Warm | 1611 [1563–1659] | 1655 [1638–1672] |

Mixed. Candidate 5-tab and 20-tab fresh ranges sit above baseline (about 50–90 ms)
while 10-tab fresh is lower; no consistent direction across tab counts. Two
repeats cannot attribute this to code rather than run variance.

## Process-tree private memory, MiB

| Tabs | Profile | Baseline idle | Candidate idle | Baseline after close | Candidate after close |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | Fresh | 318.2 [317.5–318.8] | 320.5 [320.0–321.0] | 318.9 | 321.3 |
| 1 | Warm | 322.3 [320.5–324.1] | 323.4 [321.4–325.3] | 323.1 | 324.2 |
| 5 | Fresh | 433.1 [430.5–435.6] | 430.2 [426.4–434.1] | 354.4 | 355.0 |
| 5 | Warm | 434.8 [433.1–436.5] | 434.9 [433.3–436.5] | 360.2 | 362.1 |
| 10 | Fresh | 561.9 [559.1–564.6] | 571.7 [569.9–573.5] | 389.7 | 391.1 |
| 10 | Warm | 564.2 [563.4–565.0] | 568.0 [565.3–570.7] | 393.9 | 393.6 |
| 20 | Fresh | 834.8 [832.9–836.7] | 842.0 [841.7–842.4] | 439.2 | 454.4 |
| 20 | Warm | 843.9 [836.3–851.5] | 843.2 [842.3–844.2] | 450.1 | 425.1 |

No memory reduction is established. Candidate idle private bytes are within about
±1–2% of baseline in every cell (10-tab fresh +9.8 MiB is the largest). After-close
values do not return to a single-tab fresh footprint for either version.

## Idle CPU and tab callback

| Tabs | Profile | Baseline idle CPU % | Candidate idle CPU % | Baseline callback ms | Candidate callback ms |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | Fresh | 4.1 [3.5–4.6] | 3.2 [2.6–3.9] | 3.7 | 3.8 |
| 1 | Warm | 7.2 [2.2–12.3] | 4.3 [3.5–5.2] | 3.5 | 4.9 |
| 5 | Fresh | 3.6 [0.4–6.7] | 5.2 [1.3–9.1] | 23.2 | 22.9 |
| 5 | Warm | 7.6 [7.1–8.1] | 8.4 [6.3–10.4] | 23.0 | 24.6 |
| 10 | Fresh | 6.1 [2.5–9.6] | 10.2 [7.9–12.5] | 21.6 | 21.6 |
| 10 | Warm | 3.9 [0.4–7.3] | 6.9 [2.1–11.7] | 21.6 | 21.4 |
| 20 | Fresh | 8.2 [6.8–9.7] | 9.3 [7.6–10.9] | 24.6 | 24.2 |
| 20 | Warm | 7.0 [3.1–10.8] | 7.3 [4.4–10.3] | 23.6 | 23.8 |

Five-second windows overlapping the 15-second asynchronous sleep boundary give wide,
overlapping CPU ranges; no CPU conclusion follows. The labelled idle phase still runs
the deliberate timer/busy-work fixture. Overall callback maxima: 31.2 ms baseline,
37.0 ms candidate. Eight baseline and nine candidate runs had at least one
inaccessible/exited process observation; missing observations are not zero usage.

## Extended stability run (passed)

Candidate executable above, one fresh disposable profile, 20 tabs, 30 s settle,
10 s idle, **1800 s soak** with create/close/navigate/switch churn every 10 s,
15 s recovery. Numbers: [v011-soak.json](v011-soak.json).

| Result | Value |
| --- | --- |
| Observed soak span / samples | 1798.9 s / 1825 |
| Churn cycles | 178 |
| Navigation errors | **0** |
| Final stage / exit | `done`, normal shutdown; sampled children exited |
| Diagnostic journal | only `starting`, `started`, `clean_shutdown`; no running marker left |
| Final 20-tab switch callbacks | all responsive, 19.9–26.9 ms |
| Hidden-tab ticks during idle | 0 for 19 hidden tabs; active tab 41 |
| Soak private bytes mean / range | 880.7 / 840.7–892.6 MiB |
| Soak summed working set mean / max | 937.5 / 1032.6 MiB |
| Soak CPU, one-core | 10.4% (about 0.9% of twelve logical CPUs) |
| Accessible process count | 33–35 |
| Unavailable observations | 26 of 1825 soak samples |

Five-minute private-byte means: 867.1, 881.3, 882.0, 883.6, 884.8, 885.5 MiB. After
the first window the drift is about 4 MiB over 25 minutes. One 30-minute synthetic
run can neither prove nor exclude a slow leak. This is not a real-site, media,
download, sign-in or interactive workload.

Earlier soak attempts are **not** counted: one was interrupted at 1176.7 s / 113
cycles; one completed 178 cycles but recorded 178 navigation errors from the
superseded-blank bug fixed in `c3ed142`.

## Warm-profile tab-close failure investigation

The stopped corrected campaign (candidate `28b292a` executable) failed once with
`Tab closing failed` in the 5-tab warm case, immediately after the switching phase.
That executable did not log the underlying native error, so the failing call
(visibility, bounds or focus) is unknown. About ten minutes earlier the desktop
had rejected `GetCursorPos` with access denied (locked/secure desktop).

Re-run under the unlocked desktop with identical parameters:

| Executable | 5-tab fresh+warm repeats | Result |
| --- | ---: | --- |
| Old `28b292a` candidate (no focus deferral) | 6 (12 runs) | 12/12 passed |
| Current `d77d9c9` candidate | 6 (12 runs) | 12/12 passed, no close diagnostic emitted |
| Final campaign above (`d77d9c9`) | 2 (4 runs) | 4/4 passed |

The failure did **not reproduce** in 28 runs, including 12 on the pre-fix binary.
The minimized-window regression independently reproduces a WebView2 `E_INVALIDARG`
focus rejection and the current code defers focus in that state. The evidence is
consistent with a desktop-state-dependent focus rejection, but the original root
cause is **unconfirmed**. Test builds now log the failing close-activation error and
native lifecycle state, so a recurrence will be diagnosable.

## Reproduction

Build release test executables from the exact source commits using
[README.md](README.md)'s procedure, then per version, tab count and repeat (alternate
order between repeats):

```powershell
.\benchmarks\windows.ps1 -ProbeExecutable <saved-probe.exe> -OutputRoot <fresh-scratch-root> `
  -Label <kind-sha256> -TabCounts <n> -Repeats 1 `
  -SettleSeconds 15 -SampleSeconds 5 -SoakSeconds 0 -RecoverySeconds 5
.\benchmarks\windows.ps1 -ProbeExecutable <candidate.exe> -OutputRoot <fresh-soak-root> `
  -Label <soak-label> -TabCounts 20 -Repeats 1 -SettleSeconds 30 -SampleSeconds 10 `
  -SoakSeconds 1800 -RecoverySeconds 15 -FreshOnly -Churn
```

Stronger evidence needs more independent repeats, longer steady-state windows after
suspension settles, recorded power/thermal/OS activity, and ETW/presentation traces
for true startup/display endpoints. True OS-cold startup, real-network navigation
and real-site workloads remain unmeasured.

## Withdrawn first screening

The first screening ([v011-screening.json](v011-screening.json), candidate
`58cdef9`, baseline-first order) is **withdrawn**: a short probe reproduced
process-ID reuse, where stale `ParentProcessId` links attributed unrelated processes
to a new browser. Its numbers are kept only for audit and must not be used for
comparison. The partial corrected campaign at `28b292a` (13 cases before the
tab-close failure) is superseded by the complete run above and is not mixed into it.
