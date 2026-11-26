# AGENTS.md — IP_PORT engineering rules

These rules apply to the entire repository, including automated contributors.

## Before changing code

- Read [ENGINEERING.md](ENGINEERING.md), [ARCHITECTURE.md](ARCHITECTURE.md), [DECISIONS.md](DECISIONS.md) and [ROADMAP.md](ROADMAP.md).
- Check the current main SHA and existing PRs. Work on a scoped branch; no direct changes to main or automatic merge.
- Keep unrelated dependency updates and architectural changes in separate PRs.
- Do not claim a command was executed unless an actual result is available.

## Validation

- Use the exact toolchain from rust-toolchain.toml; retain Cargo.toml MSRV 1.95.
- Run cargo fmt --all before committing Rust changes. Run the preflight script documented in [CONTRIBUTING.md](CONTRIBUTING.md).
- Preserve Cargo.lock unless a dependency change is explicitly intended. All check/clippy/test/build commands use --locked.
- Do not suppress Clippy warnings, skip tests, reduce dataset size, relax performance thresholds or change CI permissions to obtain a green result. Any justified exception requires review and documentation.
- If local execution is unavailable, disclose it and rely on actual CI evidence; do not describe unexecuted checks as passed.
- Review CI for the exact current PR head and report skipped/failed checks, reruns and flaky tests separately. Merge only after required checks pass and maintainer explicitly authorizes it.

## Architecture and OT semantics

- Use existing ScopePreview/InputCache for planned scope and the existing resolved-target exclusion path; do not add competing validators.
- Keep one authoritative results store and projections based on indices. Never equate IP, MAC OUI or port number with verified physical device identity.
- UDP Open|Filtered is not Open and has no measured RTT. Preserve profile v1, existing exports and cancellation behavior unless a change is separately approved.
- Do not perform blocking DNS, file I/O or network I/O on the GUI thread. Do not add automatic protocol writes or unapproved active probes.
- Add regression tests for changed behavior, including filter/sort/selection and clear/reset where applicable.
- Keep RU/EN, Light/Dark and compact workstation layout usable; Windows visual and bench acceptance remains manual.

## Documentation and release

- Keep roadmap, decisions, architecture, test docs and Unreleased changelog consistent. Record limitations and open work.
- Follow [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md). No tag or public release without explicit approval and verified CI for that commit.
