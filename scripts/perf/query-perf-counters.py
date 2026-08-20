#!/usr/bin/env python3
"""Query retired-compiler attribution JSON without rerunning the bench.

This parser is retained for historical artifacts. The replacement compiler's
`--perf-counters-json` output uses a different minimal schema; do not feed it
to this tool or use this parser to make rewrite attribution claims.

Default mode prints a one-page snapshot, including which
`CheckerCreationReason` accounts for the largest share of
`with_parent_cache_constructed`. That number was a lever in the retired
performance plan. Checked-in attribution runs were removed from the repo; pass
a historical JSON artifact with `--json`.

Usage:
  # Point at a specific retired-schema JSON file.
  python3 scripts/perf/query-perf-counters.py --json /tmp/post-fix-pc.json

  # Per-reason breakdown only, with absolute counts and percent share.
  python3 scripts/perf/query-perf-counters.py --json /tmp/post-fix-pc.json --by-reason

  # Compare two runs (e.g. before vs. after a PR).
  python3 scripts/perf/query-perf-counters.py \\
      --json /tmp/post-fix-pc.json \\
      --baseline /tmp/baseline-pc.json

The tool is intentionally read-only. It never invokes `tsz` or the bench
script. `scripts/bench/scale-cliff/run-cliff.sh` still owns timing mode; a
retired attribution artifact can only be reproduced from a historical checkout.
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


def fmt_pct(part: int, total: int) -> str:
    if total <= 0:
        return "0.0%"
    return f"{100.0 * part / total:.1f}%"


def by_reason_rows(snap, optional=False):
    # type: (dict, bool) -> list
    rows = snap.get("by_reason")
    if rows is None:
        msg = (
            "JSON is missing `by_reason` — produced before that field was added "
            "(PR exposing per-CheckerCreationReason counters). Re-run the perf "
            "retired compiler to regenerate. The replacement compiler does not emit it."
        )
        if optional:
            print(f"(skipping by_reason section: {msg})")
            return []
        sys.exit(msg)
    return rows


RESET_CACHE_FIELDS = (
    ("namespace_member", "namespace_member_entries", "namespace_member_bytes"),
    ("export_equals", "export_equals_entries", "export_equals_bytes"),
    ("nested_namespace", "nested_namespace_entries", "nested_namespace_bytes"),
    ("lowering_entity_name", "lowering_entity_name_entries", "lowering_entity_name_bytes"),
    ("env_eval", "env_eval_entries", "env_eval_bytes"),
)


def reset_cache_value(checker: dict, suffix: str) -> Optional[int]:
    return checker.get(f"file_session_reset_{suffix}_max")


def reset_cache_rows(checker: dict, include_unattributed=False) -> list:
    rows = []
    for name, entries_suffix, bytes_suffix in RESET_CACHE_FIELDS:
        entries = reset_cache_value(checker, entries_suffix)
        size = reset_cache_value(checker, bytes_suffix)
        if entries is None and size is None:
            continue
        rows.append(
            {
                "name": name,
                "entries": entries or 0,
                "bytes": size or 0,
            }
        )
    if include_unattributed:
        total_bytes = checker.get("file_session_reset_cache_bytes_max")
        if total_bytes is not None:
            known_bytes = sum(row["bytes"] for row in rows)
            unattributed_bytes = total_bytes - known_bytes
            if unattributed_bytes > 0:
                rows.append(
                    {
                        "name": "unattributed",
                        "entries": 0,
                        "bytes": unattributed_bytes,
                    }
                )
    return rows


def reset_cache_total_bytes(checker: dict, rows: list) -> int:
    total = checker.get("file_session_reset_cache_bytes_max")
    if total is not None:
        return total
    return sum(row["bytes"] for row in rows)


def dominant_reset_cache_row(rows: list) -> Optional[dict]:
    nonzero_rows = [row for row in rows if row["entries"] != 0 or row["bytes"] != 0]
    if not nonzero_rows:
        return None
    return sorted(nonzero_rows, key=lambda row: (-row["bytes"], row["name"]))[0]


def reset_cache_attributed_bytes(rows: list) -> int:
    return sum(row["bytes"] for row in rows if row["name"] != "unattributed")


def print_reset_cache_high_water(checker: dict, indent: str = "  ") -> None:
    entries = checker.get("file_session_reset_cache_entries_max")
    size = checker.get("file_session_reset_cache_bytes_max")
    rows = reset_cache_rows(checker, include_unattributed=True)
    if entries is None and size is None and not rows:
        return

    print(f"{indent}file-session reset cache high-water:")
    print(f"{indent}  total_entries={fmt_int(entries)}  total_bytes={fmt_int(size)}")
    if not rows:
        return

    rows_by_bytes = sorted(rows, key=lambda row: (-row["bytes"], row["name"]))
    total_for_share = reset_cache_total_bytes(checker, rows)
    dominant = rows_by_bytes[0]
    for row in rows_by_bytes:
        if row["entries"] == 0 and row["bytes"] == 0:
            continue
        print(
            f"{indent}  {row['name']:<22} "
            f"entries={fmt_int(row['entries']):>8}  "
            f"bytes={fmt_int(row['bytes']):>10}  "
            f"byte_share={fmt_pct(row['bytes'], total_for_share):>6}"
        )
    print(
        f"{indent}  dominant={dominant['name']} "
        f"bytes={fmt_int(dominant['bytes'])} "
        f"byte_share={fmt_pct(dominant['bytes'], total_for_share)}"
    )


def print_summary(snap: dict) -> None:
    delegate = snap["delegate"]
    checker = snap["checker"]
    overlay = snap["overlay"]
    interner = snap["interner"]
    materialization = snap.get("solver_materialization", {})
    shared_instantiation = snap.get("shared_instantiation_cache", {})
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
    print_reset_cache_high_water(checker)
    print(
        f"  compute_type_of_symbol  calls={fmt_int(cot_calls)}  "
        f"hits={fmt_int(cot_hits)}  hit%={cot_hit_pct:.2f}"
    )
    slow_files = snap.get("slow_check_file_timings") or []
    if slow_files:
        print("  slowest semantic check files:")
        for row in slow_files[:10]:
            print(
                f"    {row['elapsed_ms']:>8.2f} ms  "
                f"diags={fmt_int(row.get('diagnostics', 0)):>4}  {row['file']}"
            )
    slow_statements = snap.get("slow_check_statement_timings") or []
    if slow_statements:
        print("  slowest semantic check statements:")
        for row in slow_statements[:10]:
            print(
                f"    {row['elapsed_ms']:>8.2f} ms  "
                f"kind={fmt_int(row.get('kind')):>4}  "
                f"span={fmt_int(row.get('pos'))}..{fmt_int(row.get('end'))}  "
                f"{row['file']}"
            )
    slow_alias_phases = snap.get("slow_type_alias_check_timings") or []
    if slow_alias_phases:
        print("  slowest type alias check phases:")
        for row in slow_alias_phases[:10]:
            print(
                f"    {row['elapsed_ms']:>8.2f} ms  "
                f"phase={row.get('phase', '<unknown>'):<24}  "
                f"span={fmt_int(row.get('pos'))}..{fmt_int(row.get('end'))}  "
                f"{row.get('name', '<anonymous>')}  {row['file']}"
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
    if materialization:
        print()
        print("solver materialization:")
        print(
            f"  union_reductions={fmt_int(materialization['union_subtype_reduction_calls'])}  "
            f"members_total={fmt_int(materialization['union_subtype_reduction_members_total'])}  "
            f"members_max={fmt_int(materialization['union_subtype_reduction_members_max'])}"
        )
        print(
            f"  pairwise_budget={fmt_int(materialization['union_subtype_reduction_pairwise_budget_total'])}  "
            f"shallow_checks={fmt_int(materialization['union_subtype_reduction_shallow_checks'])}"
        )
        print(
            f"  property_walks={fmt_int(materialization['property_instantiation_walks'])}  "
            f"properties_total={fmt_int(materialization['property_instantiation_properties_total'])}  "
            f"properties_max={fmt_int(materialization['property_instantiation_properties_max'])}  "
            f"changed={fmt_int(materialization['property_instantiation_changed'])}"
        )
    if shared_instantiation:
        print()
        print("opt-in shared instantiation caches:")
        print(
            f"  application_eval hits={fmt_int(shared_instantiation['application_eval_shared_hits'])}  "
            f"misses={fmt_int(shared_instantiation['application_eval_shared_misses'])}  "
            f"inserts={fmt_int(shared_instantiation['application_eval_shared_inserts'])}  "
            f"bypasses={fmt_int(shared_instantiation['application_eval_shared_bypasses'])}"
        )
        print(
            f"  instantiation    hits={fmt_int(shared_instantiation['instantiation_shared_hits'])}  "
            f"misses={fmt_int(shared_instantiation['instantiation_shared_misses'])}  "
            f"inserts={fmt_int(shared_instantiation['instantiation_shared_inserts'])}  "
            f"bypasses={fmt_int(shared_instantiation['instantiation_shared_bypasses'])}"
        )


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
    print("Historical schema only; replacement-compiler attribution is not yet available.")


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
        "file_session_reset_cache_entries_max",
        "file_session_reset_cache_bytes_max",
        "file_session_reset_namespace_member_entries_max",
        "file_session_reset_namespace_member_bytes_max",
        "file_session_reset_export_equals_entries_max",
        "file_session_reset_export_equals_bytes_max",
        "file_session_reset_nested_namespace_entries_max",
        "file_session_reset_nested_namespace_bytes_max",
        "file_session_reset_lowering_entity_name_entries_max",
        "file_session_reset_lowering_entity_name_bytes_max",
        "file_session_reset_env_eval_entries_max",
        "file_session_reset_env_eval_bytes_max",
        "compute_type_of_symbol_calls",
        "compute_type_of_symbol_cache_hits",
    ):
        print(f"  {delta(post['checker'], base['checker'], k)}")
    post_rows = reset_cache_rows(post["checker"], include_unattributed=True)
    base_rows = reset_cache_rows(base["checker"], include_unattributed=True)
    if post_rows or base_rows:
        post_by_name = {row["name"]: row for row in post_rows}
        base_by_name = {row["name"]: row for row in base_rows}
        print("  reset-cache high-water by family:")
        post_total = reset_cache_total_bytes(post["checker"], post_rows)
        base_total = reset_cache_total_bytes(base["checker"], base_rows)
        for name in sorted(set(post_by_name) | set(base_by_name)):
            post_row = post_by_name.get(name, {"entries": 0, "bytes": 0})
            base_row = base_by_name.get(name, {"entries": 0, "bytes": 0})
            entries_delta = post_row["entries"] - base_row["entries"]
            bytes_delta = post_row["bytes"] - base_row["bytes"]
            entries_sign = "+" if entries_delta > 0 else ""
            bytes_sign = "+" if bytes_delta > 0 else ""
            print(
                f"    {name:<22} "
                f"entries {fmt_int(base_row['entries']):>8} → {fmt_int(post_row['entries']):>8} "
                f"({entries_sign}{fmt_int(entries_delta)})  "
                f"bytes {fmt_int(base_row['bytes']):>10} → {fmt_int(post_row['bytes']):>10} "
                f"({bytes_sign}{fmt_int(bytes_delta)})  "
                f"share {fmt_pct(base_row['bytes'], base_total):>6} → "
                f"{fmt_pct(post_row['bytes'], post_total):>6}"
            )
        base_attributed = reset_cache_attributed_bytes(base_rows)
        post_attributed = reset_cache_attributed_bytes(post_rows)
        print(
            "    attributed byte coverage "
            f"{fmt_int(base_attributed)}/{fmt_int(base_total)} "
            f"({fmt_pct(base_attributed, base_total)}) → "
            f"{fmt_int(post_attributed)}/{fmt_int(post_total)} "
            f"({fmt_pct(post_attributed, post_total)})"
        )
        base_dominant = dominant_reset_cache_row(base_rows)
        post_dominant = dominant_reset_cache_row(post_rows)
        if base_dominant or post_dominant:
            base_name = base_dominant["name"] if base_dominant else "<none>"
            post_name = post_dominant["name"] if post_dominant else "<none>"
            base_bytes = base_dominant["bytes"] if base_dominant else 0
            post_bytes = post_dominant["bytes"] if post_dominant else 0
            print(
                "    dominant retained family "
                f"{base_name} ({fmt_int(base_bytes)}, {fmt_pct(base_bytes, base_total)}) → "
                f"{post_name} ({fmt_int(post_bytes)}, {fmt_pct(post_bytes, post_total)})"
            )
    post_slow = post.get("slow_check_file_timings") or []
    base_slow = base.get("slow_check_file_timings") or []
    if post_slow or base_slow:
        print("  slowest semantic check file:")
        if base_slow:
            b = base_slow[0]
            print(f"    baseline {b['elapsed_ms']:.2f} ms  {b['file']}")
        if post_slow:
            a = post_slow[0]
            print(f"    current  {a['elapsed_ms']:.2f} ms  {a['file']}")
    post_stmt = post.get("slow_check_statement_timings") or []
    base_stmt = base.get("slow_check_statement_timings") or []
    if post_stmt or base_stmt:
        print("  slowest semantic check statement:")
        if base_stmt:
            b = base_stmt[0]
            print(
                f"    baseline {b['elapsed_ms']:.2f} ms  "
                f"kind={b.get('kind')} span={b.get('pos')}..{b.get('end')} {b['file']}"
            )
        if post_stmt:
            a = post_stmt[0]
            print(
                f"    current  {a['elapsed_ms']:.2f} ms  "
                f"kind={a.get('kind')} span={a.get('pos')}..{a.get('end')} {a['file']}"
            )
    post_alias = post.get("slow_type_alias_check_timings") or []
    base_alias = base.get("slow_type_alias_check_timings") or []
    if post_alias or base_alias:
        print("  slowest type alias check phase:")
        if base_alias:
            b = base_alias[0]
            print(
                f"    baseline {b['elapsed_ms']:.2f} ms  "
                f"phase={b.get('phase')} {b.get('name')} {b['file']}"
            )
        if post_alias:
            a = post_alias[0]
            print(
                f"    current  {a['elapsed_ms']:.2f} ms  "
                f"phase={a.get('phase')} {a.get('name')} {a['file']}"
            )
    print()
    if post.get("solver_materialization") or base.get("solver_materialization"):
        print("solver materialization:")
        post_mat = post.get("solver_materialization", {})
        base_mat = base.get("solver_materialization", {})
        for k in (
            "union_subtype_reduction_calls",
            "union_subtype_reduction_members_total",
            "union_subtype_reduction_members_max",
            "union_subtype_reduction_pairwise_budget_total",
            "union_subtype_reduction_shallow_checks",
            "property_instantiation_walks",
            "property_instantiation_properties_total",
            "property_instantiation_properties_max",
            "property_instantiation_changed",
        ):
            print(f"  {delta(post_mat, base_mat, k)}")
        print()
    if post.get("shared_instantiation_cache") or base.get("shared_instantiation_cache"):
        print("opt-in shared instantiation caches:")
        post_shared = post.get("shared_instantiation_cache", {})
        base_shared = base.get("shared_instantiation_cache", {})
        for k in (
            "application_eval_shared_hits",
            "application_eval_shared_misses",
            "application_eval_shared_inserts",
            "application_eval_shared_bypasses",
            "instantiation_shared_hits",
            "instantiation_shared_misses",
            "instantiation_shared_inserts",
            "instantiation_shared_bypasses",
        ):
            print(f"  {delta(post_shared, base_shared, k)}")
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
