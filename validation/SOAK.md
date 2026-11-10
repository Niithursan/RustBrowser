# v0.6 extended Windows soak

The most recent soak is the v0.14 run recorded in
[../benchmarks/V014_RESULTS.md](../benchmarks/V014_RESULTS.md). This page keeps
the original v0.6 record.

Measured on 2026-11-07 (America/Toronto), Windows 11 Pro 10.0.26200 x64,
AMD Ryzen 5 5600 / 12 logical processors, 31.93 GiB RAM, WebView2 157.0.4341.42,
Rust 1.99.1/MSVC and scale factor 1.0. Power plan and unrelated OS/desktop activity
were not controlled. Early parts overlapped separate GUI/media checks; a five-second
collector smoke also overlapped. No Rust build ran during the measurement.

Measured source: `5de0d2149051fabf6f6ef27a84e08b69ad6d2635`. Release test-probe SHA-256:

```text
79D9EA30B74AA02481F02843E943235D02C570F7A8D1FE46208D871A6A852E5B
```

The later documentation/sampler extraction does not change that executable. The
long run used the sampler embedded in `benchmarks/windows.ps1` at this source commit;
the extracted helper preserves its collection logic. The added streaming CSV output
is for subsequent runs. No raw profiles, process IDs, command lines, URLs or logs
are included. [Numeric aggregates and minute bins](soak-v0.6.json) retain ranges,
sample counts, CPU intervals, unavailable observations and all switch measurements.

## Workload and result

One fresh-profile run: 30 seconds settling, 10 seconds idle, **1,800 seconds soak**,
then switching through every stable tab and 15 seconds recovery after closing to one.
Twenty stable synthetic tabs plus at most one scratch tab; every approximately ten
seconds a scratch tab is replaced and a stable tab selected. The fixture uses a
250 ms timer with 4 ms busy work, virtual HTTPS mapping and no external requests.
Sleeping delay is shortened to 15 seconds in the probe; shipping default is 120 seconds.

**Passed:** 178 churn cycles, all twenty final JavaScript responsiveness callbacks,
close-to-one recovery, normal host exit and the harness's sampled-child exit check.
Whole run took approximately 1,859 seconds. There were 1,889 resource samples,
maximum gap 1.229 seconds, and no gap over five seconds. Process churn produced
38 unavailable process observations (36 during soak, two while switching); these
are retained rather than interpreted as zero-resource processes.

| Observation | Result |
| --- | ---: |
| Native construction / first usable proxy / all loaded | 1,820 / 2,088 / 2,224 ms |
| Idle tick deltas | 19 hidden tabs: 0; selected tab: 40 |
| Final switch JavaScript callback median / maximum | 46.3 / 73.7 ms |
| Soak private bytes mean / range | 883.1 / 821.8–896.2 MiB |
| First five minutes private bytes mean | 866.0 MiB |
| Last five minutes private bytes mean | 889.8 MiB |
| Soak summed working set mean / range | 975.1 / 883.7–1,062.4 MiB |
| Soak CPU, one-core utilization | 29.7% (about 2.47% of twelve logical CPUs) |
| Soak accessible process count | 33–34 |
| Last five seconds after closing to one: private / summed working set | 415.4 / 762.4 MiB |
| After-close accessible process count | 14 |

CPU is summed sampled CPU deltas divided by elapsed intervals whose endpoints are
in the same group. Processes that exit between observations can contribute missing
CPU. Working-set sums double-count shared pages; private bytes are committed address
space, not physical RAM. Startup excludes loader/test-runner time and presentation;
fresh profile is not OS-cold startup. Switch callbacks measure document responsiveness,
not displayed frames. See [metric definitions](../benchmarks/README.md).

Private bytes increased between the early and late windows and recovered substantially
after tab closure. A single run, changing active tabs, Runtime allocations and
concurrent activity cannot establish leak absence, regression magnitude or an
improvement against v0.5. No matched v0.5 long-run comparison was performed. Historical
v0.5 measurements remain separate in `benchmarks/RESULTS.md`.

This is **automated synthetic churn**, not thirty minutes of human interactive
browsing. Short actual GUI checks and native upload/media/download/crash tests are
recorded separately in [COMPATIBILITY.md](COMPATIBILITY.md). Physical sleep/wake,
real providers/devices, mixed-DPI/multi-monitor and spoken screen-reader acceptance
remain deferred. The [interactive and hour-long procedures](README.md) are available
for wider acceptance testing; an hour-long run was not performed.
