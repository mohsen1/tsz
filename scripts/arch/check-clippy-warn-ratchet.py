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
  * The committed baseline starts at ``{}`` (zero warnings under the current
    ``-D warnings`` gate).  When a future PR promotes lints to ``warn``
    (e.g. ``pedantic = warn`` from #13443), run this script with
    ``--update-baseline`` to capture the new counts.
  * A PR that lowers a lint count may lower the baseline by the same amount
    (decrement the value or remove the key when it reaches zero).
  * Promoting a lint from ``warn`` to ``deny`` removes it from the baseline
    because ``-D warnings`` already enforces it.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
BASELINE_PATH = REPO_ROOT / "scripts" / "arch" / "clippy-warn-baseline.json"

CLIPPY_FLAGS = [
    # Mirror the warn-level overrides that will be in [workspace.lints.clippy]
    # once #13443 lands.  Add lints here as they are promoted to warn so the
    # ratchet tracks them from day one.
    #
    # Example (uncomment when #13443 is merged):
    # "-W", "clippy::pedantic",
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
