#!/usr/bin/env python3
"""Link conformance regression issues to current snapshot rows.

For each input — a bare test name, a TypeScript path, or a free-form issue
title — the helper reports whether the named conformance test is failing,
passing, or accepted-as-regression in the current checked-in snapshot.

Inputs with no resolvable test names are classified as ``aggregate`` and get
a dashboard pointer instead of a fabricated single-row claim.

The helper never re-runs conformance and never posts comments. It only reads
checked-in artifacts under ``scripts/conformance``:

    conformance-detail.json                (per-test failure detail)
    conformance-snapshot.json              (summary + artifact timestamp)
    conformance-baseline.txt               (one line per test, PASS/FAIL)
    conformance-accepted-regressions.txt   (failing-but-tracked tests)

Status taxonomy
---------------

``failing``
    The named test path appears in ``conformance-detail.json`` failures and
    is not in the accepted-regression list.

``accepted-regression``
    The test fails but is listed in ``conformance-accepted-regressions.txt``.

``stale-accepted``
    The test is in the accepted-regression list but no longer appears in
    ``conformance-detail.json`` failures. The accepted entry can be retired.

``passing``
    The test passes in the baseline and is not accepted as a regression.
    Any open regression issue naming the test is stale and may be closed.

``unknown``
    The token did not match any test path in the baseline or the accepted
    list.

``aggregate``
    The input names no specific test row. The helper links the dashboard
    categories instead of fabricating a single matching row.

Usage
-----

    # Single bare test name
    python3 scripts/conformance/link-regression-issues.py tsxGenericAttributesType6

    # Several issue titles / names at once
    python3 scripts/conformance/link-regression-issues.py \\
        "tsxGenericAttributesType6 wrong codes" \\
        "excessPropertyCheckIntersectionWithRecursiveType regression"

    # One input per line from a file
    python3 scripts/conformance/link-regression-issues.py --from-file issues.txt

    # One input per line from stdin
    cat issues.txt | python3 scripts/conformance/link-regression-issues.py --stdin

    # Emit JSON instead of markdown
    python3 scripts/conformance/link-regression-issues.py --json tsxGenericAttributesType6
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from lib.conformance_query import basename
from lib.query_snapshot import load_snapshot

DEFAULT_CONFORMANCE_DIR = Path(__file__).resolve().parent

DETAIL_FILENAME = "conformance-detail.json"
SNAPSHOT_FILENAME = "conformance-snapshot.json"
BASELINE_FILENAME = "conformance-baseline.txt"
ACCEPTED_FILENAME = "conformance-accepted-regressions.txt"

DASHBOARD_HINT = "python3 scripts/conformance/query-conformance.py --dashboard"
SNAPSHOT_REGEN_HINT = "./scripts/conformance/conformance.sh snapshot"

# Each status entry holds the label rendered in the markdown table and the
# closure-pattern guidance line. Keys are returned by `_classify_path` and
# `resolve_inputs`. Order is the order the closure pattern lists them — start
# with the actions an inspector is most likely to take.
STATUSES: dict[str, tuple[str, str]] = {
    "stale-accepted": (
        "stale: accepted-regression entry no longer fails",
        "Remove the entry from `conformance-accepted-regressions.txt`, then "
        "close the regression issue with `not planned (stale)`.",
    ),
    "passing": (
        "stale: passing in current snapshot",
        "Close the issue with `not planned (stale)` and link to this comment "
        "for the snapshot evidence.",
    ),
    "failing": (
        "failing in current snapshot",
        "Keep the issue open and link to this comment so the next inspector "
        "can find the latest snapshot status.",
    ),
    "accepted-regression": (
        "accepted regression (failing but tracked)",
        "Keep the issue open. Add the `accepted-regression` label so triage "
        "is one search away.",
    ),
    "aggregate": (
        "aggregate: no single snapshot row",
        "Reply with the dashboard categories below. Close only when the "
        "matching category counter reaches zero.",
    ),
    "unknown": (
        "unknown: not found in baseline or accepted list",
        "Rename or re-scope the issue; no checked-in artifact references the "
        "name. The issue may have predated a rename of the upstream test.",
    ),
}

_TOKEN_SPLIT = re.compile(r"[\s,;:()\[\]{}\"'`]+")
TEST_EXTENSIONS = (".tsx", ".ts", ".jsx", ".js")


@dataclass(frozen=True)
class ResolvedTest:
    name: str
    path: str
    status: str


@dataclass
class InputResult:
    raw: str
    resolved: list[ResolvedTest] = field(default_factory=list)

    @property
    def is_aggregate(self) -> bool:
        return not self.resolved


@dataclass(frozen=True)
class SnapshotIndex:
    timestamp: str
    git_sha: str
    summary: dict[str, object]
    failures: set[str]
    accepted: set[str]
    baseline_pass: set[str]
    baseline_fail: set[str]
    # Basename → every path with that basename. Duplicate camelCase basenames
    # do occur (`useModule` lives under nine `projects/` directories, and
    # `parserReturnStatement4` has both a `.js` and a `.ts` variant), so
    # collapsing to a single representative would silently misreport ambiguity.
    basename_to_paths: dict[str, list[str]]


def _strip_test_ext(name: str) -> str:
    for ext in TEST_EXTENSIONS:
        if name.endswith(ext):
            return name[: -len(ext)]
    return name


def load_snapshot_index(conformance_dir: Path) -> SnapshotIndex:
    snapshot = load_snapshot(conformance_dir / SNAPSHOT_FILENAME, SNAPSHOT_REGEN_HINT)
    detail = load_snapshot(conformance_dir / DETAIL_FILENAME, SNAPSHOT_REGEN_HINT)
    baseline_path = conformance_dir / BASELINE_FILENAME
    accepted_path = conformance_dir / ACCEPTED_FILENAME
    if not baseline_path.exists():
        raise SystemExit(f"required artifact not found: {baseline_path}")
    if not accepted_path.exists():
        raise SystemExit(f"required artifact not found: {accepted_path}")

    accepted = {
        stripped
        for line in accepted_path.read_text(encoding="utf-8").splitlines()
        if (stripped := line.strip()) and not stripped.startswith("#")
    }

    baseline_pass: set[str] = set()
    baseline_fail: set[str] = set()
    basename_to_paths: dict[str, list[str]] = {}
    for line in baseline_path.read_text(encoding="utf-8").splitlines():
        head, _, rest = line.partition(" ")
        path = rest.split(" |", 1)[0].strip()
        if not path:
            continue
        if head == "PASS":
            baseline_pass.add(path)
        elif head == "FAIL":
            baseline_fail.add(path)
        else:
            continue
        basename_to_paths.setdefault(_strip_test_ext(basename(path)), []).append(path)

    return SnapshotIndex(
        timestamp=str(snapshot.get("timestamp", "")),
        git_sha=str(snapshot.get("git_sha", "")),
        summary=snapshot.get("summary", {}),
        failures=set(detail.get("failures", {}).keys()),
        accepted=accepted,
        baseline_pass=baseline_pass,
        baseline_fail=baseline_fail,
        basename_to_paths=basename_to_paths,
    )


def _classify_path(path: str, index: SnapshotIndex) -> str:
    if path in index.failures:
        return "accepted-regression" if path in index.accepted else "failing"
    if path in index.accepted:
        return "stale-accepted"
    if path in index.baseline_pass:
        return "passing"
    if path in index.baseline_fail:
        return "failing"
    return "unknown"


def _candidate_tokens(raw: str) -> list[str]:
    return [token for token in _TOKEN_SPLIT.split(raw) if token]


def _looks_like_test_token(token: str) -> bool:
    # Gate basename lookups against common English words: many upstream tests
    # have bare lowercase basenames like `emit`, `index`, `test` that would
    # otherwise match every issue title containing those words.
    if any(token.endswith(ext) for ext in TEST_EXTENSIONS):
        return True
    if "TypeScript/tests/cases" in token:
        return True
    if len(token) < 6:
        return False
    has_lower = any(c.islower() for c in token)
    has_upper = any(c.isupper() for c in token)
    return has_lower and has_upper


def _lookup_token(token: str, index: SnapshotIndex) -> list[str]:
    """Return every snapshot path that ``token`` could refer to.

    A bare basename can be ambiguous (e.g. `useModule` lives under nine
    `projects/` directories) — every match is returned so the caller can
    render one row per candidate instead of fabricating a single row.
    """
    if not _looks_like_test_token(token):
        return []
    if "TypeScript/tests/cases" in token:
        candidate = token.lstrip("<").rstrip(">")
        if (
            candidate in index.baseline_pass
            or candidate in index.baseline_fail
            or candidate in index.accepted
        ):
            return [candidate]
        return []
    return list(index.basename_to_paths.get(_strip_test_ext(basename(token)), []))


def resolve_inputs(inputs: Iterable[str], index: SnapshotIndex) -> list[InputResult]:
    results: list[InputResult] = []
    for raw in inputs:
        result = InputResult(raw=raw)
        seen: set[str] = set()
        for token in _candidate_tokens(raw):
            for path in _lookup_token(token, index):
                if path in seen:
                    continue
                seen.add(path)
                result.resolved.append(
                    ResolvedTest(
                        name=basename(path),
                        path=path,
                        status=_classify_path(path, index),
                    )
                )
        results.append(result)
    return results


def _escape_pipe(text: str) -> str:
    return text.replace("|", "\\|")


def render_markdown(results: list[InputResult], index: SnapshotIndex) -> str:
    summary = index.summary
    passed = summary.get("passed", "?")
    total = summary.get("total_tests", summary.get("total", "?"))
    lines: list[str] = [
        "## Conformance regression issue ↔ snapshot link",
        "",
        f"_Snapshot {index.timestamp or '(unknown timestamp)'} · "
        f"git `{index.git_sha or 'unknown'}` · {passed} / {total} passing._",
        "",
        "| Input | Test | Status |",
        "|-------|------|--------|",
    ]
    for result in results:
        raw = _escape_pipe(result.raw)
        if result.is_aggregate:
            lines.append(
                f"| `{raw}` | _aggregate / no specific test_ | "
                f"{STATUSES['aggregate'][0]} |"
            )
            continue
        ambiguous = len(result.resolved) > 1
        for resolved in result.resolved:
            marker = " _(ambiguous basename — one row per match)_" if ambiguous else ""
            lines.append(
                f"| `{raw}` | `{resolved.name}` (`{resolved.path}`){marker} | "
                f"{STATUSES[resolved.status][0]} |"
            )
    lines.append("")
    if any(result.is_aggregate for result in results):
        lines.extend(
            [
                "Aggregate inputs do not name a specific test row. Inspect the "
                "current dashboard categories instead:",
                "",
                f"```\n{DASHBOARD_HINT}\n```",
                "",
            ]
        )
    lines.append("### Closure pattern for stale regression issues")
    lines.append("")
    for label, guidance in STATUSES.values():
        lines.append(f"- **{label}** — {guidance}")
    return "\n".join(lines) + "\n"


def render_json(results: list[InputResult], index: SnapshotIndex) -> str:
    payload = {
        "timestamp": index.timestamp,
        "git_sha": index.git_sha,
        "summary": index.summary,
        "results": [
            {
                "input": result.raw,
                "aggregate": result.is_aggregate,
                "resolved": [
                    {"name": r.name, "path": r.path, "status": r.status}
                    for r in result.resolved
                ],
            }
            for result in results
        ],
    }
    return json.dumps(payload, indent=2) + "\n"


def _collect_inputs(args: argparse.Namespace) -> list[str]:
    inputs: list[str] = list(args.inputs)
    if args.from_file:
        inputs.extend(
            line
            for line in Path(args.from_file).read_text(encoding="utf-8").splitlines()
            if line.strip()
        )
    if args.stdin:
        inputs.extend(line for line in sys.stdin.read().splitlines() if line.strip())
    if not inputs:
        raise SystemExit(
            "no inputs provided; pass test names/titles, --from-file, or --stdin"
        )
    return inputs


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Link conformance regression issues to current snapshot rows.",
    )
    parser.add_argument(
        "inputs",
        nargs="*",
        help="Issue titles or test names. May contain a TypeScript path verbatim or a bare basename.",
    )
    parser.add_argument(
        "--from-file",
        help="Read one input per line from this file.",
    )
    parser.add_argument(
        "--stdin",
        action="store_true",
        help="Read one input per line from stdin.",
    )
    parser.add_argument(
        "--conformance-dir",
        default=str(DEFAULT_CONFORMANCE_DIR),
        help="Override the directory containing snapshot artifacts.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON instead of markdown.",
    )
    args = parser.parse_args(argv)

    inputs = _collect_inputs(args)
    index = load_snapshot_index(Path(args.conformance_dir))
    results = resolve_inputs(inputs, index)
    output = render_json(results, index) if args.json else render_markdown(results, index)
    sys.stdout.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
