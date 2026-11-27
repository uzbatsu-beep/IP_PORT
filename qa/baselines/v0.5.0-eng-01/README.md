# ENG-01.D2 — immutable Windows baseline

Exact source `0c9108b5bc9e176963404e98ef2b00320adaefa4`, [main CI](https://github.com/uzbatsu-beep/IP_PORT/actions/runs/45887472316), Windows job 140141754713, optimized headless egui. Full JSON payloads from the CI log, not reconstructed samples. See [metadata](metadata.json), [steady report](performance-report.json), [live report](live-performance-report.json). No overwrite of this baseline; add dated siblings for later comparisons. Artifact ZIP digest is provenance only; independent download byte verification not performed.

| Endpoints | View | Mean ms | P95 ms |
|---:|---|---:|---:|
| 1000 | devices | 0.356 | 0.402 |
| 1000 | ports | 0.457 | 1.129 |
| 20000 | devices | 0.399 | 1.049 |
| 20000 | ports | 0.386 | 0.941 |
| 50000 | devices | 0.361 | 0.591 |
| 50000 | ports | 0.373 | 0.869 |
| 100000 | devices | 0.339 | 0.409 |
| 100000 | ports | 0.372 | 0.768 |

| Live | Mean ms | P95 ms | Diagnostic target |
|---|---:|---:|---|
| LIVE-01 | 1.787 | 4.466 | within |
| LIVE-02 | 6.707 | 10.254 | within |
| LIVE-03 | 3.760 | 7.785 | within |
| LIVE-04 | 15.429 | 26.336 | outside |
| LIVE-05 | 2.532 | 4.231 | within |
| LIVE-06 | 11.318 | 18.027 | within |
| LIVE-07 | 5.039 | 22.553 | within |
| LIVE-08 | 6.493 | 10.108 | within |
| LIVE-09 | 2.672 | 5.341 | within |
| LIVE-10 | 4.344 | 6.370 | within |
| LIVE-11 | 16.534 | 21.982 | outside |

LIVE-04 P95 26.3363 ms and LIVE-11 mean 16.533892 ms are outside diagnostic targets on this run; no new blocking timing thresholds. PR #26 earlier results were 14.8753 ms and 12.325832 ms respectively, from a different runner. Functional accounting passed in both. This is not actual GPU FPS or physical network throughput; runner variance and short live samples limit inference. C4.3 requires repeat measurements on fixed hardware, after D.
