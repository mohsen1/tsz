#!/usr/bin/env python3
"""Validate the clean-slate README status contract.

The retired compiler's conformance, emit, fourslash, and benchmark artifacts
remain useful evidence, but they must never be promoted into current rewrite
claims. This script intentionally does not read CI metrics, benchmark snapshots,
or the network. It only keeps the R0 status block canonical and rejects the old
live-dashboard markers.

Usage:
    python3 scripts/refresh-readme.py          # validate README.md
    python3 scripts/refresh-readme.py --check  # same, explicit for CI
    python3 scripts/refresh-readme.py --write  # repair only the R0 status block
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
STATUS_START = "<!-- R0_STATUS_START -->"
STATUS_END = "<!-- R0_STATUS_END -->"
LEGACY_CHECKPOINT = "2770da88d4"

STATUS_BLOCK = """<!-- R0_STATUS_START -->
> [!WARNING]
> The rewrite is validation-only. There is no supported install, package
> release, WASM build, or drop-in replacement yet. Build it from source only
> when working on the compiler or its validation harnesses.

The fresh vertical slice currently proves exact seed behavior for:

- declarations, literal inference, and `let`/`var` widening;
- explicit annotations and assignment diagnostics;
- function calls, arguments, and return diagnostics;
- object properties and a bounded union subset;
- JavaScript emit for the seed syntax;
- deterministic diagnostics across repeated runs and reversed root-file order.

Exact seed assertions cover diagnostic codes, spans, messages, ordering, exit
status, and emitted bytes against TypeScript `7.0.2`.
<!-- R0_STATUS_END -->"""

RETIRED_LIVE_MARKERS = (
    "<!-- PERFORMANCE_START -->",
    "<!-- PERFORMANCE_END -->",
    "<!-- CONFORMANCE_START -->",
    "<!-- CONFORMANCE_END -->",
    "<!-- EMIT_START -->",
    "<!-- EMIT_END -->",
    "<!-- FOURSLASH_START -->",
    "<!-- FOURSLASH_END -->",
)

REQUIRED_CONTEXT = (
    "Current status: R0",
    "TypeScript `7.0.2`",
    "Frozen legacy checkpoint",
    LEGACY_CHECKPOINT,
    "11,667 / 12,043 runnable cases (96.9%)",
    "eventual goal is exact TypeScript compatibility and at least 3x",
    "There is no supported install, package",
)


def replace_status_block(text: str) -> str:
    """Return *text* with exactly one managed R0 block made canonical."""

    if text.count(STATUS_START) != 1 or text.count(STATUS_END) != 1:
        raise ValueError("README must contain exactly one complete R0 status block")
    start = text.index(STATUS_START)
    end = text.index(STATUS_END, start) + len(STATUS_END)
    return f"{text[:start]}{STATUS_BLOCK}{text[end:]}"


def validation_errors(text: str) -> list[str]:
    """Report ways the README could misrepresent retired results as current."""

    errors: list[str] = []
    try:
        if replace_status_block(text) != text:
            errors.append("the managed R0 status block is not canonical")
    except ValueError as exc:
        errors.append(str(exc))

    for marker in RETIRED_LIVE_MARKERS:
        if marker in text:
            errors.append(f"retired live-dashboard marker remains: {marker}")

    for required in REQUIRED_CONTEXT:
        if required not in text:
            errors.append(f"required rewrite context is missing: {required!r}")

    return errors


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate or repair the README's clean-slate R0 status block.",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="validate without writing (default)")
    mode.add_argument("--write", action="store_true", help="repair the managed R0 block, then validate")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    original = README.read_text(encoding="utf-8")
    text = original

    if args.write:
        try:
            text = replace_status_block(text)
        except ValueError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 1
        if text != original:
            README.write_text(text, encoding="utf-8")
            print("updated README.md R0 status block")

    errors = validation_errors(text)
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1

    print("README.md clean-slate status contract is current")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
