#!/usr/bin/env python3
"""Shared conformance failure picker for scripts/session wrappers."""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent


@dataclass(frozen=True)
class Failure:
    path: str
    expected: list[str]
    actual: list[str]
    missing: list[str]
    extra: list[str]

    @property
    def filter_name(self) -> str:
        return Path(self.path).stem

    @property
    def category(self) -> str:
        if not self.expected and self.actual:
            return "false-positive"
        if self.expected and not self.actual:
            return "all-missing"
        if set(self.expected) == set(self.actual):
            return "fingerprint-only"
        if self.missing and not self.extra:
            return "only-missing"
        if self.extra and not self.missing:
            return "only-extra"
        return "wrong-code"

    @property
    def codes(self) -> set[str]:
        return set(self.expected) | set(self.actual) | set(self.missing) | set(self.extra)


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "-C", str(SCRIPT_DIR), "rev-parse", "--show-toplevel"],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return Path(result.stdout.strip())


def detail_path(root: Path) -> Path:
    return root / "scripts" / "conformance" / "conformance-detail.json"


def ensure_inputs(root: Path, *, ensure_submodule: bool) -> Path:
    if ensure_submodule and not (root / "TypeScript" / "tests").is_dir():
        init_typescript_submodule(root)

    detail = detail_path(root)
    if not detail.is_file():
        print(f"error: {detail} missing.", file=sys.stderr)
        print("  run: scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot", file=sys.stderr)
        sys.exit(1)
    return detail


def init_typescript_submodule(root: Path) -> None:
    """Initialize the TypeScript submodule, falling back to a full clone.

    The shallow `--depth 1` clone is fast but only works when the pinned
    commit is reachable from the default-branch tip. Pinned commits often
    drift behind upstream `main`, leaving the shallow clone with no way to
    fetch the recorded SHA. When that happens, fall back to a full
    (non-shallow) clone so contributors aren't left with a half-cloned
    submodule that subsequent commands silently treat as missing tests.
    """
    print("TypeScript submodule missing - initializing...", file=sys.stderr)
    shallow = subprocess.run(
        ["git", "-C", str(root), "submodule", "update", "--init", "--depth", "1", "TypeScript"],
        stderr=subprocess.PIPE,
        text=True,
    )
    if shallow.returncode == 0 and (root / "TypeScript" / "tests").is_dir():
        return

    if shallow.stderr:
        sys.stderr.write(shallow.stderr)
    print(
        "Shallow submodule clone did not produce a usable working tree; retrying with full clone...",
        file=sys.stderr,
    )
    # Drop partial state so the retry isn't poisoned by a half-fetched pack.
    subprocess.run(
        ["git", "-C", str(root), "submodule", "deinit", "-f", "--", "TypeScript"],
        check=False,
        stderr=subprocess.DEVNULL,
    )
    subprocess.run(
        ["git", "-C", str(root), "submodule", "update", "--init", "--recursive", "TypeScript"],
        check=True,
    )


def load_failures(detail: Path) -> list[Failure]:
    with detail.open(encoding="utf-8") as handle:
        raw_failures = json.load(handle).get("failures", {})

    failures: list[Failure] = []
    for path, entry in raw_failures.items():
        if not entry:
            continue
        failures.append(
            Failure(
                path=path,
                expected=list(entry.get("e", [])),
                actual=list(entry.get("a", [])),
                missing=list(entry.get("m", [])),
                extra=list(entry.get("x", [])),
            )
        )
    return failures


def matches_query(failure: Failure, *, code: str | None, category: str, diff: int) -> bool:
    if code and code not in failure.codes:
        return False

    if category in ("", "any"):
        return True
    if category in (
        "false-positive",
        "all-missing",
        "fingerprint-only",
        "only-missing",
        "only-extra",
        "wrong-code",
    ):
        return failure.category == category
    if category == "one-extra":
        return not failure.missing and len(failure.extra) == 1
    if category == "one-missing":
        return not failure.extra and len(failure.missing) == 1
    if category == "close":
        return (len(failure.missing) + len(failure.extra)) <= diff and bool(failure.missing or failure.extra)

    sys.exit(f"unknown category: {category}")


def select_failures(
    failures: list[Failure],
    *,
    code: str | None = None,
    category: str = "any",
    diff: int = 2,
    seed: str | None = None,
    count: int = 1,
    sample: bool = False,
) -> tuple[list[Failure], int]:
    candidates = [failure for failure in failures if matches_query(failure, code=code, category=category, diff=diff)]
    if not candidates:
        detail = f"category={category} code={code}" if category != "any" or code else "failures"
        sys.exit(f"no matching {detail}")

    rng = random.Random(int(seed)) if seed else random.Random()
    if sample:
        picks = rng.sample(candidates, min(count, len(candidates)))
    else:
        rng.shuffle(candidates)
        picks = candidates[:count]
    return picks, len(candidates)


def fmt_codes(codes: list[str]) -> str:
    return ",".join(codes) or "-"


def print_human_pick(
    failure: Failure,
    *,
    pool: int,
    requested_category: str | None = None,
    include_verbose_command: bool = True,
) -> None:
    if requested_category:
        print(f"category: {requested_category} (resolved: {failure.category})")
        print(f"path:     {failure.path}")
    else:
        print(f"path:     {failure.path}")
        print(f"category: {failure.category}")
    print(f"expected: {fmt_codes(failure.expected)}")
    print(f"actual:   {fmt_codes(failure.actual)}")
    print(f"missing:  {fmt_codes(failure.missing)}")
    print(f"extra:    {fmt_codes(failure.extra)}")
    print(f"pool:     {pool}")
    if include_verbose_command:
        print()
        print(
            f'verbose run: ./scripts/conformance/conformance.sh run --filter "{failure.filter_name}" --verbose'
        )


def resolve_test_source(root: Path, failure: Failure) -> Path | None:
    """Locate the test source on disk.

    The detail snapshot stores absolute paths from the machine that produced
    the snapshot (e.g. `/tmp/tsz-snap-refresh/TypeScript/tests/...`), so a
    naive `root / failure.path` resolves to a non-existent path on other
    machines. Fall back to the local TypeScript submodule by anchoring the
    path on the `TypeScript/tests/` segment.
    """
    direct = Path(failure.path)
    if direct.is_absolute() and direct.is_file():
        return direct
    candidate = root / failure.path.lstrip("/")
    if candidate.is_file():
        return candidate
    parts = Path(failure.path).parts
    for marker in ("TypeScript", "tests"):
        if marker in parts:
            idx = parts.index(marker)
            candidate = root.joinpath(*parts[idx:])
            if candidate.is_file():
                return candidate
    return None


def print_test_source(root: Path, failure: Failure, *, max_lines: int = 80) -> None:
    """Print the test source body, truncated to `max_lines` to keep terminals readable."""
    print()
    print("-------------------- test source --------------------")
    source = resolve_test_source(root, failure)
    if source is None:
        print(f"(source file missing: {failure.path})")
        return
    lines = source.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in lines[:max_lines]:
        print(line)
    if len(lines) > max_lines:
        print(f"... (truncated at {max_lines} lines; total {len(lines)})")


def run_verbose(root: Path, failure: Failure) -> None:
    print()
    print(f'Running: ./scripts/conformance/conformance.sh run --filter "{failure.filter_name}" --verbose')
    os.execv(
        str(root / "scripts" / "conformance" / "conformance.sh"),
        [
            str(root / "scripts" / "conformance" / "conformance.sh"),
            "run",
            "--filter",
            failure.filter_name,
            "--verbose",
        ],
    )


def command_quick(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=True))
    picks, pool = select_failures(failures, code=args.code, seed=args.seed)
    pick = picks[0]
    print_human_pick(pick, pool=pool)
    if args.show_source:
        print_test_source(root, pick)
    if args.run:
        run_verbose(root, pick)
    return 0


def command_category(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=True))
    picks, pool = select_failures(
        failures,
        code=args.code,
        category=args.category,
        diff=args.diff,
        seed=args.seed,
    )
    pick = picks[0]
    print_human_pick(pick, pool=pool, requested_category=args.category)
    if args.show_source:
        print_test_source(root, pick)
    if args.run:
        run_verbose(root, pick)
    return 0


def command_one(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=True))
    picks, _pool = select_failures(
        failures,
        code=args.code,
        category=args.category or "any",
        seed=args.seed,
    )
    pick = picks[0]
    if args.filter:
        print(pick.filter_name)
    else:
        print(
            f"{pick.filter_name}\t{pick.category}"
            f"\texpected={fmt_codes(pick.expected)}"
            f"\tactual={fmt_codes(pick.actual)}"
            f"\tmissing={fmt_codes(pick.missing)}"
            f"\textra={fmt_codes(pick.extra)}"
            f"\tpath={pick.path}"
        )
    return 0


def command_names(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=False))
    picks, _pool = select_failures(
        failures,
        code=args.code,
        category=args.category,
        seed=args.seed,
        count=args.count,
    )
    for pick in picks:
        print(pick.filter_name)
    return 0


def command_shortlist(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=False))
    picks, _pool = select_failures(
        failures,
        code=args.code,
        seed=None,
        count=args.count,
        sample=True,
    )
    for index, pick in enumerate(picks, 1):
        print(f"[{index}] {pick.path}")
        print(f"     category: {pick.category}")
        print(f"     expected: {fmt_codes(pick.expected)}")
        print(f"     actual:   {fmt_codes(pick.actual)}")
        print(f"     missing:  {fmt_codes(pick.missing)}")
        print(f"     extra:    {fmt_codes(pick.extra)}")
        print(
            f'     verbose:  ./scripts/conformance/conformance.sh run --filter "{pick.filter_name}" --verbose'
        )
        print()
    return 0


def command_show(args: argparse.Namespace) -> int:
    root = repo_root()
    failures = load_failures(ensure_inputs(root, ensure_submodule=True))
    picks, pool = select_failures(failures, code=args.code, seed=args.seed)
    pick = picks[0]

    print("==================== random pick ====================")
    print_human_pick(pick, pool=pool, include_verbose_command=False)
    print_test_source(root, pick)
    print()
    print("==================== verbose run ====================")
    run_verbose(root, pick)
    return 0


def add_common_pick_args(parser: argparse.ArgumentParser, *, run: bool = False) -> None:
    parser.add_argument("--seed", default="")
    parser.add_argument("--code", default="")
    if run:
        parser.add_argument("--run", action="store_true")
        parser.add_argument(
            "--show-source",
            action="store_true",
            help="print the test source after the pick metadata (truncated to 80 lines)",
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    quick = subparsers.add_parser("quick", help="print one human-readable random failure")
    add_common_pick_args(quick, run=True)
    quick.set_defaults(func=command_quick)

    category = subparsers.add_parser("category", help="print one failure filtered by category")
    add_common_pick_args(category, run=True)
    category.add_argument("--category", default="any")
    category.add_argument("--diff", type=int, default=2)
    category.set_defaults(func=command_category)

    one = subparsers.add_parser("one", help="print one compact failure line")
    add_common_pick_args(one)
    one.add_argument("--category", default="")
    one.add_argument("--filter", action="store_true")
    one.set_defaults(func=command_one)

    names = subparsers.add_parser("names", help="print random failure filter names")
    names.add_argument("count", nargs="?", type=int, default=1)
    names.add_argument("--code", default="")
    names.add_argument("--category", default="any")
    names.add_argument("--seed", default="")
    names.set_defaults(func=command_names)

    shortlist = subparsers.add_parser("shortlist", help="print N random failures with metadata")
    shortlist.add_argument("count", nargs="?", type=int, default=5)
    shortlist.add_argument("code", nargs="?", default="")
    shortlist.set_defaults(func=command_shortlist)

    show = subparsers.add_parser("show", help="pick, show source, and run verbose")
    add_common_pick_args(show)
    show.set_defaults(func=command_show)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if hasattr(args, "code"):
        args.code = args.code or None
    if hasattr(args, "seed"):
        args.seed = args.seed or None
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
