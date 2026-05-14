#!/usr/bin/env python3
"""Query attribution-mode perf-counter JSON without rerunning the bench.

Reads from a `--perf-counters-json` output file (the format documented in
`docs/plan/PERFORMANCE_PLAN.md` §3 and produced by `tsz --features perf-tools`
with `TSZ_PERF_COUNTERS=1`).

Default mode prints a one-page snapshot, including which
`CheckerCreationReason` accounts for the largest share of
`with_parent_cache_constructed`. That number is the lever the
performance roadmap is trying to move. Checked-in attribution runs were
removed from the repo; pass a fresh JSON artifact with `--json`.

Usage:
  # Point at a specific JSON file (e.g. a fresh post-PR re-measurement).
  python3 scripts/perf/query-perf-counters.py --json /tmp/post-fix-pc.json

  # Per-reason breakdown only, with absolute counts and percent share.
  python3 scripts/perf/query-perf-counters.py --by-reason

  # Compare two runs (e.g. before vs. after a PR).
  python3 scripts/perf/query-perf-counters.py \\
      --json /tmp/post-fix-pc.json \\
      --baseline /tmp/baseline-pc.json

The tool is intentionally read-only. It never invokes `tsz` or the bench
script — that's the job of `scripts/bench/scale-cliff/run-cliff.sh` (in
timing mode) or a direct attribution-mode invocation recorded in the PR body.
"""

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Optional

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


def load(path: Path) -> dict:
    if not path.exists():
        sys.exit(f"perf-counter JSON not found: {path}")
    with path.open() as f:
        return json.load(f)


def fmt_int(n):
    # type: (Optional[int]) -> str
    if n is None:
        return "null"
    return f"{n:,}"


def by_reason_rows(snap, optional=False):
    # type: (dict, bool) -> list
    rows = snap.get("by_reason")
    if rows is None:
        msg = (
            "JSON is missing `by_reason` — produced before that field was added "
            "(PR exposing per-CheckerCreationReason counters). Re-run the perf "
            "binary with the current tsz to regenerate."
        )
        if optional:
            print(f"(skipping by_reason section: {msg})")
            return []
        sys.exit(msg)
    return rows


def print_summary(snap: dict) -> None:
    delegate = snap["delegate"]
    checker = snap["checker"]
    overlay = snap["overlay"]
    interner = snap["interner"]
    resolver = snap["resolver"]
    print(f"schema_version = {snap['schema_version']}")
    print(f"mode           = {snap['mode']}")
    print(f"enabled        = {snap['enabled']}")
    print()
    print("delegate (cross-arena symbol resolution):")
    de_total = delegate["calls"]
    de_hits = delegate["cache_hits_lib"] + delegate["cache_hits_cross_file"]
    hit_pct = 100.0 * de_hits / de_total if de_total else 0.0
    print(
        f"  calls={fmt_int(de_total)}  hits_lib={fmt_int(delegate['cache_hits_lib'])}  "
        f"hits_cross_file={fmt_int(delegate['cache_hits_cross_file'])}  "
        f"misses={fmt_int(delegate['misses'])}  hit%={hit_pct:.2f}"
    )
    tp_h = delegate.get("cross_file_type_params_cache_hits")
    tp_m = delegate.get("cross_file_type_params_cache_misses")
    if tp_h is not None and tp_m is not None:
        tp_total = tp_h + tp_m
        tp_pct = 100.0 * tp_h / tp_total if tp_total else 0.0
        print(
            f"  cross_file_type_params_cache  hits={fmt_int(tp_h)}  "
            f"misses={fmt_int(tp_m)}  hit%={tp_pct:.2f}"
        )
    miss_causes = snap.get("cross_file_cache_miss_causes")
    if miss_causes is not None:
        total_miss_causes = sum(row["count"] for row in miss_causes)
        print(f"  cross_file_cache_miss_causes total={fmt_int(total_miss_causes)}")
        for row in miss_causes:
            pct = 100.0 * row["count"] / total_miss_causes if total_miss_causes else 0.0
            print(f"    {row['name']:<24} {fmt_int(row['count']):>8} {pct:>6.1f}%")
    decl_residues = snap.get("delegate_declaration_file_miss_residues")
    if decl_residues:
        total_decl_residues = sum(row["count"] for row in decl_residues)
        print(
            f"  declaration_file_miss_residues rows={fmt_int(len(decl_residues))} "
            f"total={fmt_int(total_decl_residues)}"
        )
        for row in decl_residues[:20]:
            file_name = row.get("target_file") or "<unknown>"
            print(
                f"    {row['name']:<36} {row['kind']:<11} "
                f"{fmt_int(row['count']):>5}  {file_name}"
            )
    print()
    print("checker:")
    sc = checker["state_constructed"]
    wpc = checker["with_parent_cache_constructed"]
    fsr = checker["file_session_resets"]
    cot_calls = checker["compute_type_of_symbol_calls"]
    cot_hits = checker["compute_type_of_symbol_cache_hits"]
    cot_total = cot_calls + cot_hits
    cot_hit_pct = 100.0 * cot_hits / cot_total if cot_total else 0.0
    print(
        f"  state_constructed={fmt_int(sc)}  with_parent_cache={fmt_int(wpc)}  "
        f"file_session_resets={fmt_int(fsr)}"
    )
    print(
        f"  compute_type_of_symbol  calls={fmt_int(cot_calls)}  "
        f"hits={fmt_int(cot_hits)}  hit%={cot_hit_pct:.2f}"
    )
    print()
    print("overlay copy:")
    print(
        f"  copy_calls={fmt_int(overlay['copy_calls'])}  "
        f"entries_total={fmt_int(overlay['entries_total'])}  "
        f"entries_max={fmt_int(overlay['entries_max'])}"
    )
    print()
    print("resolver:")
    print(
        f"  lookup_calls={fmt_int(resolver['lookup_calls'])}  "
        f"is_file={fmt_int(resolver['is_file_calls'])}  "
        f"is_dir={fmt_int(resolver['is_dir_calls'])}  "
        f"package_json={fmt_int(resolver['package_json_reads'])}"
    )
    print()
    print("interner:")
    ic = interner["intern_calls"]
    ih = interner["intern_hits"]
    pct = 100.0 * (ih or 0) / (ic or 1) if ic else 0.0
    print(
        f"  intern_calls={fmt_int(ic)}  hits={fmt_int(ih)}  misses={fmt_int(interner['intern_misses'])}  "
        f"hit%={pct:.2f}"
    )
    hist = interner.get("lock_wait_histogram_ns")
    if hist is None:
        print("  lock_wait_histogram_ns = null (build without --features perf-tools)")
    else:
        tot = sum(hist)
        tail = sum(hist[5:])  # >=1ms
        names = ["<100ns", "<1µs", "<10µs", "<100µs", "<1ms", "<10ms", "<100ms", "overflow"]
        bar = " ".join(f"{n}={fmt_int(v)}" for n, v in zip(names, hist))
        tail_pct = 100.0 * tail / tot if tot else 0.0
        print(f"  lock_wait_histogram (total={fmt_int(tot)}  >=1ms={fmt_int(tail)}  tail%={tail_pct:.3f})")
        print(f"    {bar}")


def print_by_reason(snap: dict, optional=False) -> None:
    rows = by_reason_rows(snap, optional=optional)
    if not rows:
        return
    total = sum(r["with_parent_cache_constructed"] for r in rows)
    if total == 0:
        print("by_reason: all-zero (no with_parent_cache constructions on this run)")
        return
    print(
        f"{'reason':<28} {'cons':>8} {'cons%':>7} "
        f"{'ovl_calls':>10} {'ovl_entries':>12} {'max':>6}"
    )
    print("-" * 76)
    rows_sorted = sorted(rows, key=lambda r: -r["with_parent_cache_constructed"])
    for r in rows_sorted:
        cons = r["with_parent_cache_constructed"]
        if cons == 0 and r["overlay_copy_calls"] == 0:
            continue
        pct = 100.0 * cons / total if total else 0.0
        print(
            f"{r['reason']:<28} {fmt_int(cons):>8} {pct:>6.1f}% "
            f"{fmt_int(r['overlay_copy_calls']):>10} "
            f"{fmt_int(r['overlay_copy_entries']):>12} "
            f"{fmt_int(r['overlay_copy_max_entries']):>6}"
        )
    print()
    top = rows_sorted[0]
    top_pct = 100.0 * top["with_parent_cache_constructed"] / total if total else 0.0
    print(
        f"Dominant: {top['reason']} = {fmt_int(top['with_parent_cache_constructed'])} "
        f"({top_pct:.1f}% of with_parent_cache_constructed)"
    )
    t22_candidates = [r for r in rows_sorted if r["reason"] != "TypeEnvironmentCore"]
    if t22_candidates:
        target = t22_candidates[0]
        target_pct = 100.0 * target["with_parent_cache_constructed"] / total if total else 0.0
        print(
            f"Top non-baseline T2.2 target: {target['reason']} = "
            f"{fmt_int(target['with_parent_cache_constructed'])} ({target_pct:.1f}%)"
        )
    print("See `docs/plan/PERFORMANCE_PLAN.md` §6/§7 for the lifetime-split and typed-query playbooks.")


def print_diff(post: dict, base: dict) -> None:
    def delta(a, b, key):
        va = a.get(key)
        vb = b.get(key)
        if va is None or vb is None:
            return f"{key}: post={va} base={vb}"
        d = va - vb
        sign = "+" if d > 0 else ""
        return f"{key}: {fmt_int(vb)} → {fmt_int(va)} ({sign}{fmt_int(d)})"

    print("delegate:")
    for k in (
        "calls",
        "cache_hits_lib",
        "cache_hits_cross_file",
        "misses",
        "max_recursion_depth",
        "cross_file_type_params_cache_hits",
        "cross_file_type_params_cache_misses",
    ):
        print(f"  {delta(post['delegate'], base['delegate'], k)}")
    print()
    print("checker:")
    for k in (
        "state_constructed",
        "with_parent_cache_constructed",
        "file_session_resets",
        "compute_type_of_symbol_calls",
        "compute_type_of_symbol_cache_hits",
    ):
        print(f"  {delta(post['checker'], base['checker'], k)}")
    print()
    post_rows = {r["reason"]: r for r in by_reason_rows(post, optional=True)}
    base_rows = {r["reason"]: r for r in by_reason_rows(base, optional=True)}
    if not post_rows or not base_rows:
        return
    print()
    print("by_reason (with_parent_cache_constructed):")
    for reason in post_rows:
        a = post_rows[reason]["with_parent_cache_constructed"]
        b = base_rows.get(reason, {}).get("with_parent_cache_constructed", 0)
        d = a - b
        if a == 0 and b == 0:
            continue
        sign = "+" if d > 0 else ""
        marker = ""
        if b > 0 and a < b:
            marker = "  ← improved"
        elif a > b > 0:
            marker = "  ← regressed"
        print(f"  {reason:<28} {fmt_int(b):>8} → {fmt_int(a):>8} ({sign}{fmt_int(d)}){marker}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--json",
        type=Path,
        required=True,
        help="path to perf-counter JSON",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="optional baseline JSON to diff `--json` against (printed as before → after deltas)",
    )
    parser.add_argument(
        "--by-reason",
        action="store_true",
        help="print only the per-CheckerCreationReason breakdown (the T2.2 migration lever)",
    )
    args = parser.parse_args()

    snap = load(args.json)
    if args.baseline is not None:
        base = load(args.baseline)
        print(f"baseline: {args.baseline}")
        print(f"current:  {args.json}")
        print()
        print_diff(snap, base)
        return 0

    print(f"perf-counter JSON: {args.json}")
    print()
    if args.by_reason:
        print_by_reason(snap, optional=False)
        return 0
    print_summary(snap)
    print()
    print_by_reason(snap, optional=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
