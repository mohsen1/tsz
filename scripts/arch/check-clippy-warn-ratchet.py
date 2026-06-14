#!/usr/bin/env python3
"""Clippy warn-level ratchet: lint counts can only go down (#13453).

Runs ``cargo clippy --workspace --exclude tsz-conformance --all-targets
--message-format json`` with warn-level overrides to count active warnings
per lint code, then compares the live counts against the committed baseline
in ``scripts/arch/clippy-warn-baseline.json``.

Exit code:
  0 — all counts are at or below the baseline (monotonic improvement or steady)
  1 — at least one lint count rose above its baseline value

Typical usage (called from ``run_lint()`` in ``scripts/ci/gcp-full-ci.sh``):
  python3 scripts/arch/check-clippy-warn-ratchet.py [--profile <profile>]

Baseline lifecycle:
  * The committed baseline captures the warn-level floor declared by
    ``CLIPPY_FLAGS`` below — the ``pedantic`` group plus targeted cherry-picks,
    minus a curated allow-list (#13443).  After changing ``CLIPPY_FLAGS``, run
    this script with ``--update-baseline`` to recapture the counts.
  * A PR that lowers a lint count may lower the baseline by the same amount
    (decrement the value or remove the key when it reaches zero).
  * Promoting a lint from ``warn`` to ``deny`` (in ``[workspace.lints.clippy]``)
    removes it from the baseline because the ``-D warnings`` gate already
    enforces it at zero.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE_PATH = REPO_ROOT / "scripts" / "arch" / "clippy-warn-baseline.json"

# Warn-level overrides for the ratchet pass (#13443).
#
# Why these live here and NOT in ``[workspace.lints.clippy]``:
# the CI lint gate runs ``cargo clippy ... -- -D warnings``.  Cargo emits the
# manifest ``[lints]`` table as ordinary ``--warn`` flags, and ``-D warnings``
# promotes *every* active warn-level lint to a hard error.  So a manifest
# ``pedantic = "warn"`` would not warn under the gate — it would fail the build
# on the first pedantic finding, workspace-wide.  Keeping the pedantic floor in
# this dedicated ratchet pass (which runs WITHOUT ``-D warnings``) is the only
# way to surface the group as a tracked, monotonically-shrinking baseline while
# the ``-D warnings`` gate keeps the deny groups and the six zero-tolerance
# manifest warns honest.  The baseline lives in ``clippy-warn-baseline.json``.
CLIPPY_FLAGS = [
    # The pedantic group at warn: opt-out instead of opt-in (#13443).
    "-W", "clippy::pedantic",
    # Targeted high-signal cherry-picks called out in #13443 (use_self lowers
    # the cost of the identity-handle newtype refactors; manual_let_else folds
    # the Option-fallback match pattern; semicolon_if_nothing_returned is pure
    # style hygiene).  Explicit even where pedantic already covers them so the
    # intent survives any future group recomposition.
    "-W", "clippy::use_self",
    "-W", "clippy::manual_let_else",
    "-W", "clippy::semicolon_if_nothing_returned",
    # Curated allow-list: pedantic members that are net-noise for this codebase.
    # Documentation-shaped lints — an internal compiler, not a published API:
    "-A", "clippy::module_name_repetitions",
    "-A", "clippy::must_use_candidate",
    "-A", "clippy::missing_errors_doc",
    "-A", "clippy::missing_panics_doc",
    # Intentional-cast family: the pervasive u32 identity newtypes
    # (TypeId/SymbolId/DefId/FlowNodeId/Atom) make these fire constantly on
    # deliberate, audited casts.
    "-A", "clippy::cast_possible_truncation",
    "-A", "clippy::cast_precision_loss",
    "-A", "clippy::cast_sign_loss",
    "-A", "clippy::cast_possible_wrap",
    # Raw-string hash counting: the embedded TypeScript test fixtures uniformly
    # use r#"..."# for consistency regardless of whether a `#` is needed, so
    # this fires ~10k times on intentional formatting rather than on defects.
    "-A", "clippy::needless_raw_string_hashes",
    # Explicitly DEFERRED by #13443: signature-churn lints touch hot-path
    # function signatures and conflict with the `fast` goal.  Revisit
    # separately, if at all.
    "-A", "clippy::needless_pass_by_value",
    "-A", "clippy::trivially_copy_pass_by_ref",
]


def run_clippy(profile: str) -> dict[str, int]:
    """Run cargo clippy and return per-lint warning counts."""
    cmd = [
        "cargo",
        "clippy",
        "--profile",
        profile,
        "--workspace",
        "--exclude",
        "tsz-conformance",
        "--all-targets",
        "--message-format",
        "json",
        "--",
        *CLIPPY_FLAGS,
    ]
    result = subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    # cargo clippy exits non-zero when there are errors (deny-level), but we
    # still want to parse warnings even on non-zero exit.
    counts: dict[str, int] = {}
    for line in result.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        m = msg.get("message", {})
        if m.get("level") != "warning":
            continue
        code = m.get("code") or {}
        lint_code = code.get("code")
        if lint_code and lint_code.startswith("clippy::"):
            counts[lint_code] = counts.get(lint_code, 0) + 1
    return counts


def load_baseline() -> dict[str, int]:
    if not BASELINE_PATH.exists():
        return {}
    return json.loads(BASELINE_PATH.read_text(encoding="utf-8"))


def save_baseline(counts: dict[str, int]) -> None:
    sorted_counts = {k: counts[k] for k in sorted(counts)}
    BASELINE_PATH.write_text(
        json.dumps(sorted_counts, indent=2) + "\n",
        encoding="utf-8",
    )


def check_ratchet(live: dict[str, int], baseline: dict[str, int]) -> list[str]:
    """Return a list of regressions (lint codes whose live count exceeds baseline)."""
    regressions = []
    for lint, count in live.items():
        cap = baseline.get(lint, 0)
        if count > cap:
            regressions.append(
                f"  {lint}: {count} warnings (baseline {cap}; "
                f"+{count - cap} — lower the count or raise the baseline intentionally)"
            )
    return sorted(regressions)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Clippy warn-level ratchet: counts can only go down (#13453)"
    )
    parser.add_argument(
        "--profile",
        default="ci-lint",
        help="Cargo profile to use (default: ci-lint)",
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Write the live counts to the committed baseline and exit 0. "
        "Use when intentionally accepting a new warn floor (e.g. after #13443).",
    )
    args = parser.parse_args()

    print("Running clippy warn-level ratchet…")
    live = run_clippy(args.profile)

    if args.update_baseline:
        save_baseline(live)
        total = sum(live.values())
        print(f"Baseline updated: {total} warning(s) across {len(live)} lint(s).")
        print(f"  {BASELINE_PATH.relative_to(REPO_ROOT)}")
        return 0

    baseline = load_baseline()
    regressions = check_ratchet(live, baseline)

    if regressions:
        print("CLIPPY WARN RATCHET FAILURES:")
        for r in regressions:
            print(r)
        print(
            "\nTo accept the new counts run: "
            "python3 scripts/arch/check-clippy-warn-ratchet.py --update-baseline"
        )
        return 1

    total = sum(live.values())
    baseline_total = sum(baseline.values())
    if total < baseline_total:
        # Some warnings were fixed — remind the contributor to lower the baseline.
        print(
            f"Clippy warn ratchet: {total} warning(s) — "
            f"{baseline_total - total} below baseline. "
            "Consider lowering the committed baseline."
        )
    else:
        print(f"Clippy warn ratchet: {total} warning(s) — at or below baseline. OK.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
