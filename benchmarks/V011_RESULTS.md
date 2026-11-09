# v0.11 Windows performance findings

This is a small matched synthetic screening comparison, not a speedup claim.
Production baseline is PR #13 `729198d023384103da0464f1148031aa6e681566`
(0.8.0); candidate production code is
`58cdef9e9017e76a03c01baebbf9b44ad17dffcd` (0.11.0). Later packaging/docs
commits do not change the measured production code. The baseline probe was built
before production edits; the only earlier branch difference was a checkpoint
document. The same pre-existing probe and process-tree sampler were used.

## Environment and method

Screening result files were written during 2026-11-08 23:33–23:40 baseline,
then 2026-11-09 00:15–00:21 candidate (Toronto). Ryzen 5 5600, 12 logical processors, 31.93 GiB RAM,
Windows 11 Pro 10.0.26200 x64, Rust 1.99.1/MSVC, SDK 10.0.22621.0, 100% scale.
WebView2 **157.0.4341.42** was checked before/after. Power plan was not changed
or independently recorded. No competing local browser test/build ran in either
screening; unrelated desktop/OS activity was uncontrolled. Baseline-first order
and only **two repeats** per condition limit causal/statistical conclusions.

Release test executable SHA-256:

```text
baseline e7a54c2b4920f1df31424c7325ab08c8ae6b0c6b5dd42f0c94e38c672ef873f0
candidate 5127be5cc284c351dfdd5dbd7120458f9e3bfccda9e31c5e5d11bf4df339bda6
```

Each version completed 16 cases: 1/5/10/20 tabs × two repeats × fresh and same
reopened warm profiles. Both versions have the same 15-second experimental
sleep delay (shipping default 120 seconds), 15-second settling, 5-second idle
sampling, zero soak, and 5-second recovery after closing all but one tab.
Virtual HTTPS serves the identical bundled timer/busy-work fixture without
external requests. See [metric definitions](README.md).

First usable is a native security/toolbar-ready/active-page completion proxy,
excluding process loader/test setup and actual pixel presentation. Fresh profile
is **not OS-cold startup**. Switch responsiveness is a selected-document JS
callback, **not input-to-display latency**. True cold, presentation and real
network navigation benchmarks are unavailable. No arbitrary engine tuning or
view destruction was added to manufacture memory savings.

The root and recursively observed WebView2 children are sampled about once per
second (750 ms plus WMI overhead). Private MiB is committed private address
space; summed working set double-counts shared pages. CPU is one-core percent:
sum interval deltas after the first phase sample / phase span × 100. Divide by
12 for whole-machine percent. Missing observations and exit-between-sample CPU
are not zero consumption. All 32 runs finished normal shutdown and the harness
waited for sampled children to exit before the next run.

Sanitized per-run numeric evidence, hardware, parameters and definitions:
[v011-screening.json](v011-screening.json). It excludes process IDs, profiles,
paths, page contents and raw logs. Memory rows use each run's labelled idle mean,
then the median/min/max across two independent repeats. Recovery uses the final
three after-close samples. Fresh/warm are different conditions, not repeats.

## Startup proxy, milliseconds

Cells show median **[min–max]** across two repeats.

| Tabs | Profile | Baseline first usable | Candidate first usable |
| ---: | --- | ---: | ---: |
| 1 | Fresh | 737 [731–742] | 747 [686–808] |
| 1 | Warm | 646 [590–702] | 627 [619–635] |
| 5 | Fresh | 940 [914–966] | 933 [921–945] |
| 5 | Warm | 717 [696–739] | 812 [769–856] |
| 10 | Fresh | 1332 [1303–1361] | 1284 [1179–1390] |
| 10 | Warm | 1042 [980–1104] | 942 [861–1022] |
| 20 | Fresh | 1846 [1836–1856] | 1705 [1663–1747] |
| 20 | Warm | 1311 [1311–1311] | 1336 [1228–1444] |

Results are mixed. In this screening the 5-tab warm proxy is slower on the
candidate and the 20-tab fresh proxy is faster. Two ordered repeats cannot
establish a general improvement, regression cause or noise distribution.

## Process-tree private memory, MiB

| Tabs | Profile | Baseline idle median [range] | Candidate idle median [range] | Baseline after close | Candidate after close |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | Fresh | 322.3 [318.9–325.6] | 319.9 [318.7–321.2] | 323.0 | 320.3 |
| 1 | Warm | 309.5 [308.1–310.9] | 314.2 [312.7–315.8] | 310.7 | 314.9 |
| 5 | Fresh | 431.2 [427.1–435.4] | 432.6 [430.8–434.4] | 353.7 | 356.7 |
| 5 | Warm | 405.2 [403.6–406.9] | 413.4 [412.9–413.8] | 338.2 | 342.6 |
| 10 | Fresh | 561.2 [560.8–561.6] | 563.4 [558.7–568.0] | 372.9 | 387.3 |
| 10 | Warm | 527.5 [524.7–530.2] | 527.0 [522.7–531.3] | 367.6 | 366.7 |
| 20 | Fresh | 834.3 [827.4–841.3] | 847.0 [841.9–852.2] | 437.2 | 444.6 |
| 20 | Warm | 762.4 [756.8–768.0] | 774.3 [765.4–783.3] | 426.1 | 425.4 |

No memory reduction is established. Candidate 20-tab private commit is slightly
higher in this sample. Fresh 20-tab summed working-set means are 1788/1775 MiB
baseline and 1794/1784 MiB candidate; these are **not exclusive physical RAM**.
After-close heaps/shared engine storage do not return to a fresh single-tab
footprint. Neither a leak nor leak freedom follows from this short screening.

## Idle CPU and tab callback

CPU cells are median [min–max] of one-core percent. Callback cells are medians
of each run's per-tab callback median, milliseconds; full per-run maxima/ranges
are in the JSON. Overall callback maxima were 35.7 ms baseline, 34.0 ms candidate.

| Tabs | Profile | Baseline idle CPU | Candidate idle CPU | Baseline callback | Candidate callback |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1 | Fresh | 5.51 [4.64–6.38] | 2.94 [0.89–4.99] | 4.57 | 5.08 |
| 1 | Warm | 4.82 [4.28–5.35] | 5.88 [4.29–7.48] | 3.98 | 3.91 |
| 5 | Fresh | 3.68 [1.04–6.31] | 1.41 [0.35–2.47] | 26.66 | 28.95 |
| 5 | Warm | 6.43 [1.70–11.16] | 1.94 [0.35–3.53] | 21.32 | 22.97 |
| 10 | Fresh | 5.72 [3.13–8.30] | 11.52 [8.20–14.84] | 26.09 | 26.86 |
| 10 | Warm | 4.52 [3.47–5.56] | 4.16 [3.47–4.85] | 23.42 | 25.26 |
| 20 | Fresh | 13.71 [12.00–15.43] | 10.16 [10.06–10.25] | 29.75 | 30.24 |
| 20 | Warm | 11.94 [9.28–14.60] | 11.38 [7.10–15.66] | 27.50 | 26.99 |

The 10-tab fresh candidate CPU interval is higher; other cells move in both
directions. Five-second windows and asynchronous sleep at the 15-second boundary
are too short for a causal claim. Active timers advanced about 20 ticks; hidden
tabs advanced zero or one during that interval. Native `IsSuspended` tests give
separate API evidence; tick zero alone does not prove all engine work stopped.
Eight baseline and ten candidate samples had at least one inaccessible/exited
process observation across startup/transitions/shutdown; preserve that limitation
rather than treating missing process counters as measured zero.

## Extended stability run

A separate fixed-candidate 20-tab run uses 30 s settle, 10 s idle, **1800 s soak**,
15 s recovery, fresh disposable profile and ten-second create/close/navigate/switch
churn. No local build/native/GUI test runs concurrently. It is currently in progress;
completion, minute-window resource observations and errors will be recorded here.
It is synthetic, not a 30-minute interactive real-site or media/download workload.

## Reproduction and next measurements

Build/store release test executables from the exact source commits using
[README.md](README.md)'s procedure. The baseline already contains the identical
probe, so no production-source patch is needed. Run each with:

```powershell
.\benchmarks\windows.ps1 -ProbeExecutable <saved-probe.exe> -OutputRoot <fresh-scratch-root> `
  -Label <source-sha> -TabCounts 1,5,10,20 -Repeats 2 `
  -SettleSeconds 15 -SampleSeconds 5 -SoakSeconds 0 -RecoverySeconds 5
```

For stronger evidence alternate version order, use longer steady-state windows
after suspension settles, record power/thermal/OS activity, increase independent
repeats and use ETW/presentation traces for true startup/display endpoints. Include
real protected workloads only with isolated test data and explicit device/account
consent. Diagnostics are bounded/coalesced but this comparison does not isolate
their overhead. Prioritize those experiments before performance tuning claims.
