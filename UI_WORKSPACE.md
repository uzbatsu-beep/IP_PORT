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
