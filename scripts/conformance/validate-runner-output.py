#!/usr/bin/env python3
"""Validate one canonical conformance runner observation and emit its summary."""

import argparse
import json
import sys

from lib.results import require_complete_runner_summary


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("runner_output")
    parser.add_argument("--runner-status", required=True, type=int)
    parser.add_argument("--output")
    args = parser.parse_args()
    try:
        summary = require_complete_runner_summary(
            args.runner_output, runner_status=args.runner_status
        )
    except (OSError, ValueError) as error:
        print(f"invalid conformance runner observation: {error}", file=sys.stderr)
        return 1
    payload = json.dumps(summary, sort_keys=True)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as output:
            output.write(payload)
            output.write("\n")
    else:
        print(payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
