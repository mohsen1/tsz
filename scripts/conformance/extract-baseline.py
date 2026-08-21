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

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib.results import (
    normalize_harness_path,
    parse_runner_output,
    require_complete_runner_summary,
)


def extract(input_path):
    tests = parse_runner_output(input_path)
    if not tests:
        raise ValueError("runner output is incomplete; refusing baseline extraction")
    require_complete_runner_summary(input_path)
    results = []
    for path, rec in tests.items():
        path = normalize_harness_path(path)
        status = rec["status"]
        exp = rec["expected"]
        act = rec["actual"]
        if status in ("PASS", "SKIP", "UNSUPPORTED", "CRASH", "TIMEOUT"):
            suffix = ""
            if status == "UNSUPPORTED" and rec.get("unsupported_reason"):
                suffix = f" ({rec['unsupported_reason']})"
            results.append(f"{status} {path}{suffix}")
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
    try:
        extract(sys.argv[1])
    except (OSError, ValueError) as error:
        print(f"runner output is incomplete; refusing baseline extraction: {error}", file=sys.stderr)
        sys.exit(1)
