# UI-03 — Engineering Workspace

## Approved decisions (2026-11-26)

- Variant C: Engineering Workbench, narrow vertical icon navigation, contextual left panel, central results, right device inspector, top global area and bottom status.
- Devices is the default view, Ports is an alternative; switchable Russian and English UI.
- Only actual functions are exposed. Future history, topology and protocol adapters are not empty navigation entries.
- Preserve UI-01 appearance settings and UI-02 shared components. No new GUI framework or dependencies.

## UI-03.1 scope

The existing IP-grouped results are the transitional Devices view, not a verified physical Asset or persistent inventory. Native startup selects Devices with Discovery controls visible. Discovery displays existing scan settings, Devices expands results by hiding the context panel, Settings displays the existing persistent appearance controls. Returning to Discovery retains inputs and scan state. Ports remains the flat result table. The internal App::default_state keeps the preexisting flat test baseline; App::new selects the agreed startup view.

The inspector reads the currently selected result and its cached hostname, MAC and OUI owner, if present. It reports absent/ambiguous observations and never infers device model from port number. Full card tabs, physical identity, evidence, components and history are UI-03.6 and subsequent functional stages.

At 1100 logical points or wider, the inspector is a right panel; below it, the same selected-host details are available in a collapsible region above the center results. Left panel resizing remains supported. More comprehensive docking and layout persistence belong to UI-04/UI-07.

RU/EN currently covers shell labels, not existing scanner controls, diagnostic messages, or exports; language selection is session-only until localization/preferences work is completed. English is the initial fallback. Icon glyph visibility and actual layout at Windows DPI settings require manual review.

## Acceptance

- CI: formatting, Markdown links, Clippy, MSRV, tests and optimized builds on Windows/Linux/macOS.
- Headless tests: initial view, navigation preserves input/results/selection, rendering at 880, 1180 and 1920 points in both shell languages.
- Manual Windows acceptance: start and stop scans, select Devices/Ports, change appearance from Settings, return to Discovery, verify filter/sort/keyboard shortcuts and export. Check selection, narrow inspector, language labels and icon rendering in both themes and at 100–200% DPI.
- Existing grouped view is not virtualized for very large datasets; Ports retains the optimized table. Device-view virtualization is UI-03.5. No new network I/O or identities are introduced by the shell.
- UI-03.1 is not a finished redesign. Global action toolbar, full localization, full inspector and status detail follow the agreed stage sequence.


## UI-03.2 — Navigation

Six functional routes; Discovery at startup, grouped results by default. Navigation only changes route, never scanner state, view mode, filter or selection. Compact/expanded original line icons use egui painter. Non-result routes suppress result-specific keyboard commands; stop remains available while scanning. Full global toolbar follows UI-03.3. Language preference stored separately in workspace-language.json (schema 1), restored at startup; existing appearance.json unaffected. Shell and new route labels bilingual; full legacy translation follows UI-03.7. Expansion is session-only. Knowledge contains current OUI functions (disabled during active scan, as in Discovery) and the offline [industrial protocol catalog](INDUSTRIAL_PROTOCOLS.md). No network activity on route change.


## UI-03.3 — Global Toolbar

Approved two-level global toolbar and typed AppCommand dispatch. Start/Stop, profile load/save, CSV/JSON/HTML export, Settings and theme toggle share availability checks; toolbar and keyboard handlers use the same dispatch. Reports retains export shortcuts through dispatch. Discovery no longer duplicates these commands. Start requires valid input and no active scan; Stop requires active non-cancelled scan; profile commands are disabled while scanning or stopping; exports require results and no active export. Context labels distinguish active input snapshot from the next draft. Bind IP is the configured source address, not a guessed NIC name. Active scan status comes from the scan handle, not the status of an unrelated export. Ctrl/Cmd+O and Ctrl/Cmd+S respect text-field focus; other existing shortcuts remain. Quick-action changes of scan targets are disabled during active scans. No Pause/Resume, new network methods or changes to profile/export formats. Full localization and custom shortcuts remain later stages.

Acceptance: command guards, immutable snapshot, navigation and live result streaming, existing keyboard tests, all-platform CI; manually check toolbar layouts, file dialogs, profile round trip, export during active scan, and theme/palette preservation on Windows. The existing industrial knowledge catalog stays offline and unchanged (52 records, research backlog in [INDUSTRIAL_PROTOCOLS.md](INDUSTRIAL_PROTOCOLS.md)).


## UI-03.4 — Discovery Panel

General preserves existing presets; Industrial selects manufacturer, exact catalog family or protocol group and previews transport-specific port candidates; Custom exposes the editable port list. Switching modes, family or transport does not automatically replace ports or start a scan. Explicit Replace/Append actions apply the candidate list; no implicit protocol switching or new adapters. Loading JSON v1 selects Custom while preserving profile data; mode/category are session-only, not part of the profile format. Advanced settings are collapsible; source-IP selector, targets, exclusions, host enrichment and tuning remain supported. OUI and services management move to Knowledge Base, disabled during active scans.

Existing InputCache/ScopePreview is the only scope validator. Preview shows planned targets, names, excluded IPs, ports and probes separately from actual last/active counters. Timeout-only estimate is illustrative, not a guarantee; DNS, banners and overhead excluded. Active scan disables editing as before. The Knowledge Service shares the bundled catalog and read-only source summary; scanner behavior remains unchanged. Explicit reviewed allowlist excludes uncertain, configurable, L2, cyclic IO, trap destinations and ambiguous endpoints; UDP candidates use the existing generic probe, not a verified protocol adapter. See [INDUSTRIAL_PROTOCOLS.md](INDUSTRIAL_PROTOCOLS.md).

Version 0.5.0 is prepared in Cargo.toml and Cargo.lock, not tagged or published yet. Test the Windows CI artifact after green checks; public release requires existing exact SemVer tag workflow and manual acceptance.


## UI-03.5 — Results Workspace

Virtualized fixed-height host/endpoint rows, initially collapsed; separate host sort and existing port sort, horizontal scroll for narrow windows, AND filters, explicit reset, matching host and endpoint counts. Selected host/endpoint shares the existing stable result index; host-only selection suppresses endpoint actions. Expansion cached by IP; raw results and export remain unchanged. State/evidence labels distinguish UDP silence from Open and service verification from identity. OUI owner is not necessarily device manufacturer. RTT plot collapsed by default; Diagnostics still available. Density deferred to UI-05. Headless 100k endpoint/10k host stress checks bounded draw output and cache stability; existing 20k latency budget retained. Manual Windows DPI and UI acceptance required.
