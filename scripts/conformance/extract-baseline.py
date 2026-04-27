#!/usr/bin/env python3
"""Extract per-test baseline from raw conformance runner output.

Collapses multi-line runner output into one line per test:
  PASS path
  FAIL path | expected:[TS2322,TS2345] actual:[TS2322]
  XFAIL path | expected:[TS2322,TS2345] actual:[TS2322]

Output is sorted by test path for stable diffing.
"""

import sys
import os
import re

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.results import parse_runner_output


# Always write paths repo-relative so the baseline is portable across
# checkout dirs (CI runners, worktrees, contributors' workspaces).
_ABS_PREFIX_RE = re.compile(r"^.*?(TypeScript/)")


def normalize_path(path):
    return _ABS_PREFIX_RE.sub(r"\1", path, count=1)


def extract(input_path):
    tests = parse_runner_output(input_path)
    results = []
    for path, rec in tests.items():
        path = normalize_path(path)
        status = rec["status"]
        exp = rec["expected"]
        act = rec["actual"]
        if status == "PASS":
            results.append("PASS " + path)
        elif status in ("FAIL", "XFAIL"):
            if exp or act:
                results.append(
                    f'{status} {path} | expected:[{",".join(exp)}] actual:[{",".join(act)}]'
                )
            else:
                results.append(f"{status} {path}")

    for r in sorted(results):
        print(r)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <runner-output-file>", file=sys.stderr)
        sys.exit(1)
    extract(sys.argv[1])
