#!/usr/bin/env python3
"""Canonical parsing and validation for the accepted-conformance-regression ledger.

The accepted-regression ledger
(`scripts/conformance/conformance-accepted-regressions.txt`) records conformance
tests that are temporarily allowed to fail in CI without blocking the aggregate
gate. Its default budget is monotonically non-increasing on `main`: a PR may
remove entries (progress), and new entries must carry adjacent issue-linked
evidence plus a removal condition.

This module is the single source of truth for how that ledger is parsed,
normalized, and validated. It is shared by:

  * `check-accepted-regression-growth.py` - the CI growth/integrity gate.
  * `query-conformance.py` - the `--dashboard` accepted-regression counter.

The CI aggregate matcher in `scripts/ci/lib/gcp-full-ci-conformance.sh` (the
`_check_conformance_regression_allowlist` heredoc) applies the same `normalize`
rule when it compares per-shard failure lists against this ledger.
`test_accepted_regressions.py` pins that contract so the inline copy in the
shell heredoc and this module cannot drift apart silently. (A second, display-
only `normalize` copy in the same shell file falls back to `os.path.basename`
and is deliberately not mirrored here -- it does not govern the ledger.)
"""

from __future__ import annotations

from dataclasses import dataclass

# Every accepted regression names a TypeScript conformance test case. The
# aggregate matcher only keys on the ``TypeScript`` segment, so this prefix is
# intentionally stricter than the matcher: the ledger has only ever held
# ``tests/cases/`` fixtures, and the tighter shape catches typos. Widen it here
# if a legitimate failing fixture ever lives outside ``tests/cases/``.
TEST_CASE_PREFIX = "TypeScript/tests/cases/"

# TypeScript fixtures use these source extensions. A ledger entry that does not
# end in one of them can never match a real failing test path and is almost
# certainly a typo.
TEST_CASE_SUFFIXES = (".ts", ".tsx", ".js", ".jsx", ".mts", ".cts")


def normalize(path: str) -> str:
    """Return the ledger-canonical form of a test path.

    Mirrors the aggregate-matcher ``normalize`` helper in
    ``scripts/ci/lib/gcp-full-ci-conformance.sh``: backslashes become forward
    slashes and the path is sliced from its first ``TypeScript`` segment so that
    absolute shard paths and repo-relative ledger entries compare equal.

    This deliberately differs from the adjacent
    ``lib/results.normalize_harness_path``, which slices from the *last*
    ``TypeScript/`` and does not convert backslashes; do not unify them, or the
    CI matcher parity pinned by ``test_accepted_regressions.py`` breaks.
    """
    parts = path.replace("\\", "/").split("/")
    for index, part in enumerate(parts):
        if part == "TypeScript":
            return "/".join(parts[index:])
    return "/".join(parts)


def iter_raw_entries(text: str):
    """Yield ``(lineno, value)`` for each non-blank, non-comment ledger line.

    ``lineno`` is 1-based. ``value`` is the stripped (but not yet normalized)
    line, so integrity checks can flag non-canonical spellings before they are
    collapsed by :func:`normalize`.
    """
    for lineno, raw_line in enumerate(text.splitlines(), start=1):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        yield lineno, stripped


def parse_entries(text: str) -> list[str]:
    """Return stripped ledger entries in file order (duplicates preserved)."""
    return [value for _lineno, value in iter_raw_entries(text)]


def normalized_entries(text: str) -> list[str]:
    """Return normalized ledger entries in file order (duplicates preserved)."""
    return [normalize(value) for value in parse_entries(text)]


def entry_set(text: str) -> frozenset[str]:
    """Return the deduplicated, normalized set of ledger entries.

    This is the set CI actually treats as "accepted", so it is the right basis
    for growth comparison and for the visible dashboard counter.
    """
    return frozenset(normalized_entries(text))


def check_growth(
    base_entries: frozenset[str],
    head_entries: frozenset[str],
) -> tuple[frozenset[str], frozenset[str]]:
    """Return ``(added, removed)`` normalized entry sets relative to ``base``."""
    return head_entries - base_entries, base_entries - head_entries


def entry_comment_blocks(text: str) -> dict[str, list[str]]:
    """Return the contiguous comment block immediately preceding each entry.

    The growth gate uses this to distinguish silent ledger growth from an
    explicitly documented temporary exception. Blank lines reset the block, so
    only the comments visually attached to an entry authorize that entry.
    """
    blocks: dict[str, list[str]] = {}
    pending_comments: list[str] = []

    for raw_line in text.splitlines():
        stripped = raw_line.strip()
        if not stripped:
            pending_comments = []
            continue
        if stripped.startswith("#"):
            pending_comments.append(stripped[1:].strip())
            continue
        blocks[normalize(stripped)] = pending_comments
        pending_comments = []

    return blocks


def documented_temporary_additions(
    text: str,
    added_entries: frozenset[str],
) -> tuple[frozenset[str], frozenset[str]]:
    """Split added entries into documented temporary exceptions and rejects.

    A temporary addition must have an adjacent comment block that names the
    tracking issue, exact evidence, and the removal condition. This keeps the
    default no-growth guard intact while permitting explicitly owned queue
    stabilization debt.
    """
    blocks = entry_comment_blocks(text)
    documented: set[str] = set()
    rejected: set[str] = set()

    for entry in added_entries:
        block = "\n".join(blocks.get(entry, [])).lower()
        has_issue = "tracked by issue #" in block or "tracked by #" in block
        has_evidence = "exact evidence" in block
        has_removal = "remove once" in block or "removal condition" in block
        if has_issue and has_evidence and has_removal:
            documented.add(entry)
        else:
            rejected.add(entry)

    return frozenset(documented), frozenset(rejected)


@dataclass(frozen=True)
class IntegrityProblem:
    """A single ledger hygiene violation, suitable for a CI annotation."""

    kind: str
    value: str
    lineno: int | None
    message: str

    def format(self) -> str:
        location = f"line {self.lineno}: " if self.lineno is not None else ""
        return f"{location}{self.message}"


def check_integrity(text: str) -> list[IntegrityProblem]:
    """Return ledger hygiene violations, in a stable reporting order.

    A healthy ledger is the trustworthy basis for the visible counter and for
    the growth gate, so this rejects three classes that would otherwise enter
    silently:

      * non-canonical entries (``value != normalize(value)``), which would make
        the growth set disagree with CI's aggregate matcher;
      * duplicate entries (after normalization), which inflate the counter and
        can mask a removal;
      * malformed entries that do not look like a real
        ``TypeScript/tests/cases/<...>`` test path and so could never match a
        failing test.
    """
    problems: list[IntegrityProblem] = []
    first_seen: dict[str, int] = {}

    for lineno, value in iter_raw_entries(text):
        canonical = normalize(value)

        if value != canonical:
            problems.append(
                IntegrityProblem(
                    kind="non-canonical",
                    value=value,
                    lineno=lineno,
                    message=(
                        f"entry {value!r} is not in canonical form; "
                        f"write it as {canonical!r}"
                    ),
                )
            )

        if canonical in first_seen:
            problems.append(
                IntegrityProblem(
                    kind="duplicate",
                    value=canonical,
                    lineno=lineno,
                    message=(
                        f"duplicate entry {canonical!r} "
                        f"(first listed on line {first_seen[canonical]})"
                    ),
                )
            )
        else:
            first_seen[canonical] = lineno

        if not canonical.startswith(TEST_CASE_PREFIX) or not canonical.endswith(
            TEST_CASE_SUFFIXES
        ):
            problems.append(
                IntegrityProblem(
                    kind="malformed",
                    value=canonical,
                    lineno=lineno,
                    message=(
                        f"entry {canonical!r} does not look like a test path "
                        f"under {TEST_CASE_PREFIX!r} ending in one of "
                        f"{', '.join(TEST_CASE_SUFFIXES)}"
                    ),
                )
            )

    return problems
