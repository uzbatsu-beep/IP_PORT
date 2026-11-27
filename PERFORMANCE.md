# Performance QA (ENG-01.B)

The 20k debug benchmark and 100k functional stress test remain mandatory and isolated. Optimized QA uses `python scripts/performance.py`: four sizes (1k, 20k, 50k, 100k endpoints), ten endpoints per IP, Devices and Ports, five fixed warm-up frames and 60 measured frames for each of eight scenarios. All samples are retained. 20k mean <16 ms and P95 <25 ms remain blocking, now also measured for both optimized views; other sizes report timing against the same target but do not introduce unapproved timing thresholds. All sizes require correct row counts, stable projection cache and bounded shape count (<5000). P95 uses the existing index convention (sorted index 57 of 60); P99 is max for 60 samples. Headless egui elapsed time is not real display/GPU FPS.

The JSON schema includes ingestion (direct storage, not event queue), initial projection, sort, filter, cold/warm-up samples, measured frame samples, mean/median/P95/P99/min/max, max shapes, OS/architecture/CPU count and CI SHA. RSS is a best-effort process working-set snapshot after rendering, not a peak or allocation delta: /proc/self/status on Linux, ps on macOS, Get-Process on Windows. Null denotes unavailable. The older debug benchmark still measures event ingestion via channel. Reports are uploaded even on a failed gate when generated; missing/incomplete report fails the job. Use a fixed runner/hardware for precise historical comparisons.

The A baseline (debug headless 20k after isolation): Windows mean 5.106 ms/P95 5.252 ms; Linux 3.484/3.546; macOS 5.979/10.210. Cold first frame 179.461/116.246/208.421 ms respectively. Source: CI 45840226622. No cold-start limit approved. Optimized data is a separate baseline, not directly comparable to debug.


C2 adds cumulative `projection_metrics` to each scenario (filter_rebuilds, port_sorts, group_rebuilds, host_sorts, device_row_rebuilds, view_rebuilds). All counters must stay unchanged during measured and warm-up frames of stable scenarios. C1 Windows 100k baseline: Devices mean 0.369 ms / RSS 44.4 MiB, Ports mean 0.433 ms / RSS 43.9 MiB (CI 45864084899); runner variation applies. Live ingestion and metadata stress remain C4.


## Live Performance QA (DEC-019)

Run `python scripts/performance.py` for existing steady-state QA and LIVE-01–11. The second optimized test uses real App frame/event drainage and a bounded 4096-event mock channel; 64/256/1024 arrivals, bursts, filters, sorts, injected worker metadata, selection, capacity, cancellation/terminal ordering, clear/rescan, background export and combined load. `target/live-performance-report.json` records raw frame samples, queue before/after, data/terminal accounting, event/enrichment/projection times, isolated endpoint lookup, stage counters, RSS and synthetic stop-to-terminal latency. Queue high water is sampled before frames. Throughput is events divided by measured frame CPU/wall time, excluding producer generation and inter-frame waits, not network throughput. Cancellation measures App drainage with a synthetic terminal, not socket cancellation; existing scanner cancellation tests remain. No GPU rendering or actual DNS/network probing. Timings are diagnostic (16 ms mean / 25 ms P95), functional accounting, bounds and complete report are blocking; previous steady-state gates unchanged. C4.3 optimization requires measured evidence.


## ENG-01.D2 — permanent baseline

Exact Windows Preview 1 source CI report payloads and provenance are retained in [qa/baselines/v0.5.0-eng-01/README.md](qa/baselines/v0.5.0-eng-01/README.md). LIVE-04 and LIVE-11 exceed diagnostic targets in this run; this is not a new blocking gate. Keep prior steady-state gates unchanged.
