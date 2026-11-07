# v0.5 Windows performance findings

Measured on 2026-11-07: AMD Ryzen 5 5600 (6 cores / 12 logical processors), 31.93 GiB
RAM, Windows 11 Pro 10.0.26200 x64, WebView2 **157.0.4341.42**, Rust 1.99.1/MSVC,
scale factor 1.0. Power plan was not changed or independently recorded. No other
Rust Browser GUI instance or competing build ran during the comparison; unrelated
desktop/OS activity was not controlled. The fixture has no external network requests.

Baseline production source: `8e97713fa9c006586e250f07fdc4af12003b192d` (v0.4).
Candidate production source: `cce25b9` (v0.5). Both release-mode test executables use
the identical test-only probe. SHA-256:

```text
v0.4 7A78934F23CD7770B22F441824D684EF004671B321B0D854724E67C54966A25B
v0.5 60A90CEBC452A3456CA6959A679A75DBEC323E927EA65AD8EB3FAC49485D7598
```

See [procedure and metric definitions](README.md). These are instrumented startup
and JavaScript responsiveness proxies, not OS-cold startup or frame-presentation
latency. Each profile/count condition was run **once** in the screening matrix;
baseline ran first, candidate second. Fresh and warm conditions are not independent
repeats. Within-run memory ranges do not establish cross-run statistical confidence.

## Screening matrix

Twenty seconds settling, eight seconds idle, no soak, eight seconds after closing
all but one tab. Sleeping delay was shortened to 15 seconds on the candidate only;
the shipping default is two minutes. All sixteen runs finished their switches,
close/recovery phase and normal shutdown successfully. All sampled idle processes
were accessible; process counts were 14/18/23/33 for 1/5/10/20 tabs, including children.

Startup times in milliseconds (native construction / first usable proxy / all loaded):

| Tabs | Profile | v0.4 construction | v0.5 construction | v0.4 usable | v0.5 usable | v0.4 all loaded | v0.5 all loaded |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | Fresh | 699 | 728 | 902 | 946 | 902 | 946 |
| 1 | Warm | 645 | 665 | 849 | 743 | 849 | 743 |
| 5 | Fresh | 1046 | 1007 | 1360 | 1146 | 1382 | 1174 |
| 5 | Warm | 757 | 776 | 828 | 913 | 837 | 917 |
| 10 | Fresh | 1275 | 1371 | 1478 | 1564 | 1518 | 1598 |
| 10 | Warm | 1057 | 1046 | 1214 | 1222 | 1244 | 1247 |
| 20 | Fresh | 1935 | 1979 | 2222 | 2274 | 2248 | 2462 |
| 20 | Warm | 1470 | 1409 | 1689 | 1590 | 1697 | 2506 |

No startup speedup is established. The 20-tab warm all-loaded proxy was appreciably
slower on the candidate in this sample; repeat profiling is needed before assigning
a cause or making a general claim. First usable and all loaded describe different
events; background completion can lag behind the usable active tab.

Process-tree memory in MiB; idle means across samples, recovery is the final three
samples after closing all but one tab:

| Tabs | Profile | v0.4 private | v0.5 private | v0.4 working set | v0.5 working set | v0.4 recovery private | v0.5 recovery private |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | Fresh | 326.3 | 329.6 | 677.1 | 685.4 | 321.5 | 323.5 |
| 1 | Warm | 318.4 | 317.0 | 667.2 | 666.1 | 313.3 | 311.5 |
| 5 | Fresh | 442.9 | 433.9 | 913.3 | 911.8 | 345.9 | 351.1 |
| 5 | Warm | 428.4 | 419.7 | 890.0 | 892.7 | 333.5 | 339.4 |
| 10 | Fresh | 613.2 | 569.1 | 1230.4 | 1227.0 | 423.9 | 402.9 |
| 10 | Warm | 568.0 | 550.8 | 1178.1 | 1179.8 | 352.7 | 381.1 |
| 20 | Fresh | 888.0 | 852.8 | 1815.1 | 1825.6 | 436.8 | 408.6 |
| 20 | Warm | 822.0 | 781.4 | 1734.7 | 1741.9 | 418.6 | 390.3 |

For 20 fresh tabs, idle private-byte samples ranged 884.1–889.9 MiB on v0.4 and
849.8–856.2 MiB on v0.5. Warm ranges were 820.4–826.7 and 778.7–784.8 MiB.
Private bytes were about 4–5% lower in those two samples; summed working set was
slightly higher, so this is **not evidence of a physical-RAM reduction**. Shared
process allocations and unreclaimed heaps remain after closing tabs; recovery does
not return uniformly to a fresh single-tab footprint. No leak or leak-freedom is proven.

Per-switch document responsiveness, median / maximum milliseconds:

| Tabs | Profile | v0.4 JS callback | v0.5 JS callback |
| ---: | --- | ---: | ---: |
| 1 | Fresh | 8.8 / 8.8 | 9.3 / 9.3 |
| 1 | Warm | 7.5 / 7.5 | 10.1 / 10.1 |
| 5 | Fresh | 44.8 / 51.1 | 45.0 / 57.3 |
| 5 | Warm | 46.3 / 50.1 | 42.9 / 54.7 |
| 10 | Fresh | 43.6 / 50.2 | 62.2 / 76.5 |
| 10 | Warm | 44.7 / 50.2 | 55.5 / 67.1 |
| 20 | Fresh | 59.1 / 81.9 | 66.6 / 88.9 |
| 20 | Warm | 49.0 / 69.9 | 55.1 / 85.8 |

Each tab was selected once, including the previously active tab. Resume has a cost:
several multi-tab candidate samples were slower, while remaining responsive without
reload. These short, ordered samples cannot establish a general latency distribution.
Raw synchronous command timings are also provided in the numeric files.

Every hidden v0.4 tab advanced eight timer ticks during the eight-second idle sample.
Every hidden v0.5 tab advanced **zero**; its active tab advanced 32–33, consistent
with four ticks/second. Native regression checks independently confirmed suspension
and resumption of the same controller/document. This is the clearest observed activity
improvement; it does not imply all browser/network/service-worker activity stops.

The matrix CPU deltas were invalid because PowerShell selected an integer Math.Max
overload. They were excluded and replaced with `null` in the published numeric data.
An earlier incomplete run stopped at the child-exit guard; it and a concurrent-build
pilot are excluded. The harness now checks child exit for up to 60 seconds and uses
explicit double CPU arithmetic. Memory/timing fields were unaffected by that defect.

Sanitized numeric data: [baseline matrix](results/screening-baseline.json),
[candidate matrix](results/screening-current.json). These files contain no URLs,
profiles, page contents or executable artifacts.

## Corrected CPU and short stability screen

The separate 20-tab runs used the corrected sampler: 20 seconds settling, 20 seconds
idle, **60 seconds additional soak**, eight seconds recovery. Both versions used
the same executable hashes above. Fresh/warm conditions each ran once; all four
runs completed successfully, selected every tab without a responsiveness timeout,
closed to one tab, exited normally and passed the sampled child-exit guard.

CPU values below sum the app and engine descendants. Percentages are of **one logical
core**; this machine has twelve. Whole-machine equivalents are 5.72→4.35% fresh and
4.22→3.59% warm. Means exclude transition intervals and divide summed CPU time by
actual sampled elapsed time.

| Profile | v0.4 idle CPU | v0.5 idle CPU | Relative reduction | v0.4 interval range | v0.5 interval range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fresh | 68.59% | 52.21% | 23.9% | 44.7–94.0% | 37.9–71.2% |
| Warm | 50.66% | 43.03% | 15.1% | 25.0–72.6% | 32.7–69.8% |

There were 18–19 valid consecutive idle intervals per run, at roughly one second
including WMI overhead. Their wide overlapping ranges and the single run per
condition limit confidence; this is an observed workload result, not a general CPU
guarantee. CPU remains material for the active timer/title workload and browser UI.
No saving is claimed when important background work must remain awake.

During the twenty-second idle interval, each hidden baseline tab advanced twenty
ticks, each hidden candidate tab zero, and both active tabs eighty. For the following
minute, mean private bytes over the first/last three samples were:

| Version / profile | Soak first three (MiB) | Soak last three (MiB) |
| --- | ---: | ---: |
| v0.4 fresh | 886.7 | 891.0 |
| v0.4 warm | 825.6 | 829.5 |
| v0.5 fresh | 829.4 | 812.5 |
| v0.5 warm | 745.6 | 733.8 |

Those observations describe a short synthetic stability screen. They do not prove
hours-long browsing reliability, bounded memory growth, crash recovery for every
operation, audio/capture/download compatibility or real-site behavior. A rebooted
cold-start study, independent repeated trials, visual latency traces and a longer
interactive soak remain follow-up work. The slower screening switch/all-loaded
samples above remain reported rather than replaced by the more favorable rerun.

Corrected numeric data: [baseline CPU/soak](results/cpu-soak-baseline.json),
[candidate CPU/soak](results/cpu-soak-current.json).
