#!/usr/bin/env python3
"""Report or verify the clean-slate rewrite architecture ratchet."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

import arch_guard


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="repository root",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail unless every metric equals the committed no-growth baseline",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    root = args.root.resolve()
    metrics = arch_guard.rewrite_architecture_metrics(root)
    print(json.dumps(metrics, indent=2, sort_keys=True))
    if not args.check:
        return 0
    violations = arch_guard.check_rewrite_architecture_ratchet(root)
    for violation in violations:
        print(violation.render(), file=sys.stderr)
    if violations:
        return 1
    print("rewrite architecture ratchet: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
