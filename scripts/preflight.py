#!/usr/bin/env python3
"""Cross-platform preflight. Run from any directory; stop on the first failure."""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PERFORMANCE_TESTS = (
    "app::tests::stress_benchmark_events_sorting_and_rendering",
    "app::tests::virtualized_device_rows_at_storage_limit",
    "app::tests::performance_matrix_optimized",
)


def test_commands(channel: str) -> list[list[str]]:
    cargo = ["cargo", f"+{channel}", "test", "--locked"]
    functional = [*cargo, "--all-targets", "--"]
    for name in PERFORMANCE_TESTS:
        functional.extend(["--skip", name])
    isolated = [
        [*cargo, "--bin", "ip_port_scanner", name, "--", "--exact", "--test-threads=1", "--nocapture"]
        for name in PERFORMANCE_TESTS[:2]
    ]
    return [functional, *isolated]


def verify_performance_tests(channel: str) -> bool:
    """Fail closed if a renamed or missing test would result in zero tests."""
    command = ["cargo", f"+{channel}", "test", "--locked", "--bin", "ip_port_scanner", "--", "--list"]
    try:
        result = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True)
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"Cannot enumerate performance tests: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        return False
    listed = set(result.stdout.splitlines())
    missing = [name for name in PERFORMANCE_TESTS if f"{name}: test" not in listed]
    if missing:
        print(f"Missing required performance tests: {missing}", file=sys.stderr)
        return False
    return True


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
            *test_commands(channel),
            [sys.executable, "scripts/performance.py"],
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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--format-only", action="store_true", help="Only Rustfmt and local Markdown links")
    mode.add_argument("--tests-only", action="store_true", help="Functional tests and isolated performance tests")
    args = parser.parse_args()
    try:
        channel = pinned_channel()
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    if args.format_only:
        return execute(commands(channel, True))
    if args.tests_only:
        return execute(test_commands(channel)) if verify_performance_tests(channel) else 1
    # Preserve the preflight order: format, docs, check, lint, test, release.
    checks = commands(channel)
    result = execute(checks[:4])
    if result:
        return result
    if not verify_performance_tests(channel):
        return 1
    return execute(checks[4:])


if __name__ == "__main__":
    sys.exit(main())
