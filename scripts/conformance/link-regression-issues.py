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
    ``conformance-detail.json`` failures. The accepted entry can be
    retired.

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
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DEFAULT_CONFORMANCE_DIR = REPO_ROOT / "scripts" / "conformance"

DETAIL_FILENAME = "conformance-detail.json"
SNAPSHOT_FILENAME = "conformance-snapshot.json"
BASELINE_FILENAME = "conformance-baseline.txt"
ACCEPTED_FILENAME = "conformance-accepted-regressions.txt"

DASHBOARD_HINT = "python3 scripts/conformance/query-conformance.py --dashboard"

STATUS_LABELS = {
    "failing": "failing in current snapshot",
    "accepted-regression": "accepted regression (failing but tracked)",
    "stale-accepted": "stale: accepted-regression entry no longer fails",
    "passing": "stale: passing in current snapshot",
    "unknown": "unknown: not found in baseline or accepted list",
    "aggregate": "aggregate: no single snapshot row",
}

CLOSURE_GUIDANCE = {
    "passing": (
        "Close the issue with `not planned (stale)` and link to this comment "
        "for the snapshot evidence."
    ),
    "stale-accepted": (
        "Remove the entry from `conformance-accepted-regressions.txt`, then "
        "close the regression issue with `not planned (stale)`."
    ),
    "failing": (
        "Keep the issue open and link to this comment so the next inspector "
        "can find the latest snapshot status."
    ),
    "accepted-regression": (
        "Keep the issue open. Add the `accepted-regression` label so triage "
        "is one search away."
    ),
    "unknown": (
        "Rename or re-scope the issue; no checked-in artifact references the "
        "name. The issue may have predated a rename of the upstream test."
    ),
    "aggregate": (
        "Reply with the dashboard categories below. Close only when the "
        "matching category counter reaches zero."
    ),
}

# Split issue titles / free-form input on whitespace and common punctuation.
# Identifiers, dotted paths, and quoted test names survive the split intact.
_TOKEN_SPLIT = re.compile(r"[\s,;:()\[\]{}\"'`]+")

TEST_EXTENSIONS = (".tsx", ".ts", ".jsx", ".js")


@dataclass(frozen=True)
class ResolvedTest:
    """A test name resolved against the snapshot artifacts."""

    name: str
    path: str
    status: str


@dataclass
class InputResult:
    """Per-input report row."""

    raw: str
    resolved: list[ResolvedTest] = field(default_factory=list)

    @property
    def is_aggregate(self) -> bool:
        return not self.resolved


@dataclass(frozen=True)
class SnapshotIndex:
    """Indexed view of the conformance artifacts the helper needs."""

    timestamp: str
    git_sha: str
    summary: dict
    failures: set[str]
    accepted: set[str]
    baseline_pass: set[str]
    baseline_fail: set[str]
    basename_to_path: dict[str, str]


def _basename(path: str) -> str:
    return path.rsplit("/", 1)[-1] if "/" in path else path


def _strip_test_ext(name: str) -> str:
    for ext in TEST_EXTENSIONS:
        if name.endswith(ext):
            return name[: -len(ext)]
    return name


def _read_json(path: Path) -> dict:
    if not path.exists():
        raise SystemExit(f"required artifact not found: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def _read_lines(path: Path) -> list[str]:
    if not path.exists():
        raise SystemExit(f"required artifact not found: {path}")
    return path.read_text(encoding="utf-8").splitlines()


def load_snapshot_index(conformance_dir: Path) -> SnapshotIndex:
    snapshot = _read_json(conformance_dir / SNAPSHOT_FILENAME)
    detail = _read_json(conformance_dir / DETAIL_FILENAME)
    baseline_lines = _read_lines(conformance_dir / BASELINE_FILENAME)
    accepted_lines = _read_lines(conformance_dir / ACCEPTED_FILENAME)

    failures = set(detail.get("failures", {}).keys())
    accepted = {
        stripped
        for line in accepted_lines
        if (stripped := line.strip()) and not stripped.startswith("#")
    }

    baseline_pass: set[str] = set()
    baseline_fail: set[str] = set()
    basename_to_path: dict[str, str] = {}
    for line in baseline_lines:
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
        # Index by basename-without-extension for free-form input matching.
        # First occurrence wins so the lookup is deterministic; basename
        # collisions across directories are rare and a deterministic policy
        # is enough for this helper's reporting purpose.
        basename_to_path.setdefault(_strip_test_ext(_basename(path)), path)

    summary = snapshot.get("summary", {})
    return SnapshotIndex(
        timestamp=str(snapshot.get("timestamp", "")),
        git_sha=str(snapshot.get("git_sha", "")),
        summary=summary,
        failures=failures,
        accepted=accepted,
        baseline_pass=baseline_pass,
        baseline_fail=baseline_fail,
        basename_to_path=basename_to_path,
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
    """True when ``token`` is shaped like a TypeScript test name reference.

    Required to gate basename lookups against common English words: many
    upstream tests are named with bare lowercase basenames like ``emit``,
    ``index``, or ``test`` that would otherwise match every issue title
    that happens to contain those words.
    """
    if any(token.endswith(ext) for ext in TEST_EXTENSIONS):
        return True
    if "TypeScript/tests/cases" in token:
        return True
    if len(token) < 6:
        return False
    has_lower = any(c.islower() for c in token)
    has_upper = any(c.isupper() for c in token)
    return has_lower and has_upper


def _lookup_token(token: str, index: SnapshotIndex) -> str | None:
    if not _looks_like_test_token(token):
        return None
    if "TypeScript/tests/cases" in token:
        # Strip leading punctuation that survived tokenization (e.g. `<path>`).
        candidate = token.lstrip("<").rstrip(">")
        if candidate in index.baseline_pass or candidate in index.baseline_fail:
            return candidate
    return index.basename_to_path.get(_strip_test_ext(_basename(token)))


def resolve_inputs(inputs: Iterable[str], index: SnapshotIndex) -> list[InputResult]:
    results: list[InputResult] = []
    for raw in inputs:
        result = InputResult(raw=raw)
        seen: set[str] = set()
        for token in _candidate_tokens(raw):
            path = _lookup_token(token, index)
            if path is None or path in seen:
                continue
            seen.add(path)
            result.resolved.append(
                ResolvedTest(
                    name=_basename(path),
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
                f"{STATUS_LABELS['aggregate']} |"
            )
            continue
        for resolved in result.resolved:
            lines.append(
                f"| `{raw}` | `{resolved.name}` (`{resolved.path}`) | "
                f"{STATUS_LABELS[resolved.status]} |"
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
    for status, label in STATUS_LABELS.items():
        lines.append(f"- **{label}** — {CLOSURE_GUIDANCE[status]}")
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
        path = Path(args.from_file)
        if not path.exists():
            raise SystemExit(f"--from-file path does not exist: {path}")
        inputs.extend(
            line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()
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
