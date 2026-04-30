#!/usr/bin/env python3
"""Quick random failure picker.

Reads scripts/conformance/conformance-detail.json and prints one random
failing conformance test. Optional flags:
  --seed N      reproducible pick
  --code CODE   only consider failures touching CODE (expected/actual/missing/extra)
  --run         after printing, exec the verbose conformance runner on the pick

This is a thin, self-contained alternative to scripts/session/quick-pick.sh
for agents that just want one failing test in a single shell-out.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve()
REPO = Path(
    subprocess.check_output(
        ["git", "-C", str(SCRIPT.parent), "rev-parse", "--show-toplevel"], text=True
    ).strip()
)
DETAIL = REPO / "scripts" / "conformance" / "conformance-detail.json"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=None)
    parser.add_argument("--code", default=None)
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()

    if not DETAIL.is_file():
        sys.exit(
            f"error: {DETAIL} missing.\n"
            "  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot"
        )

    failures = json.loads(DETAIL.read_text("utf-8")).get("failures", {})
    items = []
    for path, entry in failures.items():
        codes = set(entry.get("e", [])) | set(entry.get("a", [])) | set(entry.get("m", [])) | set(entry.get("x", []))
        if args.code and args.code not in codes:
            continue
        items.append((path, entry))

    if not items:
        sys.exit("no matching failures")

    rng = random.Random(args.seed) if args.seed is not None else random.Random()
    path, entry = rng.choice(items)
    name = Path(path).stem
    fmt = lambda xs: ",".join(xs) or "-"

    print(f"path:     {path}")
    print(f"expected: {fmt(entry.get('e', []))}")
    print(f"actual:   {fmt(entry.get('a', []))}")
    print(f"missing:  {fmt(entry.get('m', []))}")
    print(f"extra:    {fmt(entry.get('x', []))}")
    print(f"pool:     {len(items)}")
    print()
    print(f'verbose run: ./scripts/conformance/conformance.sh run --filter "{name}" --verbose')

    if args.run:
        runner = REPO / "scripts" / "conformance" / "conformance.sh"
        os.execv(str(runner), [str(runner), "run", "--filter", name, "--verbose"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
