# Engineering Quality & Architecture — ENG-01

Approved 2026-11-26 (DEC-015). The objective is reproducible dependency resolution, consistent toolchain, reliable quality checks and incremental architecture improvement. This is not a claim of byte-identical binaries across operating systems or runner images.

## Plan and boundaries

| Stage | Scope | Acceptance |
|---|---|---|
| ENG-01.A | Pin Rust 1.100.0, preserve MSRV 1.95.0, locked builds, preflight, Actions revision, engineering rules | Existing five CI jobs pass; no Rust application behavior changes |
| ENG-01.B | Separate performance QA, machine-readable reports, Cargo Audit/Deny, Dependabot, Quality Gate and Ruleset preparation, release commit CI verification | Required gates fail closed; thresholds unchanged; policy reviewed before enforcement |
| ENG-01.C | Extract ResultsStore, ResultsProjection, typed SelectionState | Existing storage limits, scope/profile/export semantics, selection and live updates preserved; regressions and stress tests pass |
| ENG-01.D | Engineering acceptance, documentation and Windows test artifact | Green CI, compare performance, manual Windows smoke/bench test; no public release without approval |

No stages B–D are claimed implemented by A. No scanner probes or catalog changes in A.

## Toolchain

Development and release: Rust 1.100.0, Rust 2024; MSRV: 1.95.0. The root rust-toolchain.toml is the primary pin; MSRV uses explicit cargo +1.95.0. The manifest's rust-version remains 1.95. Use --locked for check, Clippy, tests and builds. Cargo fmt does not accept --locked. No dependency upgrades in A. Future stable compatibility check is proposed for B, nonblocking until agreed.

## Preflight

Python 3.10+ and rustup required. From repository root:

- Windows: `powershell -File scripts/preflight.ps1`
- Linux/macOS: `bash scripts/preflight.sh`
- Cross-platform: `python scripts/preflight.py` (or python3)

Use --format-only for the lightweight Rustfmt/docs check, not a substitute for full preflight. Run cargo fmt --all before committing Rust changes. The preflight stops at the first failed command; it never changes Cargo.lock or formats files automatically. CI runs the same format/docs stage and the existing matrix; the Python preflight regression tests run without Rust subprocesses.

## Actions revision

Exact upstream release commits, verified against public GitHub release tags and action.yml Node.js 24 runtime on 2026-11-26:

| Action | Version |
|---|---|
| actions/checkout | [v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1) |
| actions/upload-artifact | [v7.0.2](https://github.com/actions/upload-artifact/releases/tag/v7.0.2) |
| actions/download-artifact | [v8.0.2](https://github.com/actions/download-artifact/releases/tag/v8.0.2) |
| Swatinem/rust-cache | [v2.9.3](https://github.com/Swatinem/rust-cache/releases/tag/v2.9.3) |
| softprops/action-gh-release | [v3.0.4](https://github.com/softprops/action-gh-release/releases/tag/v3.0.4) |

Actions are pinned by full SHA with version comments. Checkout credentials are not persisted; use rustup directly rather than floating toolchain action refs. Release workflow still requires the exact tag and four packages. Tag-to-green-CI enforcement and dependency audit are B. Release workflow execution cannot be validated by ordinary PR CI; no tag is created for A.

## Open verification

After two Windows failures in A, the 20k and 100k tests run sequentially in separate Cargo test processes after functional tests. Preflight verifies both exact names exist before filtering the suite. The 20k benchmark reports five fixed cold/warm-up frames separately and measures all subsequent 60 frames with the original strict mean <16 ms and P95 <25 ms thresholds. Runner noise is still possible; cold-start delay is reported but has no separately approved limit. This is a measurement-method change, not evidence that application latency improved. Optimized-profile scenarios and JSON reports remain B. Rulesets are not changed by A. Previous API check returned no repository Rulesets and branch protection read was forbidden; maintainer must verify branch settings before activating Quality Gate. A retains existing job names to avoid breaking required checks. OS images, linker and system packages are not fixed by the Rust pin; exact byte reproducibility is out of scope.


## ENG-01.B implementation

See [PERFORMANCE.md](PERFORMANCE.md) and [DEPENDENCIES.md](DEPENDENCIES.md). The existing matrix remains mandatory; it runs isolated debug tests and optimized eight-scenario QA with validated JSON artifacts. Dependency Audit is a separate mandatory job. Quality Gate uses always() and requires success from formatting, matrix, MSRV and dependency audit (fail/skip/cancel blocks). Release checks exact checkout SHA against latest CI push run on main and successful Quality Gate, with actions:read only. No tag created; release gate unit-tested in CI but real tag execution pending. Ruleset JSON at [.github/rulesets/main-quality-gate.json](.github/rulesets/main-quality-gate.json) is a preparation artifact, not applied. Import only after green main Quality Gate; check existing protections and repository plan, select GitHub Actions as expected check source if available. Zero required approvals supports solo maintainer; PR and green CI remain required. No branch settings modified.


## ENG-01.C — Windows-first (DEC-016)

C1 encapsulates ResultsStore; a single row owns its ScanResult plus immutable derived display text. App exposes read-only results, not mutable Vec. IDs are generation/index pairs, stable during a session and invalid after clear. Revision changes on successful insert and clear; dropped count does not invalidate cached RTT. Existing index-based projection and selection compatibility remain until C2/C3. C2 moves the projection without algorithm changes, C3 replaces two-field selection, C4 adds live QA and integration. get_if_addrs replacement remains separate.

Routine PR and main CI now builds/tests Windows x64 only; MSRV check moves to Windows. Formatting and dependency audit retain Linux tooling but do not build the application. Quality Gate still requires all configured jobs. Linux/macOS QA moves to manually dispatched Cross-platform Acceptance workflow; run on exact main SHA at ENG-01.D/final acceptance and before any public release. Release verification requires successful main push CI and successful exact-SHA manual Cross-platform Gate. Four release targets remain supported, not claimed tested in ordinary development. No release tag created. Existing Ruleset remains unchanged; if old platform-specific required contexts exist, replace them with Quality Gate after checking settings. See [ARCHITECTURE.md](ARCHITECTURE.md).


DEC-017: C2 variant B approved, keep Windows-first, preserve UX and algorithm semantics, use revision keys and stage counters, remove duplicated protocol/state search checks. Existing counters and timing gates stay intact. No incremental insertion algorithm, new UI or dependency changes.
