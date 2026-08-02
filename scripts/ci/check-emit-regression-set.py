#!/usr/bin/env python3
"""Direction check for the emit gate: diff the named failing-row set (#16171).

The emit gate in ``scripts/ci/full-ci.sh`` compares summed pass *counts*
against ``scripts/emit/emit-snapshot.json``. Two things that comparison
structurally cannot see:

1. **A swap.** One row fixed and another broken leaves ``jsPass`` unchanged,
   so a real regression passes the gate.
2. **A ratchet-down.** ``cap_positive_baseline`` computes ``min(baseline,
   floor)``, so ``TSZ_CI_JS_ACCEPTED_FLOOR`` is a *ceiling* — an
   anti-unsatisfiability valve — not a floor. If ``emit-snapshot.json`` is
   ever hand-refreshed while emit is regressed, the count bar follows the
   snapshot down and the constant that looks like a backstop does not stop it.

``emit-snapshot.json``'s ``detailFingerprint`` / ``detailResultCount`` pin that
``emit-detail.json`` matches its own summary. That is an internal-consistency
check, not a direction check.

This script is the direction check, modelled on how conformance diffs its
failure set rather than its pass count: a row that is failing now and was not
failing in the committed baseline is reported by name and fails the gate,
whatever the counts say.

Deliberately asymmetric:

* **Newly failing rows are fatal.** That is the regression this exists to catch.
* **Baseline rows absent from the run are a warning, not an error.** The
  corpus legitimately gains and loses tests when the TypeScript submodule is
  bumped, and a gate that hard-fails on that would just get disabled.
* **Rows that are failing in the baseline and still failing are silent.** They
  are the accepted set; shrinking it is an improvement and never blocks.
"""

import argparse
import json
import pathlib
import sys


FAILING_STATUSES = ("fail", "timeout")


def row_key(row):
    """Stable identity for one emit result row.

    ``(testPath, baselineFile, name)`` is unique across the committed corpus
    (11564/11564 distinct at the time of writing).
    """
    return (
        row.get("testPath") or "",
        row.get("baselineFile") or "",
        row.get("name") or "",
    )


def format_key(key):
    test_path, baseline_file, name = key
    return "%s [%s] (%s)" % (name, baseline_file, test_path)


def load_results(path):
    """Return the ``results`` list from an emit detail JSON document."""
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    results = data.get("results")
    if not isinstance(results, list):
        raise ValueError("%s has no 'results' array" % path)
    return results


def index_rows(results):
    """Map row key -> row, last write wins for duplicate keys."""
    indexed = {}
    for row in results:
        if isinstance(row, dict):
            indexed[row_key(row)] = row
    return indexed


def is_failing(status):
    return status in FAILING_STATUSES


def find_regressions(baseline_rows, current_rows):
    """Rows failing now that were not failing in the baseline.

    Returns a list of ``(key, kind, baseline_status, current_status)`` tuples
    sorted by name, where ``kind`` is ``"JS"`` or ``"DTS"``.
    """
    regressions = []
    for key, current in sorted(current_rows.items()):
        baseline = baseline_rows.get(key)
        if baseline is None:
            # A row the baseline has never seen (new corpus test). It cannot be
            # a regression against a baseline that does not describe it.
            continue
        for kind, field in (("JS", "jsStatus"), ("DTS", "dtsStatus")):
            current_status = current.get(field)
            baseline_status = baseline.get(field)
            if is_failing(current_status) and not is_failing(baseline_status):
                regressions.append((key, kind, baseline_status, current_status))
    return regressions


def find_absent(baseline_rows, current_rows):
    """Baseline rows the run did not report at all."""
    return sorted(key for key in baseline_rows if key not in current_rows)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        default="scripts/emit/emit-detail.json",
        help="committed emit detail JSON to diff against",
    )
    parser.add_argument(
        "detail",
        nargs="+",
        help="per-shard emit detail JSON produced by scripts/emit/run.sh --json-out",
    )
    parser.add_argument(
        "--max-report",
        type=int,
        default=50,
        help="cap the number of named rows printed (all are counted)",
    )
    args = parser.parse_args(argv)

    baseline_path = pathlib.Path(args.baseline)
    if not baseline_path.is_file():
        print(
            "error: emit regression set check needs a baseline at %s" % baseline_path,
            file=sys.stderr,
        )
        return 1

    try:
        baseline_rows = index_rows(load_results(baseline_path))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print("error: cannot read emit baseline %s: %s" % (baseline_path, exc), file=sys.stderr)
        return 1

    current_rows = {}
    for detail in args.detail:
        try:
            current_rows.update(index_rows(load_results(detail)))
        except (OSError, ValueError, json.JSONDecodeError) as exc:
            print("error: cannot read emit detail %s: %s" % (detail, exc), file=sys.stderr)
            return 1

    if not current_rows:
        print(
            "error: emit regression set check found no result rows across %d detail file(s)"
            % len(args.detail),
            file=sys.stderr,
        )
        return 1

    absent = find_absent(baseline_rows, current_rows)
    if absent:
        # Not fatal: the corpus changes when the TypeScript submodule moves.
        print(
            "warning: %d baseline emit row(s) were not reported by this run "
            "(corpus drift, or a shard that produced no detail)" % len(absent),
            file=sys.stderr,
        )
        for key in absent[: args.max_report]:
            print("warning:   absent %s" % format_key(key), file=sys.stderr)

    regressions = find_regressions(baseline_rows, current_rows)
    if regressions:
        print(
            "error: emit regression: %d row(s) fail now and did not fail in %s"
            % (len(regressions), baseline_path),
            file=sys.stderr,
        )
        for key, kind, baseline_status, current_status in regressions[: args.max_report]:
            print(
                "error:   %s %s: %s -> %s"
                % (kind, format_key(key), baseline_status, current_status),
                file=sys.stderr,
            )
        if len(regressions) > args.max_report:
            print(
                "error:   ... and %d more" % (len(regressions) - args.max_report),
                file=sys.stderr,
            )
        return 1

    baseline_failing = sum(
        1
        for row in baseline_rows.values()
        if is_failing(row.get("jsStatus")) or is_failing(row.get("dtsStatus"))
    )
    current_failing = sum(
        1
        for row in current_rows.values()
        if is_failing(row.get("jsStatus")) or is_failing(row.get("dtsStatus"))
    )
    print(
        "Emit regression set OK: %d row(s) compared, failing rows %d -> %d "
        "(no row newly failing)" % (len(current_rows), baseline_failing, current_failing)
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
