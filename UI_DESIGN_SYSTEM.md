# UI-02 — Shared design system

## Scope

Centralize common egui components before restructuring the workspace. Keep application actions, scan state, data models and existing navigation in their owning views. No new dependencies.

## Component catalog

| Component | Purpose |
| --- | --- |
| primary_button | Accent treatment for Start scan |
| danger_button | Semantic danger treatment for enabled Stop |
| secondary_button | Neutral action, e.g. Clear |
| status_text / semantic_text | Shared tone mapping for scan status and verification |
| surface | Uniform group background, padding and radius |
| section_heading / section_divider | Shared panel hierarchy |
| empty_state | Consistent empty/filter/scan feedback |
| metric_row | Label and aligned monospace numeric value |
| TABLE_ROW_HEIGHT | Shared result table row size |
| rtt_color | Theme-aware severity colors, independent of selected palette |

Use [UI appearance](UI_APPEARANCE.md) as the source of theme tokens. Maintain explicit labels for semantic states. RTT shades change only for readability, not bucket membership or statistics. The existing HTML export has its own presentation and is not redesigned here.

## Acceptance

CI: formatting, Markdown links, Clippy, unit tests, MSRV and builds for three operating systems. New headless tests render components and populated flat/grouped views in every theme/palette combination; this is not pixel-level visual validation. Manual Windows check: compare Start/Stop, status indicators, disabled Clear, RTT chart, inspector, table selection and empty state in both themes; confirm unchanged scanning and export behavior. Check 880×560 and 125–200% DPI. UI-03 may change layout only after review of this foundation.
