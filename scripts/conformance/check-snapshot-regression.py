#!/usr/bin/env python3
"""Fail when checked-in conformance snapshots regress against a base ref."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass
from typing import Any


SNAPSHOT_PATH = "scripts/conformance/conformance-snapshot.json"
DETAIL_PATH = "scripts/conformance/conformance-detail.json"
CATEGORY_KEYS = (
    "false_positive",
    "all_missing",
    "wrong_code",
    "fingerprint_only",
    "same_code_count_drift",
    "close_to_passing",
)


@dataclass(frozen=True)
class ConformanceSnapshot:
    passed: int
    total: int
    failures: dict[str, dict[str, Any]]
    categories: dict[str, int]


@dataclass(frozen=True)
class SnapshotComparison:
    base: ConformanceSnapshot
    head: ConformanceSnapshot
    fixed_failures: list[str]
    new_failures: list[str]
    changed_failures: list[str]
    category_delta: dict[str, int]

    @property
    def pass_delta(self) -> int:
        return self.head.passed - self.base.passed

    def has_blocking_regression(self, allow_new_failures: bool = False) -> bool:
        if self.pass_delta < 0:
            return True
        if self.new_failures and not allow_new_failures:
            return True
        return False


def _read_ref_json(ref: str, path: str) -> dict[str, Any]:
    try:
        raw = subprocess.check_output(
            ["git", "show", f"{ref}:{path}"],
            text=True,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError as exc:
        message = exc.stderr.strip() or str(exc)
        raise SystemExit(f"failed to read {path} from {ref}: {message}") from exc

    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"failed to parse {path} from {ref}: {exc}") from exc


def _summary_passed(snapshot: dict[str, Any]) -> int:
    summary = snapshot.get("summary", {})
    return int(summary.get("passed", 0))


def _summary_total(snapshot: dict[str, Any]) -> int:
    summary = snapshot.get("summary", {})
    return int(summary.get("total_tests", summary.get("total", 0)))


def _categories(snapshot: dict[str, Any], detail: dict[str, Any]) -> dict[str, int]:
    raw = snapshot.get("categories") or detail.get("aggregates", {}).get("categories") or {}
    return {key: int(raw.get(key, 0)) for key in CATEGORY_KEYS}


def _failure_signature(entry: dict[str, Any]) -> str:
    relevant = {
        "e": entry.get("e", []),
        "a": entry.get("a", []),
        "m": entry.get("m", []),
        "x": entry.get("x", []),
        "status": entry.get("status"),
        "reason": entry.get("reason"),
    }
    return json.dumps(relevant, sort_keys=True, separators=(",", ":"))


def load_ref(ref: str) -> ConformanceSnapshot:
    snapshot = _read_ref_json(ref, SNAPSHOT_PATH)
    detail = _read_ref_json(ref, DETAIL_PATH)
    failures = detail.get("failures", {})
    if not isinstance(failures, dict):
        raise SystemExit(f"{DETAIL_PATH} from {ref} does not contain a failures object")

    return ConformanceSnapshot(
        passed=_summary_passed(snapshot),
        total=_summary_total(snapshot),
        failures=failures,
        categories=_categories(snapshot, detail),
    )


def compare_snapshots(
    base: ConformanceSnapshot,
    head: ConformanceSnapshot,
) -> SnapshotComparison:
    base_failures = set(base.failures)
    head_failures = set(head.failures)
    common_failures = base_failures & head_failures

    changed_failures = sorted(
        path
        for path in common_failures
        if _failure_signature(base.failures[path]) != _failure_signature(head.failures[path])
    )
    category_delta = {
        key: head.categories.get(key, 0) - base.categories.get(key, 0)
        for key in CATEGORY_KEYS
    }

    return SnapshotComparison(
        base=base,
        head=head,
        fixed_failures=sorted(base_failures - head_failures),
        new_failures=sorted(head_failures - base_failures),
        changed_failures=changed_failures,
        category_delta=category_delta,
    )


def _print_limited(title: str, values: list[str], limit: int = 25) -> None:
    print(f"{title}: {len(values)}")
    for value in values[:limit]:
        print(f"  - {value}")
    if len(values) > limit:
        print(f"  ... and {len(values) - limit} more")


def print_report(comparison: SnapshotComparison) -> None:
    print("Conformance snapshot comparison:")
    print(
        f"- passed: {comparison.base.passed}/{comparison.base.total} -> "
        f"{comparison.head.passed}/{comparison.head.total} ({comparison.pass_delta:+d})"
    )
    _print_limited("- fixed failures", comparison.fixed_failures)
    _print_limited("- new failures", comparison.new_failures)
    _print_limited("- changed failures", comparison.changed_failures)
    category_text = ", ".join(
        f"{key} {delta:+d}" for key, delta in comparison.category_delta.items()
    )
    print(f"- category delta: {category_text}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Compare checked-in conformance snapshots against a base git ref."
    )
    parser.add_argument("--base-ref", required=True, help="Base git ref to compare against")
    parser.add_argument("--head-ref", default="HEAD", help="Head git ref to inspect")
    parser.add_argument(
        "--allow-new-failures",
        action="store_true",
        help="Do not fail when pass count is non-negative but new tests fail",
    )
    args = parser.parse_args(argv)

    comparison = compare_snapshots(load_ref(args.base_ref), load_ref(args.head_ref))
    print_report(comparison)

    if comparison.has_blocking_regression(args.allow_new_failures):
        if comparison.pass_delta < 0:
            print(
                f"::error::conformance snapshot pass count regressed "
                f"({comparison.base.passed} -> {comparison.head.passed})"
            )
        if comparison.new_failures and not args.allow_new_failures:
            print(
                f"::error::conformance snapshot introduced "
                f"{len(comparison.new_failures)} new failing test(s)"
            )
        return 1

    print("Conformance snapshot gate passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
