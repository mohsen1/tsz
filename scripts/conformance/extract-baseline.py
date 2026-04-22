#!/usr/bin/env python3
"""Extract per-test baseline from raw conformance runner output.

Collapses multi-line runner output into one line per test:
  PASS path
  FAIL path | expected:[TS2322,TS2345] actual:[TS2322]
  XFAIL path | expected:[TS2322,TS2345] actual:[TS2322]

Output is sorted by test path for stable diffing.
"""

import sys
import re


def extract(input_path):
    lines = open(input_path).readlines()
    results = []
    i = 0
    while i < len(lines):
        line = lines[i].rstrip()

        m = re.match(r"^PASS\s+(.+)$", line)
        if m:
            results.append("PASS " + m.group(1))
            i += 1
            continue

        m = re.match(r"^(FAIL|XFAIL)\s+(.+?)(?:\s+\(.+\))?$", line)
        if m:
            status = m.group(1)
            path = m.group(2)
            exp, act = [], []
            j = i + 1
            while j < len(lines) and lines[j].startswith("  "):
                em = re.match(r"^\s+expected:\s+\[(.*?)\]", lines[j])
                if em:
                    exp = [c.strip() for c in em.group(1).split(",") if c.strip()]
                am = re.match(r"^\s+actual:\s+\[(.*?)\]", lines[j])
                if am:
                    act = [c.strip() for c in am.group(1).split(",") if c.strip()]
                j += 1
            if exp or act:
                results.append(
                    f'{status} {path} | expected:[{",".join(exp)}] actual:[{",".join(act)}]'
                )
            else:
                results.append(f"{status} {path}")
            i = j
            continue

        i += 1

    for r in sorted(results):
        print(r)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <runner-output-file>", file=sys.stderr)
        sys.exit(1)
    extract(sys.argv[1])
