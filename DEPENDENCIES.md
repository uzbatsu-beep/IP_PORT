# Dependency policy (ENG-01.B)

RustSec cargo-audit 0.23.0 and cargo-deny 0.22.1 are pinned tool installs with --locked. Audit the committed Cargo.lock; cargo deny --locked checks advisories, licenses, bans and sources for all four release targets. Vulnerabilities, yanked packages, unsupported licenses and unapproved sources block CI. Direct unmaintained dependencies block; transitive unmaintained advisories and duplicate versions require review, not automatic suppression. No advisory exceptions. The initial permissive SPDX allowlist is a project policy, not a legal opinion; review license obligations before distribution. Report unknown or incompatible licenses rather than broadening the list to make CI green.

Dependabot opens weekly Cargo and Actions PRs (limit five per ecosystem); no automatic merge. Review upstream release notes, lockfile diff, MSRV, compatibility, security and regressions. Audit tools may fail on real existing dependency findings; remediation must be explicit and independently reviewed. Advisory database changes are expected even for an unchanged lockfile. Reports are retained in CI for 30 days. See [ENGINEERING.md](ENGINEERING.md).

Upstream: [cargo-audit 0.23.0](https://github.com/rustsec/rustsec/releases/tag/cargo-audit%2Fv0.23.0), [cargo-deny 0.22.1](https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.22.1).
