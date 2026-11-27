# ENG-01.D1 — инженерный аудит

Дата 2026-11-27; inspected source `0c9108b5bc9e176963404e98ef2b00320adaefa4`; [main CI 45887472316](https://github.com/uzbatsu-beep/IP_PORT/actions/runs/45887472316), [Windows Preview workflow 45889664961](https://github.com/uzbatsu-beep/IP_PORT/actions/runs/45889664961) — SUCCESS. PR #25/#26/#27 merged. 153 functional + 4 isolated Rust tests passed; 8 steady and 11 live scenarios. No runtime source change in this PR. D1/D2/D3 preparation only; D4 remains open.

## Audit scope

All 27 Markdown files at source tree enumerated and read for headings, version/status/CI contradictions, historical versus normative claims. This is a documentation consistency and targeted architecture audit, not a fresh external-source verification of industrial standards, license opinion, full static security audit, or Windows manual test. Edited documents below; historical content intentionally retained.

| File | Disposition |
|---|---|
| .github/pull_request_template.md | Updated status, policy, links, or acceptance guidance |
| ACCESSIBILITY.md | Updated status, policy, links, or acceptance guidance |
| AGENTS.md | Updated status, policy, links, or acceptance guidance |
| ARCHITECTURE.md | Updated status, policy, links, or acceptance guidance |
| CHANGELOG.md | Updated status, policy, links, or acceptance guidance |
| CONTRIBUTING.md | Updated status, policy, links, or acceptance guidance |
| DECISIONS.md | Updated status, policy, links, or acceptance guidance |
| DEPENDENCIES.md | Updated status, policy, links, or acceptance guidance |
| ENGINEERING.md | Updated status, policy, links, or acceptance guidance |
| OPERATIONS.md | Updated status, policy, links, or acceptance guidance |
| OUI_DATA.md | Updated status, policy, links, or acceptance guidance |
| PACKAGING.md | Updated status, policy, links, or acceptance guidance |
| PERFORMANCE.md | Updated status, policy, links, or acceptance guidance |
| README.md | Updated status, policy, links, or acceptance guidance |
| RELEASE_CHECKLIST.md | Updated status, policy, links, or acceptance guidance |
| ROADMAP.md | Updated status, policy, links, or acceptance guidance |
| SECURITY.md | Updated status, policy, links, or acceptance guidance |
| TESTING.md | Updated status, policy, links, or acceptance guidance |
| UI_APPEARANCE.md | Updated status, policy, links, or acceptance guidance |
| UI_WORKSPACE.md | Updated status, policy, links, or acceptance guidance |
| AUDIT.md | Исторический аудит PR #11, оставить неизменным. |
| CODE_OF_CONDUCT.md | Нормативный текст, не зависит от версии приложения. |
| DEVICE_DESCRIPTIONS.md | Проектирование будущего импорта, не утверждать реализацию. |
| INDUSTRIAL_PROTOCOLS.md | Справочник и backlog; C4/D не добавляют протоколы. |
| INDUSTRY_TRENDS_2026.md | Датированный исследовательский обзор, не повторная верификация внешних стандартов. |
| THIRD_PARTY_NOTICES.md | Лицензионные уведомления сохраняются; зависимости не изменены. |
| UI_DESIGN_SYSTEM.md | Действующие ограничения компонентной системы; редизайн не входит в D. |

## Architecture boundaries (targeted source inspection)

| Boundary | Evidence | Assessment |
|---|---|---|
| ResultsStore | src/results/store.rs: private rows, generation/revision, cap 100000, read-only rows, index_of | Single owner; stale IDs rejected |
| ResultsProjection | src/results/projection.rs: cache keys, metadata revision, private indices/groups/rows | No egui/App imports; settings editable, derived data private |
| SelectionState | src/results/selection.rs: None/Host/Endpoint, validated transitions | No egui/App imports; host independent from port |
| App | src/app.rs: event drain max 1024; clear resets session and selection; UI reads accessors | Coordinator; no old two-field selection in inspected paths |
| UI | src/ui/results.rs / workspace.rs: bounded virtualized rows, Inspector uses selected host | Some visible rows cloned for rendering; no full-store UI clone observed |
| Export | src/app.rs export_to_path: snapshot and metadata clone, then worker | Explicit full snapshot on UI thread; measure cost (TD-05), no hidden live second store |
| UDP | src/scanner/udp.rs, probe.rs, mod.rs: silence OpenFiltered, connected socket errors, generic response | Response confirms endpoint response, not application protocol. service_verified=true for any reply is a candidate semantic risk TD-09, manual D-08 required |

## Acceptance and supply chain

- Five required CI jobs all success on exact preview SHA; reports preserved at [baseline](baselines/v0.5.0-eng-01/README.md). LIVE-04/11 diagnostic excursions recorded; no threshold changed.
- [Release](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/preview-windows-v0.5.0-1) is a prerelease; stable v0.4.0 retained. GitHub asset digest and manifest existence confirmed, downloaded archive not independently hashed or executed in this audit.
- Rulesets GET returned []; branch protection GET returned 403; available connection has no administration write action. Existing template [main-quality-gate.json](../.github/rulesets/main-quality-gate.json) requires PR, strict status check Quality Gate, no forced update/deletion, zero independent reviewer requirement. No active enforcement claimed. Owner must import/check Settings → Rules → Rulesets; choose main target, active enforcement, no bypass, GitHub Actions Quality Gate source if selectable, verify resulting rule and PR blocked on red. Do not deliberately break main. Existing classic protection may need owner inspection first. No branch policy weakened.
- Open issues and PRs at inspected source: none from API. This is not evidence that P0/P1 are absent; manual acceptance pending. Known debt listed in [TECHNICAL_DEBT.md](../TECHNICAL_DEBT.md); UDP evidence needs D-08 before closure.
- [D-01–D-20](acceptance/windows-v0.5.0-preview-1.md) NOT RUN. W-01–W-15 remain traceable. Cross-platform acceptance deferred to stable release under DEC-020; no new live timing gate.

## D4 exit conditions

Record manual evidence with exact release/SHA, issue disposition and accepted exceptions; verify required Quality Gate enforcement; no open P0/P1; record acceptance decision. Do not imply tests executed from this preparation PR. Return to C4.3 only after D.
