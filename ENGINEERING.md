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
