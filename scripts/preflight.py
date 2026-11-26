#!/usr/bin/env python3
"""Cross-platform preflight. Run from any directory; stop on the first failure."""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def pinned_channel() -> str:
    text = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")
    match = re.search(r'^channel\s*=\s*"(\d+\.\d+\.\d+)"\s*$', text, re.MULTILINE)
    if match is None:
        raise ValueError("rust-toolchain.toml must declare an exact Rust version")
    return match.group(1)


def commands(channel: str, format_only: bool = False) -> list[list[str]]:
    cargo = ["cargo", f"+{channel}"]
    checks = [
        [*cargo, "fmt", "--all", "--check"],
        [sys.executable, "scripts/check_docs.py"],
    ]
    if not format_only:
        checks += [
            [*cargo, "check", "--locked", "--all-targets"],
            [*cargo, "clippy", "--locked", "--all-targets", "--", "-D", "warnings"],
            [*cargo, "test", "--locked", "--all-targets"],
            [*cargo, "build", "--locked", "--release"],
        ]
    return checks


def execute(checks: list[list[str]]) -> int:
    for command in checks:
        print("+ " + " ".join(command), flush=True)
        try:
            subprocess.run(command, cwd=ROOT, check=True)
        except subprocess.CalledProcessError as error:
            print(f"Preflight failed (exit {error.returncode})", file=sys.stderr)
            return error.returncode if error.returncode > 0 else 1
        except OSError as error:
            print(f"Cannot start command: {error}", file=sys.stderr)
            return 1
    print("Preflight passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format-only", action="store_true", help="Only Rustfmt and local Markdown links")
    args = parser.parse_args()
    try:
        channel = pinned_channel()
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    return execute(commands(channel, args.format_only))


if __name__ == "__main__":
    sys.exit(main())
