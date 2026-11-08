# Servo v0.7 resource observation

Observed 2026-11-07 on Windows 11 Pro 10.0.26200 x64, AMD Ryzen 5 5600
(12 logical processors), 31.93 GiB RAM. WebView2 Runtime was 157.0.4341.42.
This is one short observation per browser shell, not a statistically meaningful
benchmark or an engine-only comparison.

## Workload and method

Both shells displayed one copy of the same controlled loopback fixture
(`fixture_sha256` in [the numeric sample](resources-v0.7.json)). The page includes
a 250 ms canvas animation and synthetic local interactions; it makes no external
requests. The collector sampled the root process and discovered descendants after
20 seconds of post-verification settling, for approximately 30 seconds. No builds
ran during sampling. The WebView2 run was first, followed by Servo. Both were
configured for a 1200x820 logical client size at scale factor 1.0; equal window
dimensions do not mean equal content viewports.

The sample records an existing Rust Browser release executable from source
`dfb9e58e8763a0f028555b54fc5cb06b710716c4` and WebView2 Runtime 157.0.4341.42.
Servo was the official v0.7.0 `servoshell` release from source
`aac43a3f31a259f04a574f5ec4e959c943ad7cc7`; the executable digest matches
[the pinned package](pin.json). WebView2 used a new persistent profile with one
seeded tab; Servo used a new config directory and `--temporary-storage`. Neither
used a normal browsing profile.

## Observed resource samples

| Shell | Samples / observed time | Mean private bytes (range) | Mean working set | Mean process count | CPU per logical core |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust Browser with WebView2 | 34 / 30.675 s | 323.21 MiB (322.22–323.81) | 690.20 MiB | 14 | 6.51% |
| Official Servo servoshell | 36 / 30.459 s | 297.18 MiB (297.15–297.29) | 210.00 MiB | 1 | 1.37% |

CPU is the sampled process-tree CPU-time delta divided by elapsed sample time,
expressed as a percentage of one logical core (not total-machine utilization).
Private bytes are committed private memory, not physical RAM. Summed working sets
can double-count shared pages. Sample spacing was about 0.84–0.95 seconds; the
per-shell interval duration and other raw numeric samples are retained in
[resources-v0.7.json](resources-v0.7.json).

## Interpretation and limits

These are corrected single-run observations of two different browser shells. They
do **not** show that Servo is inherently faster, uses less memory in equivalent
conditions, or is ready to replace WebView2. Shell UI and content viewport, process
topology, browser/runtime internals, profile/storage behavior, and foreground
conditions differ. The power plan, OS activity, and caches were not controlled;
foreground state was not tracked continuously. One run per shell provides no
repeatability or statistical confidence.

Equivalent startup/first-paint timing, numeric input latency, source-build time and
cost, and a real-site compatibility matrix are unavailable. This is not a security,
stability, leak, or daily-use test. In particular, the pinned Servo release does not
provide a supported Windows OS sandbox; low observed resource use does not change
that suitability blocker. See [research](RESEARCH.md) and the controlled procedure
in [README](README.md) before attempting another run.

For stronger comparisons, alternate shell order across several independent runs,
keep the fixture and measurement windows fixed, record foreground and machine
conditions, and instrument equivalent startup, presentation, and input endpoints.
Retain failed runs and report per-run observations rather than treating samples
within one process as independent trials.
