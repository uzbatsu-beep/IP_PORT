#!/usr/bin/env python3
"""Require latest CI push run on main for the exact release commit."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from urllib.parse import urlencode
from urllib.request import Request, urlopen


def select_run(runs: list[dict], sha: str, event: str = "push") -> dict:
    exact = [r for r in runs if r.get("head_sha") == sha
             and r.get("head_branch") == "main" and r.get("event") == event]
    if not exact:
        raise ValueError("No CI push run on main for this exact SHA")
    run = max(exact, key=lambda r: (r.get("created_at", ""), r["id"]))
    if run.get("status") != "completed" or run.get("conclusion") != "success":
        raise ValueError("Latest exact-SHA main CI has not succeeded")
    return run


def require_gate(jobs: list[dict], name: str = "Quality Gate") -> None:
    gates = [j for j in jobs if j.get("name") == name]
    if len(gates) != 1 or gates[0].get("status") != "completed" or gates[0].get("conclusion") != "success":
        raise ValueError("Missing or unsuccessful Quality Gate")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--windows-preview", action="store_true",
                        help="Manual Windows-only prerelease; not the stable release gate")
    args = parser.parse_args()
    try:
        if args.windows_preview and (os.environ.get("GITHUB_REF") != "refs/heads/main"
                                     or os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch"):
            raise ValueError("Windows preview requires manual dispatch on main")
        sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        repo = os.environ["GITHUB_REPOSITORY"]
        if not re.fullmatch(r"[0-9a-f]{40}", sha) or not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
            raise ValueError("Invalid SHA or repository")
        token = os.environ["GH_TOKEN"]

        def get(path: str) -> dict:
            request = Request(f"https://api.github.com/repos/{repo}/{path}", headers={
                "Authorization": f"Bearer {token}", "Accept": "application/vnd.github+json",
                "User-Agent": "IP-PORT-release-gate", "X-GitHub-Api-Version": "2022-11-28",
            })
            with urlopen(request, timeout=30) as response:
                return json.load(response)

        query = urlencode({"head_sha": sha, "branch": "main", "event": "push", "per_page": 100})
        run = select_run(get("actions/workflows/ci.yml/runs?" + query)["workflow_runs"], sha)
        jobs = get(f"actions/runs/{run['id']}/jobs?filter=latest&per_page=100")
        if jobs["total_count"] > len(jobs["jobs"]):
            raise ValueError("Incomplete job listing")
        require_gate(jobs["jobs"])
        if args.windows_preview:
            require_gate(jobs["jobs"], "Test & Clippy (windows-latest)")
            print(f"Windows preview SHA {sha}: main CI {run['id']} and Windows QA passed")
            return 0
        cross_query = urlencode({"head_sha": sha, "branch": "main", "event": "workflow_dispatch", "per_page": 100})
        cross = select_run(get("actions/workflows/cross-platform.yml/runs?" + cross_query)["workflow_runs"], sha, "workflow_dispatch")
        cross_jobs = get(f"actions/runs/{cross['id']}/jobs?filter=latest&per_page=100")
        if cross_jobs["total_count"] > len(cross_jobs["jobs"]):
            raise ValueError("Incomplete cross-platform job listing")
        require_gate(cross_jobs["jobs"], "Cross-platform Gate")
        print(f"Release SHA {sha}: main CI {run['id']} and cross-platform acceptance {cross['id']} passed")
        return 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Release blocked: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
