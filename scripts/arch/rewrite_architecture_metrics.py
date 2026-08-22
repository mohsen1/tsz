#!/usr/bin/env python3
"""Report or verify the clean-slate rewrite architecture ratchet."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
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
    parser.add_argument(
        "--base-ref",
        help=(
            "also reject any committed metric that increased relative to this "
            "merge-base commit"
        ),
    )
    return parser.parse_args(argv)


def validate_baseline(value: object, source: str) -> dict[str, int]:
    if not isinstance(value, dict):
        raise ValueError(f"{source} must contain a JSON object")
    invalid = sorted(
        name
        for name, measured in value.items()
        if not isinstance(name, str) or type(measured) is not int or measured < 0
    )
    if invalid:
        raise ValueError(
            f"{source} has non-negative-integer metric violations: {invalid!r}"
        )
    return value


def load_current_baseline(root: Path) -> dict[str, int]:
    path = root / arch_guard.ARCHITECTURE_RATCHET_PATH
    try:
        return validate_baseline(json.loads(path.read_text(encoding="utf-8")), str(path))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error


def run_git(root: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError(f"git {' '.join(arguments)!r} failed: {detail}")
    return result.stdout


def path_exists_at_ref(root: Path, revision: str, relative: str) -> bool:
    listing = run_git(root, "ls-tree", "--name-only", revision, "--", relative)
    return relative in listing.splitlines()


def is_ancestor(root: Path, ancestor: str, descendant: str) -> bool:
    result = subprocess.run(
        ["git", "-C", str(root), "merge-base", "--is-ancestor", ancestor, descendant],
        capture_output=True,
        text=True,
    )
    if result.returncode not in (0, 1):
        detail = result.stderr.strip() or result.stdout.strip()
        raise ValueError(f"cannot inspect git ancestry: {detail}")
    return result.returncode == 0


def relevant_history_is_shallow(root: Path, head: str, base: str) -> bool:
    if run_git(root, "rev-parse", "--is-shallow-repository").strip() == "false":
        return False
    shallow_path = Path(run_git(root, "rev-parse", "--git-path", "shallow").strip())
    if not shallow_path.is_absolute():
        shallow_path = root / shallow_path
    try:
        boundaries = shallow_path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise ValueError(f"cannot read git shallow boundaries: {error}") from error
    return any(
        is_ancestor(root, boundary, tip)
        for boundary in boundaries
        for tip in (head, base)
    )


def resolve_base_commit(root: Path, base_ref: str) -> str:
    head = run_git(root, "rev-parse", "--verify", "HEAD^{commit}").strip()
    base = run_git(root, "rev-parse", "--verify", f"{base_ref}^{{commit}}").strip()
    if relevant_history_is_shallow(root, head, base):
        raise ValueError(
            "architecture merge-base direction requires complete HEAD/base history"
        )
    if base == head:
        raise ValueError("architecture ratchet base must precede HEAD")
    merge_bases = run_git(root, "merge-base", "--all", "HEAD", base).splitlines()
    if merge_bases != [base]:
        raise ValueError(
            f"architecture ratchet base {base_ref!r} is not HEAD's unique merge base"
        )
    return base


def load_baseline_at_commit(root: Path, revision: str) -> dict[str, int]:
    """Read and validate one committed architecture-ratchet snapshot."""

    ratchet = arch_guard.ARCHITECTURE_RATCHET_PATH
    shown = run_git(root, "show", f"{revision}:{ratchet}")
    try:
        value = json.loads(shown)
    except json.JSONDecodeError as error:
        raise ValueError(
            f"architecture ratchet at {revision!r} is invalid JSON: {error}"
        ) from error
    return validate_baseline(value, f"architecture ratchet at {revision!r}")


def load_baseline_at_ref(root: Path, base_ref: str) -> dict[str, int] | None:
    """Return the strictest committed floor reachable from the checked HEAD.

    The trusted base is still resolved fail-closed so CI cannot compare against
    an unrelated revision or truncated relevant history. The floor itself is
    historical: once a metric has been committed, deleting/recreating the
    files, advancing the PR base, or merging parallel introductions cannot
    reset or raise it.
    """

    resolve_base_commit(root, base_ref)
    ratchet = arch_guard.ARCHITECTURE_RATCHET_PATH
    checker = "scripts/arch/rewrite_architecture_metrics.py"
    head = run_git(root, "rev-parse", "--verify", "HEAD^{commit}").strip()
    candidates = run_git(
        root,
        "rev-list",
        "--full-history",
        "--reverse",
        "--topo-order",
        head,
        "--",
        ratchet,
        checker,
    ).splitlines()

    snapshots = []
    for candidate in candidates:
        if path_exists_at_ref(root, candidate, ratchet) and path_exists_at_ref(
            root, candidate, checker
        ):
            snapshots.append(load_baseline_at_commit(root, candidate))

    if not snapshots:
        if (root / ratchet).is_file() and (root / checker).is_file():
            # The only genuine bootstrap is the first, still-uncommitted
            # introduction. A committed CI checkout always has a snapshot.
            return None
        raise ValueError("established architecture ratchet is missing from HEAD history")

    floor: dict[str, int] = {}
    for snapshot in snapshots:
        for name, value in snapshot.items():
            floor[name] = min(floor.get(name, value), value)
    return floor


def direction_violations(
    base: dict[str, int], current: dict[str, int]
) -> list[str]:
    """Reject debt growth or removal of a metric that existed at merge base."""

    violations = []
    for name in sorted(base):
        if name not in current:
            violations.append(f"architecture metric {name!r} was removed")
        elif current[name] > base[name]:
            violations.append(
                f"architecture metric {name!r} grew across the merge base: "
                f"base={base[name]}, current={current[name]}"
            )
    return violations


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.base_ref and not args.check:
        print("error: --base-ref requires --check", file=sys.stderr)
        return 2
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
    if args.base_ref:
        try:
            current = load_current_baseline(root)
            base = load_baseline_at_ref(root, args.base_ref)
        except ValueError as error:
            print(f"error: {error}", file=sys.stderr)
            return 1
        if base is None:
            print(
                "rewrite architecture merge-base direction: bootstrap "
                f"({arch_guard.ARCHITECTURE_RATCHET_PATH} absent at {args.base_ref})"
            )
        else:
            direction = direction_violations(base, current)
            for violation in direction:
                print(f"error: {violation}; lower or hold the baseline", file=sys.stderr)
            if direction:
                return 1
            print("rewrite architecture merge-base direction: pass")
    print("rewrite architecture ratchet: pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
