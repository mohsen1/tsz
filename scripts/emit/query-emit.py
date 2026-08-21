#!/usr/bin/env python3
"""Query emit test results offline without re-running tests.

Reads from scripts/emit/emit-detail.json (produced by the emit runner with --json-out).

Usage:
  # Show overview
  python3 scripts/emit/query-emit.py

  # Top failure messages
  python3 scripts/emit/query-emit.py --top-errors

  # Failure-family dashboard
  python3 scripts/emit/query-emit.py --families

  # Machine-readable failure-family dashboard
  python3 scripts/emit/query-emit.py --families-json

  # Include historical family rows even when emit-detail.json is stale
  python3 scripts/emit/query-emit.py --families --include-stale-detail

  # Filter by substring in test name
  python3 scripts/emit/query-emit.py --filter class

  # Show only JS failures or DTS failures
  python3 scripts/emit/query-emit.py --js-failures
  python3 scripts/emit/query-emit.py --dts-failures

  # Tests closest to passing (e.g., only DTS failing)
  python3 scripts/emit/query-emit.py --close

  # Filter by status
  python3 scripts/emit/query-emit.py --status fail
  python3 scripts/emit/query-emit.py --status timeout

  # Export paths for piping
  python3 scripts/emit/query-emit.py --js-failures --paths-only
"""

import os
import sys
import argparse
import re
import json
import hashlib
from collections import Counter
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from lib.query_snapshot import load_snapshot, print_top_counter, print_truncated_more

DETAIL_FILE = Path(__file__).parent / "emit-detail.json"
SNAPSHOT_FILE = Path(__file__).parent / "emit-snapshot.json"
ROOT_DIR = Path(__file__).resolve().parents[2]
README_FILE = ROOT_DIR / "README.md"


JS_FAMILY_RULES = [
    ("resource-management lowering", ("using", "dispose", "resource")),
    (
        "async/await/generator lowering",
        ("async", "await", "generator", "yield", "forawait"),
    ),
    (
        "class/private/accessor/decorator lowering",
        ("class", "private", "accessor", "decorator", "super", "staticblock"),
    ),
    (
        "module/import/export emit",
        ("module", "import", "export", "commonjs", "amd", "umd", "esmodule"),
    ),
    (
        "block-scoping/hoisting emit",
        ("blockscoped", "capturedlet", "let", "const", "tdz", "usebeforedef"),
    ),
    ("destructuring/spread/rest emit", ("destruct", "spread", "rest", "bindingpattern")),
    ("enum/namespace emit", ("enum", "namespace", "internalmodule", "declarationmerging")),
    ("jsx/react emit", ("jsx", "react", "tsx")),
    (
        "loop/control-flow emit",
        ("forof", "forin", "switch", "try", "catch", "break", "continue", "label"),
    ),
    ("literal/template emit", ("template", "literal", "regexp", "numericseparator")),
    ("comments/source-map emit", ("comment", "sourcemap", "source map", "source-map")),
    # The rules below match by substring against test name + path + error text and are
    # triage aids over test metadata only, not compiler-behavior decisions.
    (
        "parser/recovery emit",
        ("parser", "parsebigint", "parseinvalid", "parseassert", "parseerror", "skippedtoken"),
    ),
    ("type-guard emit", ("typeguard", "typeguards", "typepredicate")),
    (
        "optional-chain/nullish emit",
        ("optionalchain", "optionalchaining", "chain", "nullishcoalesc"),
    ),
    ("unicode/identifier-encoding emit", ("unicode", "unicodeescape")),
    ("reserved-word emit", ("reservedword", "reservedname")),
    ("js-file/plain-js emit", ("jsfile", "jsdeclaration", "plainjsgrammar")),
    ("new-target emit", ("newtarget",)),
    ("tslib/helper emit", ("tslib",)),
    ("jsdoc-type emit", ("jsdoc",)),
]


DTS_FAMILY_RULES = [
    (
        "module/declaration merging",
        (
            "moduleaugmentation",
            "augmentation",
            "declarationmerging",
            "ambientmodule",
            "symlink",
            "moduledecl",
            "nodemodule",
        ),
    ),
    (
        "import/export/nameability",
        ("import", "export", "alias", "qualified", "externalmodules", "specifier"),
    ),
    (
        "jsdoc/javascript declarations",
        ("jsdoc", "javascript", "salsa", "typedef", "checkjs", "jsfile"),
    ),
    (
        "class/private/accessor declarations",
        ("class", "private", "accessor", "constructor", "extends", "implements", "privacy"),
    ),
    (
        "generic/type-display declarations",
        (
            "generic",
            "conditional",
            "mapped",
            "infer",
            "typeparameter",
            "recursive",
            "typeof",
            "keyof",
            "indexed",
            "indexsignature",
            "signature",
            "template",
            "variadic",
            "tuple",
            "stringliteral",
            "spread",
            "never",
            "noimplicit",
        ),
    ),
    (
        "isolated-declaration constraints",
        ("isolateddeclaration", "isolated declaration"),
    ),
    ("enum/namespace declarations", ("enum", "namespace", "internalmodule", "declarationmerging")),
    ("jsx/react declarations", ("jsx", "react")),
    ("ambient/lib declarations", ("ambient", "global", "lib", "defaultlib")),
    ("type-guard declarations", ("typeguard", "typeguards")),
    ("unique-symbol declarations", ("uniquesymbol",)),
]


TERMINAL_STATUSES = {"fail", "timeout", "unsupported", "crash", "incomplete"}


def is_terminal_status(status):
    return status in TERMINAL_STATUSES


def load_detail():
    return load_snapshot(DETAIL_FILE, "Run: ./scripts/emit/run.sh --json-out")


def load_emit_snapshot(path=SNAPSHOT_FILE):
    try:
        with open(path) as f:
            return json.load(f)
    except OSError:
        return None


def emit_summary(data):
    summary = data.get("summary", {})
    return {
        "jsPass": summary.get("jsPass"),
        "jsTotal": summary.get("jsTotal"),
        "dtsPass": summary.get("dtsPass"),
        "dtsTotal": summary.get("dtsTotal"),
    }


def emit_summary_from_readme_text(text):
    section = text.split("<!-- EMIT_START -->", 1)
    if len(section) != 2:
        return None
    section = section[1].split("<!-- EMIT_END -->", 1)[0]

    summary = {}
    for line in section.splitlines():
        if "JavaScript" in line:
            prefix = "js"
        elif "Declaration" in line:
            prefix = "dts"
        else:
            continue

        match = re.search(r"\(([\d,]+)\s*/\s*([\d,]+)", line)
        if not match:
            continue
        summary[f"{prefix}Pass"] = int(match.group(1).replace(",", ""))
        summary[f"{prefix}Total"] = int(match.group(2).replace(",", ""))

    required = {"jsPass", "jsTotal", "dtsPass", "dtsTotal"}
    return summary if required.issubset(summary) else None


def emit_snapshot_summary(snapshot):
    if not snapshot:
        return None
    summary = snapshot.get("summary")
    if not summary:
        return None
    required = {"jsPass", "jsTotal", "dtsPass", "dtsTotal"}
    return emit_summary({"summary": summary}) if required.issubset(summary) else None


def emit_summary_from_readme(path=README_FILE):
    try:
        return emit_summary_from_readme_text(path.read_text())
    except OSError:
        return None


def emit_detail_row_summary(data):
    results = data.get("results")
    if not isinstance(results, list):
        return None

    summary = {
        "jsPass": 0,
        "jsFail": 0,
        "jsSkip": 0,
        "jsTimeout": 0,
        "dtsPass": 0,
        "dtsFail": 0,
        "dtsSkip": 0,
    }
    for result in results:
        js_status = result.get("jsStatus")
        dts_status = result.get("dtsStatus")
        if js_status == "pass":
            summary["jsPass"] += 1
        elif is_terminal_status(js_status):
            summary["jsFail"] += 1
            if js_status == "timeout":
                summary["jsTimeout"] += 1
        else:
            summary["jsSkip"] += 1

        if dts_status == "pass":
            summary["dtsPass"] += 1
        elif is_terminal_status(dts_status):
            summary["dtsFail"] += 1
        else:
            summary["dtsSkip"] += 1

    summary["jsTotal"] = summary["jsPass"] + summary["jsFail"]
    summary["dtsTotal"] = summary["dtsPass"] + summary["dtsFail"]
    return summary


def emit_detail_rows_match_summary(data):
    row_summary = emit_detail_row_summary(data)
    detail_summary = data.get("summary", {})
    if row_summary is None:
        return False

    keys = (
        "jsPass",
        "jsFail",
        "jsSkip",
        "jsTimeout",
        "jsTotal",
        "dtsPass",
        "dtsFail",
        "dtsSkip",
        "dtsTotal",
    )
    return all(row_summary.get(key) == detail_summary.get(key) for key in keys)


def emit_detail_row_fingerprint(data):
    results = data.get("results")
    if not isinstance(results, list):
        return None

    rows = []
    for result in results:
        rows.append({
            "artifactState": result.get("artifactState"),
            "baselineFile": result.get("baselineFile"),
            "dtsError": result.get("dtsError"),
            "dtsStatus": result.get("dtsStatus"),
            "jsError": result.get("jsError"),
            "jsStatus": result.get("jsStatus"),
            "name": result.get("name"),
            "testPath": result.get("testPath"),
        })
    encoded = json.dumps(
        sorted(rows, key=lambda row: (
            row.get("name") or "",
            row.get("baselineFile") or "",
            row.get("testPath") or "",
        )),
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def emit_row_freshness(data, public_summary):
    snapshot = load_emit_snapshot()
    snapshot_summary = emit_snapshot_summary(snapshot)
    detail_summary = emit_summary(data)
    row_fingerprint = emit_detail_row_fingerprint(data)
    snapshot_fingerprint = (snapshot or {}).get("detailFingerprint")
    expected_count = (snapshot or {}).get("detailResultCount")
    actual_count = len(data.get("results", [])) if isinstance(data.get("results"), list) else None
    rows_match_summary = emit_detail_rows_match_summary(data)

    proven = (
        rows_match_summary
        and row_fingerprint is not None
        and row_fingerprint == snapshot_fingerprint
        and expected_count == actual_count
        and snapshot_summary == detail_summary
        and snapshot_summary == public_summary
    )

    if proven:
        evidence = "emit-snapshot-detail-fingerprint"
    elif snapshot_fingerprint is None:
        evidence = "missing-snapshot-fingerprint"
    elif row_fingerprint != snapshot_fingerprint:
        evidence = "snapshot-fingerprint-mismatch"
    elif not rows_match_summary:
        evidence = "detail-rows-summary-mismatch"
    elif expected_count != actual_count:
        evidence = "detail-result-count-mismatch"
    elif snapshot_summary != detail_summary:
        evidence = "snapshot-summary-mismatch"
    elif snapshot_summary != public_summary:
        evidence = "snapshot-public-summary-mismatch"
    else:
        evidence = "unproven"

    return {
        "proven": proven,
        "evidence": evidence,
        "detailFingerprint": row_fingerprint,
        "snapshotFingerprint": snapshot_fingerprint,
        "detailResultCount": actual_count,
        "snapshotDetailResultCount": expected_count,
        "detailRowsMatchSummary": rows_match_summary,
    }


def emit_freshness_note(detail_summary, public_summary):
    status = emit_freshness_status(detail_summary, public_summary)
    if status["state"] != "stale":
        return None

    return (
        "Note: README/public emit aggregate is newer than "
        "scripts/emit/emit-detail.json "
        f"(JS {public_summary['jsPass']:,}/{public_summary['jsTotal']:,} vs "
        f"{detail_summary['jsPass']:,}/{detail_summary['jsTotal']:,}; "
        f"DTS {public_summary['dtsPass']:,}/{public_summary['dtsTotal']:,} vs "
        f"{detail_summary['dtsPass']:,}/{detail_summary['dtsTotal']:,}). "
        f"Pass delta: JS +{status['jsPassDelta']:,}, DTS +{status['dtsPassDelta']:,}. "
        "Failure-family rows below are historical checked-detail triage only; "
        "do not cite them as the current public remaining set until "
        "emit-detail.json is refreshed."
    )


def emit_freshness_status(detail_summary, public_summary):
    if not detail_summary:
        return {"state": "missing-detail"}
    if not public_summary:
        return {"state": "unknown-public"}

    same_domain = (
        detail_summary.get("jsTotal") == public_summary.get("jsTotal")
        and detail_summary.get("dtsTotal") == public_summary.get("dtsTotal")
    )
    status = {
        "jsPassDelta": public_summary.get("jsPass", 0) - detail_summary.get("jsPass", 0),
        "dtsPassDelta": public_summary.get("dtsPass", 0) - detail_summary.get("dtsPass", 0),
        "jsTotalDelta": public_summary.get("jsTotal", 0) - detail_summary.get("jsTotal", 0),
        "dtsTotalDelta": public_summary.get("dtsTotal", 0) - detail_summary.get("dtsTotal", 0),
    }
    if not same_domain:
        return {"state": "different-domain", **status}
    if status["jsPassDelta"] > 0 or status["dtsPassDelta"] > 0:
        return {"state": "stale", **status}
    if status["jsPassDelta"] < 0 or status["dtsPassDelta"] < 0:
        return {"state": "detail-ahead", **status}
    return {"state": "aggregate-match", **status}


def emit_freshness_report(detail_summary, public_summary, detail_data=None):
    status = emit_freshness_status(detail_summary, public_summary)
    aggregate_matches_public = status["state"] == "aggregate-match"
    row_freshness = (
        emit_row_freshness(detail_data, public_summary)
        if detail_data is not None and aggregate_matches_public
        else None
    )
    row_freshness_proven = bool(row_freshness and row_freshness["proven"])
    return {
        "state": status["state"],
        "detailSummary": detail_summary,
        "publicSummary": public_summary,
        "detailIsCurrent": emit_detail_is_current(status),
        "detailAggregateMatchesPublic": aggregate_matches_public,
        "rowFreshnessProven": row_freshness_proven,
        "rowFreshnessEvidence": (
            row_freshness["evidence"]
            if row_freshness is not None
            else ("aggregate-only" if aggregate_matches_public else status["state"])
        ),
        "rowFreshness": row_freshness,
        "jsPassDelta": status.get("jsPassDelta"),
        "dtsPassDelta": status.get("dtsPassDelta"),
        "jsTotalDelta": status.get("jsTotalDelta"),
        "dtsTotalDelta": status.get("dtsTotalDelta"),
        "message": emit_freshness_status_line_from_report(status, row_freshness),
    }


def emit_freshness_status_line(data):
    detail_summary = emit_summary(data)
    public_summary = emit_summary_from_readme()
    status = emit_freshness_status(detail_summary, public_summary)
    row_freshness = (
        emit_row_freshness(data, public_summary)
        if status["state"] == "aggregate-match"
        else None
    )
    return emit_freshness_status_line_from_report(status, row_freshness)


def emit_freshness_status_line_from_report(status, row_freshness):
    if status["state"] == "aggregate-match" and row_freshness and row_freshness["proven"]:
        return (
            "Emit detail freshness: row-proven "
            "(README/public aggregate matches checked detail; "
            "emit-snapshot fingerprint matches detail rows)."
        )
    return emit_freshness_status_line_from_status(status)


def emit_freshness_status_line_from_status(status):
    state = status["state"]
    if state == "stale":
        return (
            "Emit detail freshness: stale "
            f"(README/public ahead by JS +{status['jsPassDelta']:,} pass, "
            f"DTS +{status['dtsPassDelta']:,} pass over matching totals)."
        )
    if state == "aggregate-match":
        return (
            "Emit detail freshness: aggregate-match "
            "(README/public aggregate matches checked detail; "
            "per-row freshness is not proven)."
        )
    if state == "detail-ahead":
        return (
            "Emit detail freshness: detail-ahead "
            f"(checked detail exceeds README/public by JS {-status['jsPassDelta']:,} pass, "
            f"DTS {-status['dtsPassDelta']:,} pass over matching totals)."
        )
    if state == "different-domain":
        return (
            "Emit detail freshness: incomparable "
            f"(README/public totals differ by JS {status['jsTotalDelta']:+,}, "
            f"DTS {status['dtsTotalDelta']:+,})."
        )
    return f"Emit detail freshness: {state}."


def emit_detail_is_current(freshness_status):
    return freshness_status.get("state") == "aggregate-match"


def emit_pass_rate(summary, prefix):
    passed = summary.get(f"{prefix}Pass")
    total = summary.get(f"{prefix}Total")
    if passed is None or total is None or total <= 0:
        return "N/A"
    return f"{(passed / total) * 100:.1f}"


def emit_headline_summary(detail_summary, public_summary):
    status = emit_freshness_status(detail_summary, public_summary)
    if status["state"] == "stale":
        return public_summary, "README/public aggregate"
    return detail_summary, "checked detail"


def emit_remaining_failures(summary, surface):
    prefix = "js" if surface == "js" else "dts"
    passed = summary.get(f"{prefix}Pass")
    total = summary.get(f"{prefix}Total")
    if passed is None or total is None:
        return None
    return total - passed


def failure_family_surface_heading(surface, title, detail_total, detail_summary, public_summary):
    status = emit_freshness_status(detail_summary, public_summary)
    if status["state"] != "stale":
        return f"{title}: {detail_total} failures/timeouts"

    public_remaining = emit_remaining_failures(public_summary, surface)
    detail_remaining = emit_remaining_failures(detail_summary, surface)
    if public_remaining is None or detail_remaining is None:
        return f"{title} checked-detail: {detail_total} failures/timeouts"

    return (
        f"{title} STALE checked-detail triage: {detail_total} failures/timeouts "
        f"(public aggregate remaining: {public_remaining:,}; "
        f"detail aggregate remaining: {detail_remaining:,})"
    )


def print_stale_failure_family_guard(detail_summary, public_summary):
    print(emit_freshness_status_line_from_status(
        emit_freshness_status(detail_summary, public_summary)
    ))
    print(
        "Failure-family rows are suppressed because scripts/emit/emit-detail.json "
        "does not match the README/public aggregate."
    )
    print()
    for surface, title in (("js", "JavaScript"), ("dts", "Declaration")):
        public_remaining = emit_remaining_failures(public_summary, surface)
        detail_remaining = emit_remaining_failures(detail_summary, surface)
        if public_remaining is None or detail_remaining is None:
            continue
        print(
            f"{title}: public aggregate remaining {public_remaining:,}; "
            f"checked-detail remaining {detail_remaining:,}"
        )
    print()
    print(
        "Refresh with `./scripts/emit/run.sh --json-out`, or pass "
        "`--include-stale-detail` to view historical checked-detail triage."
    )


def print_emit_freshness_status(data):
    print(emit_freshness_status_line(data))


def print_emit_freshness_json(data):
    print(
        json.dumps(
            emit_freshness_report(emit_summary(data), emit_summary_from_readme(), data),
            sort_keys=True,
        )
    )


def print_emit_freshness_note(data):
    note = emit_freshness_note(emit_summary(data), emit_summary_from_readme())
    if note:
        print(note)
        print()


def show_overview(data):
    s = data["summary"]
    public_summary = emit_summary_from_readme()
    headline_summary, headline_source = emit_headline_summary(emit_summary(data), public_summary)
    print(f"Emit Test Results")
    print_emit_freshness_note(data)
    print(f"  Source: {headline_source}")
    print(
        "  JavaScript: "
        f"{headline_summary['jsPass']}/{headline_summary['jsTotal']} "
        f"({emit_pass_rate(headline_summary, 'js')}%)"
    )
    print(
        "  Declaration: "
        f"{headline_summary['dtsPass']}/{headline_summary['dtsTotal']} "
        f"({emit_pass_rate(headline_summary, 'dts')}%)"
    )
    if headline_source != "checked detail":
        print(
            "  Checked-detail JavaScript: "
            f"{s['jsPass']}/{s['jsTotal']} ({s['jsPassRate']}%)"
        )
        print(
            "  Checked-detail Declaration: "
            f"{s['dtsPass']}/{s['dtsTotal']} ({s['dtsPassRate']}%)"
        )
    detail_label = "Checked-detail " if headline_source != "checked detail" else ""
    print()

    results = data["results"]
    js_fails = [r for r in results if is_terminal_status(r["jsStatus"])]
    dts_fails = [r for r in results if is_terminal_status(r["dtsStatus"])]
    timeouts = [r for r in results if r["jsStatus"] == "timeout" or r["dtsStatus"] == "timeout"]

    print(f"  {detail_label}JS failures: {len(js_fails)}")
    print(f"  {detail_label}DTS failures: {len(dts_fails)}")
    print(f"  {detail_label}Timeouts: {len(timeouts)}")
    print()

    # JS-pass but DTS-fail (close to full pass)
    js_pass_dts_fail = [
        r for r in results
        if r["jsStatus"] == "pass" and is_terminal_status(r["dtsStatus"])
    ]
    print(
        f"  {detail_label}JS pass + DTS fail (close to full pass): "
        f"{len(js_pass_dts_fail)}"
    )

    # DTS-pass but JS-fail
    dts_pass_js_fail = [
        r for r in results
        if r["dtsStatus"] == "pass" and is_terminal_status(r["jsStatus"])
    ]
    print(f"  {detail_label}DTS pass + JS fail: {len(dts_pass_js_fail)}")
    print()

    # Top error messages
    print("Top JS failure messages:")
    js_error_counter = Counter()
    for r in js_fails:
        msg = r.get("jsError", "unknown")
        # Normalize to first 80 chars
        js_error_counter[msg[:80]] += 1
    print_top_counter(js_error_counter, 10)
    print()

    print("Top DTS failure messages:")
    dts_error_counter = Counter()
    for r in dts_fails:
        msg = r.get("dtsError", "unknown")
        dts_error_counter[msg[:80]] += 1
    print_top_counter(dts_error_counter, 10)


def show_js_failures(data, top=40, paths_only=False):
    results = data["results"]
    fails = [r for r in results if is_terminal_status(r["jsStatus"])]
    fails.sort(key=lambda r: r["name"])

    if paths_only:
        for r in fails:
            print(r["name"])
        return

    print(f"JS failures: {len(fails)}")
    for r in fails[:top]:
        err = r.get("jsError", "")[:80]
        print(f"  {r['name']}  {err}")
    print_truncated_more(fails, top)


def show_dts_failures(data, top=40, paths_only=False):
    results = data["results"]
    fails = [r for r in results if is_terminal_status(r["dtsStatus"])]
    fails.sort(key=lambda r: r["name"])

    if paths_only:
        for r in fails:
            print(r["name"])
        return

    print(f"DTS failures: {len(fails)}")
    for r in fails[:top]:
        err = r.get("dtsError", "")[:80]
        print(f"  {r['name']}  {err}")
    print_truncated_more(fails, top)


def filter_data_by_name(data, pattern):
    if not pattern:
        return data

    lower = pattern.lower()
    filtered = dict(data)
    filtered["results"] = [r for r in data["results"] if lower in r["name"].lower()]
    return filtered


def show_top_errors(data, top=20):
    results = data["results"]

    print("Top JS error messages:")
    js_counter = Counter()
    for r in results:
        if is_terminal_status(r["jsStatus"]) and r.get("jsError"):
            js_counter[r["jsError"][:100]] += 1
    print_top_counter(js_counter, top)

    print()
    print("Top DTS error messages:")
    dts_counter = Counter()
    for r in results:
        if is_terminal_status(r["dtsStatus"]) and r.get("dtsError"):
            dts_counter[r["dtsError"][:100]] += 1
    print_top_counter(dts_counter, top)


def failure_haystack(result, surface):
    fields = [
        result.get("name", ""),
        result.get("testPath", ""),
        result.get("baselineFile", ""),
    ]
    if surface == "js":
        fields.append(result.get("jsError", ""))
    else:
        fields.append(result.get("dtsError", ""))
    return " ".join(fields).lower()


def classify_failure(result, surface):
    rules = JS_FAMILY_RULES if surface == "js" else DTS_FAMILY_RULES
    haystack = failure_haystack(result, surface)
    for family, needles in rules:
        if any(needle in haystack for needle in needles):
            return family
    return "other"


def collect_failures_by_family(data, surface):
    status_key = "jsStatus" if surface == "js" else "dtsStatus"
    failures = [r for r in data["results"] if is_terminal_status(r.get(status_key))]
    families = {}
    for result in failures:
        family = classify_failure(result, surface)
        families.setdefault(family, []).append(result)
    return families


def family_rows(data, surface, top=None):
    families = collect_failures_by_family(data, surface)
    rows = sorted(
        families.items(),
        key=lambda item: (-len(item[1]), item[0]),
    )
    if top is not None:
        rows = rows[:top]
    return [
        {
            "family": family,
            "count": len(results),
            "examples": [
                result["name"]
                for result in sorted(results, key=lambda result: result["name"])[:3]
            ],
        }
        for family, results in rows
    ]


def failure_count_by_surface(data, surface):
    return sum(len(results) for results in collect_failures_by_family(data, surface).values())


def failure_family_report(data, top=20, include_stale_detail=False, freshness_data=None):
    freshness_data = freshness_data or data
    detail_summary = emit_summary(freshness_data)
    public_summary = emit_summary_from_readme()
    freshness = emit_freshness_report(detail_summary, public_summary, freshness_data)
    families_suppressed = freshness["state"] == "stale" and not include_stale_detail

    surfaces = []
    for surface, title in (("js", "JavaScript"), ("dts", "Declaration")):
        rows = [] if families_suppressed else family_rows(data, surface, top)
        surfaces.append(
            {
                "surface": surface,
                "title": title,
                "checkedDetailFailures": None
                if families_suppressed
                else failure_count_by_surface(data, surface),
                "publicRemaining": emit_remaining_failures(public_summary, surface),
                "checkedDetailRemaining": emit_remaining_failures(detail_summary, surface),
                "families": rows,
            }
        )

    return {
        "freshness": freshness,
        "familiesSuppressed": families_suppressed,
        "includeStaleDetail": include_stale_detail,
        "top": top,
        "surfaces": surfaces,
    }


def print_failure_families_json(data, top=20, include_stale_detail=False, freshness_data=None):
    print(
        json.dumps(
            failure_family_report(
                data,
                top=top,
                include_stale_detail=include_stale_detail,
                freshness_data=freshness_data,
            ),
            sort_keys=True,
        )
    )


def show_failure_families(data, top=20, include_stale_detail=False, freshness_data=None):
    print("Emit failure families")
    print()
    freshness_data = freshness_data or data
    detail_summary = emit_summary(freshness_data)
    public_summary = emit_summary_from_readme()
    freshness_status = emit_freshness_status(detail_summary, public_summary)
    if freshness_status["state"] == "stale" and not include_stale_detail:
        print_stale_failure_family_guard(detail_summary, public_summary)
        return

    if freshness_status["state"] == "aggregate-match":
        print(emit_freshness_status_line(freshness_data))
        print()
    else:
        print_emit_freshness_note(freshness_data)

    for surface, title in (("js", "JavaScript"), ("dts", "Declaration")):
        families = collect_failures_by_family(data, surface)
        rows = sorted(
            families.items(),
            key=lambda item: (-len(item[1]), item[0]),
        )
        total = sum(len(results) for _, results in rows)
        print(
            failure_family_surface_heading(
                surface, title, total, detail_summary, public_summary
            )
        )
        for family, results in rows[:top]:
            examples = ", ".join(
                r["name"] for r in sorted(results, key=lambda r: r["name"])[:3]
            )
            print(f"  {len(results):>4d}  {family}  ({examples})")
        print_truncated_more(rows, top)
        print()


def show_close(data, top=40):
    """Tests where JS passes but DTS fails, or vice versa."""
    results = data["results"]
    close = []
    for r in results:
        js_ok = r["jsStatus"] in ("pass", "skip")
        dts_ok = r["dtsStatus"] in ("pass", "skip")
        if js_ok and not dts_ok:
            close.append(("js-pass/dts-fail", r))
        elif dts_ok and not js_ok:
            close.append(("dts-pass/js-fail", r))

    print(f"Close-to-passing tests: {len(close)}")
    for kind, r in close[:top]:
        err = r.get("jsError") or r.get("dtsError") or ""
        print(f"  [{kind}] {r['name']}  {err[:60]}")
    print_truncated_more(close, top)


def show_filter(data, pattern, top=40, paths_only=False):
    results = data["results"]
    lower = pattern.lower()
    matches = [r for r in results if lower in r["name"].lower()]

    if paths_only:
        for r in matches:
            print(r["name"])
        return

    passing = sum(1 for r in matches if r["jsStatus"] == "pass" and r["dtsStatus"] in ("pass", "skip"))
    print(f"Tests matching '{pattern}': {len(matches)} ({passing} fully passing)")
    for r in matches[:top]:
        status = f"js={r['jsStatus']} dts={r['dtsStatus']}"
        print(f"  {r['name']}  [{status}]")
    print_truncated_more(matches, top)


def show_status(data, status, top=40):
    results = data["results"]
    matches = [r for r in results if r["jsStatus"] == status or r["dtsStatus"] == status]
    print(f"Tests with status '{status}': {len(matches)}")
    for r in matches[:top]:
        st = f"js={r['jsStatus']} dts={r['dtsStatus']}"
        err = r.get("jsError") or r.get("dtsError") or ""
        print(f"  {r['name']}  [{st}]  {err[:60]}")
    print_truncated_more(matches, top)


def main():
    parser = argparse.ArgumentParser(description="Query emit test results offline")
    parser.add_argument("--js-failures", action="store_true", help="Show JS failures")
    parser.add_argument("--dts-failures", action="store_true", help="Show DTS failures")
    parser.add_argument("--top-errors", action="store_true", help="Show top error messages")
    parser.add_argument("--families", action="store_true", help="Show JS/DTS failure family counts")
    parser.add_argument(
        "--include-stale-detail",
        action="store_true",
        help="With --families, print historical family rows even when emit-detail.json is stale",
    )
    parser.add_argument(
        "--families-json",
        action="store_true",
        help="Show JS/DTS failure family counts as machine-readable JSON",
    )
    parser.add_argument("--close", action="store_true", help="Show close-to-passing tests")
    parser.add_argument("--filter", type=str, help="Filter by substring in test name")
    parser.add_argument("--status", type=str, help="Filter by status (pass/fail/skip/timeout)")
    parser.add_argument("--paths-only", action="store_true", help="Output only test names (for piping)")
    parser.add_argument("--top", type=int, default=40, help="Limit rows shown")
    parser.add_argument("--freshness", action="store_true", help="Report whether emit-detail.json is current")
    parser.add_argument(
        "--freshness-json",
        action="store_true",
        help="Report emit-detail freshness as machine-readable JSON",
    )
    parser.add_argument(
        "--require-current-detail",
        action="store_true",
        help="Exit non-zero unless README/public aggregate matches emit-detail.json",
    )
    args = parser.parse_args()

    data = load_detail()
    filtered_data = filter_data_by_name(data, args.filter)
    freshness_status = emit_freshness_status(emit_summary(data), emit_summary_from_readme())

    if args.require_current_detail and not emit_detail_is_current(freshness_status):
        print(emit_freshness_status_line(data), file=sys.stderr)
        return 1

    if args.freshness_json:
        print_emit_freshness_json(data)
    elif args.freshness:
        print_emit_freshness_status(data)
    elif args.js_failures:
        show_js_failures(filtered_data, args.top, args.paths_only)
    elif args.dts_failures:
        show_dts_failures(filtered_data, args.top, args.paths_only)
    elif args.top_errors:
        show_top_errors(filtered_data, args.top)
    elif args.families_json:
        print_failure_families_json(
            filtered_data,
            args.top,
            args.include_stale_detail,
            freshness_data=data,
        )
    elif args.families:
        show_failure_families(
            filtered_data,
            args.top,
            args.include_stale_detail,
            freshness_data=data,
        )
    elif args.close:
        show_close(filtered_data, args.top)
    elif args.status:
        show_status(filtered_data, args.status, args.top)
    elif args.filter:
        show_filter(data, args.filter, args.top, args.paths_only)
    else:
        show_overview(data)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
