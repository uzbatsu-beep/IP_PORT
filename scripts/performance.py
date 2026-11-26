#!/usr/bin/env python3
"""Run optimized headless performance QA and validate its report, fail closed."""
from __future__ import annotations

import json
import math
import os
import subprocess
import sys
from pathlib import Path

from preflight import ROOT, pinned_channel, PERFORMANCE_TESTS

COUNTS = (1000, 20000, 50000, 100000)
VIEWS = ("devices", "ports")


def validate(report: dict) -> None:
    if report.get("schema_version") != 1 or report.get("profile") != "release":
        raise ValueError("Expected schema 1 optimized release report")
    scenarios = report["scenarios"]
    expected = {(count, view) for count in COUNTS for view in VIEWS}
    if len(scenarios) != len(expected) or {(s["endpoints"], s["view"]) for s in scenarios} != expected:
        raise ValueError("Missing or duplicate performance scenarios")
    for s in scenarios:
        frames = s["frame_ms"]
        warmup = s["warmup_ms"]
        if len(frames) != 60 or len(warmup) != 5:
            raise ValueError("Incorrect frame sample count")
        if not all(isinstance(v, (int, float)) and math.isfinite(v) and v >= 0 for v in frames + warmup):
            raise ValueError("Invalid frame samples")
        if s["hosts"] != s["endpoints"] // 10 or not 0 <= s["max_shapes"] < 5000:
            raise ValueError("Invalid host count or unbounded draw output")
        required = s["endpoints"] == 20000
        if s["timing_gate_required"] is not required:
            raise ValueError("Invalid gate designation")
        target = sum(frames) / 60 < 16.0 and sorted(frames)[57] < 25.0
        if s["within_target"] is not target or (required and not target):
            raise ValueError("Performance timing gate failed")
    if report.get("gate_passed") is not True:
        raise ValueError("Performance test reported failure")


def main() -> int:
    path = ROOT / "target" / "performance-report.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    live_path = ROOT / "target" / "live-performance-report.json"
    live_path.unlink(missing_ok=True)
    command = ["cargo", f"+{pinned_channel()}", "test", "--locked", "--release",
               "--bin", "ip_port_scanner"]
    exact = ["--", "--exact", "--test-threads=1", "--nocapture"]
    env = dict(os.environ, IP_PORT_PERF_REPORT=str(path), IP_PORT_LIVE_REPORT=str(live_path))
    try:
        subprocess.run([*command, PERFORMANCE_TESTS[-2], *exact], cwd=ROOT, env=env, check=True)
        validate(json.loads(path.read_text(encoding="utf-8")))
        subprocess.run([*command, PERFORMANCE_TESTS[-1], *exact], cwd=ROOT, env=env, check=True)
        from live_performance import validate_live
        validate_live(json.loads(live_path.read_text(encoding="utf-8")))
    except (OSError, subprocess.CalledProcessError, ValueError, KeyError, TypeError) as error:
        print(f"Performance QA failed: {error}", file=sys.stderr)
        return 1
    print(f"Performance QA passed: {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
