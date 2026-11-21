#!/usr/bin/env python3
"""Check repository-local links in Markdown without accessing the network.

Use: python3 scripts/check_docs.py
Only local Markdown/file destinations are validated. External URLs, anchors,
mailto links, and fenced code samples are intentionally ignored.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
LINK_PATTERN = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
SCHEME_PATTERN = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


def local_targets(markdown: Path):
    in_fence = False
    fence_char = ""
    for number, line in enumerate(markdown.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.lstrip()
        if stripped.startswith(("```","~~~")):
            # Avoid confusing Markdown link syntax with example source code.
            prefix = stripped[:3]
            if not in_fence:
                in_fence, fence_char = True, prefix
            elif prefix == fence_char:
                in_fence = False
            continue
        if in_fence:
            continue
        for raw in LINK_PATTERN.findall(line):
            value = raw.strip().strip("<>")
            # Inline title: [text](path.md "title").
            value = re.split(r'\s+["\']', value, maxsplit=1)[0]
            if not value or value.startswith(("#", "//")) or SCHEME_PATTERN.match(value):
                continue
            value = unquote(value.split("#", 1)[0].split("?", 1)[0])
            if value:
                yield number, value


def check() -> int:
    errors = []
    count = 0
    for markdown in sorted(ROOT.rglob("*.md")):
        if any(p in {".git", "target", ".venv"} for p in markdown.relative_to(ROOT).parts):
            continue
        count += 1
        for line, destination in local_targets(markdown):
            target = (markdown.parent / destination).resolve()
            if not target.is_relative_to(ROOT) or not target.exists():
                errors.append(
                    f"{markdown.relative_to(ROOT)}:{line}: "
                    f"missing or escaping local path: {destination}"
                )
    for message in errors:
        print(f"ERROR: {message}", file=sys.stderr)
    if errors:
        print(f"Documentation check failed: {len(errors)} broken local links", file=sys.stderr)
        return 1
    print(f"Documentation check passed: {count} Markdown files; no broken local links")
    return 0


if __name__ == "__main__":
    sys.exit(check())
