# UI-01 — Appearance foundation

## Usage

Expand Appearance above the scanner controls. Choose Light/Dark and Classic Blue, Industrial Slate, Teal, Emerald, Violet, Amber, Monochrome or Indigo. The settings apply independently and do not restart scans. Reset restores Dark / Classic Blue. Retry save explicitly writes the current selection if the previous operation failed.

## Persistence

Version 1 JSON: `{"schema_version":1,"appearance":{"theme":"dark","palette":"blue"}}`.

- Windows: `%LOCALAPPDATA%\IP_PORT\appearance.json` (APPDATA fallback).
- Linux: `$XDG_DATA_HOME/ip_port/appearance.json` (fallback `~/.local/share/ip_port/appearance.json`).
- macOS: `~/Library/Application Support/IP_PORT/appearance.json`.

Missing file uses defaults; malformed/unsupported/oversized data uses defaults and reports the problem. Writes stage a unique temporary file in the same directory, sync it and rename it over the previous file. The old configuration is preserved if replacement fails. Concurrent instances use last successful writer wins. No scan targets or credentials are stored. A crash before the rename may leave a temporary file; a power-loss durability guarantee is not claimed.

## Design constraints

Centralize appearance in src/ui/theme.rs. Accent affects selection, links and active controls, while success, warning, error and RTT severity keep their meaning independent of palette. Theme-specific semantic shades maintain legibility. Existing layout and scan logic remain unchanged.

## Acceptance

Automated tests cover all 16 combinations, contrast, semantic color stability, persistence overwrite, invalid JSON and unsupported schema. Existing CI runs formatting, Clippy, tests and builds on Windows/Linux/macOS.

Manual Windows acceptance: switch all palettes in each theme, inspect sidebar, results, status, RTT visualization and toast, switch while a scan is active, close/reopen, launch from a different working directory, test reset, and verify recovery from a malformed settings file. Verify failed saves are visible and retry succeeds after write access is restored. Windows visual acceptance is required before publishing a release.
