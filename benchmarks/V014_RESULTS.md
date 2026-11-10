# v0.14 matched measurements

Measured 2026-11-09 on one Windows 11 Pro x64 machine (10.0.26200), WebView2 Runtime
157.0.4341.42, Rust 1.99.1, release test-profile builds. Sanitized numbers:
[v014-screening.json](v014-screening.json) and [v014-soak.json](v014-soak.json).
Procedure and definitions: [README.md](README.md).

**Summary.** No difference between v0.13 and v0.14 can be established from these
runs. Every metric overlaps within the spread of its two repeats. v0.14 was not
changed to be faster and no performance improvement is claimed. The one deliberate
cost is not captured by this probe: a closed tab's process lives one second longer,
and the browser process exits about one second after its window closes.

## Method

- Baseline: `main` at `ca7ac14` (0.13.0) with its own committed probe. Candidate:
  `e3180e3` (0.14.0). The two probe sources differ only by one constructor field
  required by the v0.14 `App` struct.
- Frozen executables: baseline SHA-256 `983CC36B…FFC9F5CD`, candidate
  `146CE7D5…D6293ECC` (full values in the JSON). The harness refuses to run if either
  changes.
- 1, 5, 10 and 20 tabs; fresh profile then the same profile reopened; 15 s settle,
  5 s idle sampling, 5 s recovery after closing to one tab; 2 repeats. Repeat 1 ran
  the baseline first, repeat 2 the candidate first. 32 runs, all completed with zero
  navigation errors.
- No build, native test or other browser test ran during the campaign. Only text
  files were edited. The desktop session was not instrumented.

Cells are the median of 2 runs with [minimum–maximum]. Two repeats cannot support a
statistical comparison; treat differences smaller than the bracketed spread as noise.

## First usable (ms, instrumented proxy; not OS-cold start)

| Tabs | Profile | v0.13 | v0.14 |
| --- | --- | --- | --- |
| 1 | Fresh | 851.5 [780.2–922.9] | 784.2 [715.1–853.3] |
| 1 | Warm | 684.7 [641.8–727.6] | 726.7 [714.7–738.7] |
| 5 | Fresh | 1060.5 [1048.8–1072.2] | 1181.1 [1111.6–1250.7] |
| 5 | Warm | 863.8 [800.5–927.1] | 849.3 [790.5–908.1] |
| 10 | Fresh | 1445.1 [1438.2–1452.0] | 1372.7 [1339.7–1405.7] |
| 10 | Warm | 1125.1 [1101.2–1149.0] | 1097.7 [1095.4–1100.1] |
| 20 | Fresh | 1922.5 [1884.2–1960.8] | 1905.8 [1829.9–1981.7] |
| 20 | Warm | 1486.7 [1382.1–1591.3] | 1573.2 [1513.3–1633.1] |

## Idle process-tree private memory (MiB)

| Tabs | Profile | v0.13 | v0.14 |
| --- | --- | --- | --- |
| 1 | Fresh | 319.8 [319.8–319.8] | 321.1 [318.8–323.5] |
| 1 | Warm | 313.3 [313.1–313.4] | 314.4 [312.3–316.5] |
| 5 | Fresh | 432.4 [432.1–432.7] | 429.6 [429.2–430.0] |
| 5 | Warm | 412.9 [411.6–414.1] | 417.2 [416.9–417.5] |
| 10 | Fresh | 565.5 [563.1–567.9] | 566.8 [564.2–569.4] |
| 10 | Warm | 536.1 [535.9–536.2] | 530.3 [530.2–530.4] |
| 20 | Fresh | 838.6 [832.5–844.7] | 840.6 [837.7–843.4] |
| 20 | Warm | 772.0 [764.8–779.2] | 780.1 [779.2–781.0] |

## Private memory after closing to one tab (MiB, last three samples)

| Tabs | Profile | v0.13 | v0.14 |
| --- | --- | --- | --- |
| 1 | Fresh | 320.4 [320.4–320.5] | 321.8 [319.4–324.2] |
| 1 | Warm | 313.9 [313.7–314.1] | 315.0 [312.9–317.1] |
| 5 | Fresh | 356.9 [355.3–358.5] | 358.8 [358.3–359.3] |
| 5 | Warm | 344.9 [344.8–344.9] | 344.2 [343.6–344.9] |
| 10 | Fresh | 372.9 [359.4–386.4] | 392.7 [390.4–395.1] |
| 10 | Warm | 373.2 [372.5–374.0] | 369.2 [369.2–369.3] |
| 20 | Fresh | 452.0 [448.9–455.2] | 459.9 [458.9–460.9] |
| 20 | Warm | 428.5 [428.2–428.7] | 431.5 [429.1–433.9] |

The 10-tab fresh v0.14 value is above both baseline runs (392.7 against at most
386.4); the 10-tab warm value is below. With two repeats and the recovery window
only 5 s long, while v0.14 keeps closed views for 1 s, this is noted and not
interpreted. A longer recovery window with more repeats would be needed.

## Idle CPU (% of one core)

| Tabs | Profile | v0.13 | v0.14 |
| --- | --- | --- | --- |
| 1 | Fresh | 4.9 [4.5–5.4] | 6.0 [2.7–9.4] |
| 1 | Warm | 4.8 [3.0–6.7] | 3.5 [3.3–3.7] |
| 5 | Fresh | 6.7 [1.6–11.7] | 4.1 [3.6–4.6] |
| 5 | Warm | 4.3 [2.4–6.3] | 5.8 [3.3–8.2] |
| 10 | Fresh | 10.8 [7.4–14.2] | 5.2 [4.2–6.3] |
| 10 | Warm | 5.8 [4.8–6.9] | 4.2 [3.6–4.7] |
| 20 | Fresh | 14.2 [10.8–17.6] | 11.6 [8.4–14.8] |
| 20 | Warm | 10.8 [8.2–13.5] | 9.2 [5.9–12.6] |

The synthetic page runs a 250 ms timer with busy work in every tab, so these are
not the idle CPU of ordinary pages.

## Tab-switch callback (median ms; a JavaScript callback proxy, not input-to-display)

| Tabs | Profile | v0.13 | v0.14 |
| --- | --- | --- | --- |
| 1 | Fresh | 6.2 [6.2–6.3] | 6.7 [6.2–7.2] |
| 1 | Warm | 5.2 [5.1–5.4] | 5.0 [4.9–5.1] |
| 5 | Fresh | 41.4 [38.8–43.9] | 37.5 [37.2–37.7] |
| 5 | Warm | 29.4 [29.2–29.5] | 28.9 [27.4–30.5] |
| 10 | Fresh | 32.8 [31.8–33.8] | 32.5 [31.4–33.6] |
| 10 | Warm | 29.0 [28.4–29.5] | 29.7 [28.9–30.5] |
| 20 | Fresh | 33.6 [32.3–35.0] | 36.6 [35.2–37.9] |
| 20 | Warm | 30.3 [30.0–30.6] | 30.8 [29.4–32.2] |

Largest single switch callback: 49.4 ms (v0.13), 46.0 ms (v0.14). Samples in which
at least one process could not be read: 11 (v0.13), 13 (v0.14).

## 30-minute churn soak (v0.14 candidate only)

One run, 20 stable tabs plus one scratch tab that is closed, recreated and
navigated every 10 seconds, fresh disposable profile.

| Measure | Result |
| --- | --- |
| Soak span | 1799.4 s, 1768 samples; completed (`stage: done`) |
| Churn cycles | 178 |
| Failed navigations | 0 |
| Renderer or engine failures recorded | 0 (journal: starting 1, started 1, clean_shutdown 1) |
| Browser exit | Normal; sampled child processes exited; no running marker left |
| Process count | 34–35 throughout |
| CPU | 11.5% of one core over the soak |
| Private memory | mean 893.8 MiB, min 851.5, max 925.0 |
| Working set | mean 961.0 MiB, max 1102.4 |
| Private memory after closing to one tab | 407.1 MiB (idle before the soak: 827.7) |

Five-minute windows (mean private MiB / mean working-set MiB / CPU % of one core):
0–5: 878.0 / 983.1 / 10.3; 5–10: 893.4 / 970.9 / 11.8; 10–15: 896.6 / 960.9 / 10.7;
15–20: 896.3 / 953.1 / 12.7; 20–25: 897.6 / 948.9 / 11.6; 25–30: 900.5 / 949.3 / 11.7.

Private memory rose 22.5 MiB (2.6%) from the first to the last window, most of it
in the first five minutes, while the working set fell. One 30-minute synthetic run
cannot show whether that small rise continues; it does not prove the absence of a
memory leak, and it is not real-site browsing. Each churn cycle closes a tab, so
this run exercised the deferred view destruction 178 times.

27 samples had at least one unreadable process. CPU deltas omit processes that exit
between samples, and summed working set double-counts shared pages.
