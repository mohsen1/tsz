#!/usr/bin/env python3
"""Query conformance snapshot data without re-running tests.

Reads from scripts/conformance/conformance-detail.json (produced by `conformance.sh snapshot`).

Usage:
  # KPI dashboard (primary daily signal)
  python3 scripts/conformance/query-conformance.py --dashboard

  # Show overview of what to work on next
  python3 scripts/conformance/query-conformance.py

  # Show root-cause campaigns instead of one-off quick wins
  python3 scripts/conformance/query-conformance.py --campaigns
  python3 scripts/conformance/query-conformance.py --campaign contextual-typing

  # Show tests fixable by adding a single missing code (highest impact)
  python3 scripts/conformance/query-conformance.py --one-missing

  # Show false positive breakdown
  python3 scripts/conformance/query-conformance.py --false-positives

  # Show tests that need a specific code
  python3 scripts/conformance/query-conformance.py --code TS2454

  # Show tests fixable by removing a single extra code
  python3 scripts/conformance/query-conformance.py --one-extra

  # List all tests failing with a specific extra code (false emissions)
  python3 scripts/conformance/query-conformance.py --extra-code TS7053

  # Show tests closest to passing (diff <= N)
  python3 scripts/conformance/query-conformance.py --close 2

  # Show fingerprint-only failures (same code set, different fingerprint)
  python3 scripts/conformance/query-conformance.py --fingerprint-only

  # List fingerprint-only tests touching a specific code
  python3 scripts/conformance/query-conformance.py --fingerprint-only --code TS2322

  # Show same-code count drift visible in compact expected/actual code lists
  python3 scripts/conformance/query-conformance.py --count-drift

  # Export test paths for a code to feed into conformance runner
  python3 scripts/conformance/query-conformance.py --code TS2454 --paths-only
"""

import sys
import json
import argparse
from collections import Counter
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "conformance-detail.json"

# =============================================================================
# Failure-category definitions (used by --campaign / --campaigns).
# These are offline analysis buckets; the session-level campaign system
# itself has been removed. See scripts/session/conformance-agent-prompt.md.
# =============================================================================

CAMPAIGNS = {
    # =========================================================================
    # TIER 1 — FINGERPRINT PARITY (50% of agent capacity)
    # =========================================================================
    "type-display-parity": {
        "title": "Type display parity — match tsc's error message formatting (Tier 1)",
        "description": "~310 fingerprint-only tests fail because error message text differs from tsc. The type printer makes different choices about alias-vs-expansion, function sig display, literal preservation, and intersection/union formatting. One printer fix can flip 50+ tests.",
        "codes": ["TS2322", "TS2345", "TS2339", "TS2741", "TS2769", "TS2416"],
        "keywords": [],
        "areas": [],
        "focus": [
            "Fix type printer to match tsc's alias-vs-expansion display policy.",
            "Fix function signature display (typeof foo vs full function type).",
            "Preserve literal types in error messages where tsc preserves them.",
            "Match tsc's intersection/union display formatting.",
            "KPI: fingerprint-only count for TS2322+TS2345+TS2339.",
        ],
        "match_mode": "fingerprint_only",
    },
    "diagnostic-count": {
        "title": "Diagnostic count accuracy — match tsc's emission rules (Tier 1)",
        "description": "~130 fingerprint-only tests fail because we emit a different number of instances of the same error code. Over-emitting (extra assignability checks, duplicate elaboration) or under-emitting (missing super-before-this, control-flow narrowed checks).",
        "codes": ["TS2322", "TS2345", "TS2339", "TS17009", "TS2564", "TS1264"],
        "keywords": [],
        "areas": [],
        "focus": [
            "Match tsc's diagnostic emission count for common error codes.",
            "Fix redundant elaboration/assignability checks that cause over-emission.",
            "Add missing control-flow and super-before-this checks for under-emission.",
            "KPI: fingerprint-only count where instance counts differ per code.",
        ],
        "match_mode": "fingerprint_only",
    },
    # =========================================================================
    # TIER 2 — WRONG-CODE CAMPAIGNS (30% of agent capacity)
    # =========================================================================
    "big3": {
        "title": "Big 3 relation kernel unification (Tier 2)",
        "description": "Route ALL TS2322/TS2339/TS2345 checks through one canonical relation boundary. 65 tests emit wrong error codes.",
        "codes": ["TS2322", "TS2339", "TS2345", "TS2416", "TS2769"],
        "keywords": [
            "contextual",
            "inference",
            "correlated",
            "contravariant",
            "intersection",
            "union",
            "generic",
            "mapped",
            "indexed",
            "property",
            "assignab",
            "subtype",
            "compat",
        ],
        "areas": ["types", "expressions", "controlFlow", "classes", "jsx", "jsdoc", "salsa"],
        "focus": [
            "Route TS2322/TS2345-family checks through one assignability boundary and remove checker-local forks.",
            "Find invariants that fix BOTH missing AND extra diagnostics in the same family.",
            "Delete checker-local re-derivations of EPC, missing-property, weak-union, property-shape classification.",
            "KPI: wrong-code count for TS2322+TS2339+TS2345 (currently 71).",
        ],
    },
    "contextual-typing": {
        "title": "Request/context transport completion (Tier 2)",
        "description": "Complete TypingRequest migration. Push request object through all hot paths. Eliminate raw contextual_type mutations from CheckerContext.",
        "codes": ["TS2322", "TS2345", "TS7006", "TS2769", "TS2416"],
        "keywords": [
            "contextual",
            "inference",
            "instantiate",
            "overload",
            "callback",
            "tuple",
            "rest",
            "application",
            "readonly",
        ],
        "areas": ["expressions", "types", "jsx"],
        "focus": [
            "Every contextual-type flow should use TypingRequest, not raw ctx.contextual_type mutations.",
            "Hot paths: call inference, JSX props/children, JSDoc callbacks, generic constructors, object-literal callbacks.",
            "KPI: count of raw contextual-state mutations outside TypingRequest.",
        ],
    },
    "narrowing-flow": {
        "title": "Narrowing boundary cleanup (Tier 2)",
        "description": "Finish narrowing.rs as boundary-clean. Zero direct solver calls from narrowing code. Add boundary helpers for all narrowing queries.",
        "codes": ["TS2339", "TS18048", "TS2454", "TS7022", "TS1360", "TS2322", "TS2345"],
        "keywords": [
            "controlflow",
            "discriminant",
            "predicate",
            "optional",
            "alias",
            "narrow",
            "catch",
            "finally",
        ],
        "areas": ["controlFlow", "expressions/typeGuards", "types/literal"],
        "focus": [
            "Zero direct solver type-query calls from narrowing code.",
            "Add boundary helpers for predicate extraction, lazy-def resolution, truthiness, type-param queries.",
            "Re-add narrowing.rs to architecture_contract_tests.rs when clean.",
            "KPI: direct solver query calls remaining in narrowing code.",
        ],
    },
    # =========================================================================
    # TIER 3 — SUBSYSTEM & LEAF (20% of agent capacity)
    # =========================================================================
    "module-resolution": {
        "title": "Node/declaration-emit coordination (Tier 3)",
        "description": "Resolver diagnostics, driver-provided mode facts, package self-name/exports semantics, and declaration-emit coordination. ~26 real Node lane failures. NOT big3 work.",
        "codes": ["TS2307", "TS2835", "TS2792", "TS1479", "TS2883", "TS5107", "TS1192", "TS5101"],
        "keywords": [
            "nodemodules",
            "nodenext",
            "packageexports",
            "specifier",
            "resolution-mode",
            "declarationemit",
            "symlink",
            "packagejson",
        ],
        "areas": ["externalModules", "node", "declarationEmit", "moduleResolution"],
        "focus": [
            "Keep TS2307/TS2792/TS2834/TS2835 selection owned by resolver+driver plumbing.",
            "Treat file-format facts, import-mode attributes, and package exports/self-name semantics as coordination inputs.",
            "Node lane KPI: projects/NodeModulesSearch + projects/jsFileCompilation + node + declarationEmit pass rate.",
        ],
    },
    "parser-recovery": {
        "title": "Parser recovery diagnostic selection (Tier 3)",
        "description": "Catch-all recovery emits the wrong TS1xxx code and cascades into secondary parser noise.",
        "codes": ["TS1005", "TS1128", "TS1109", "TS1434", "TS1003", "TS1134"],
        "keywords": [
            "parser",
            "modifier",
            "async",
            "export",
            "import",
            "class",
        ],
        "areas": [],
        "focus": [
            "Reduce TS1005 catch-all usage and choose the most specific recovery code first.",
            "Fix recovery boundaries in import/export/class-member contexts to collapse cascaded parser errors.",
            "Measure parser fixes by code-family deltas, not by individual malformed test files.",
        ],
    },
    "jsdoc-jsx-salsa": {
        "title": "Semantic integration areas (Tier 3)",
        "description": "These areas are broad consumers of the same solver/checker mechanics. Most improvements come from Tier 1/2 campaigns.",
        "codes": ["TS2322", "TS2339", "TS2345", "TS7006", "TS2353", "TS2786"],
        "keywords": ["jsdoc", "jsx", "salsa", "defaultprops", "component", "typedef", "callback"],
        "areas": ["jsdoc", "jsx", "salsa"],
        "focus": [
            "Use these suites as regression baskets for Tier 1/2 fixes.",
            "Only work area-local gaps after Tier 1/2 progress stabilizes.",
            "Reserve area-local fixes for true syntax or feature-surface gaps after semantic root causes are addressed.",
        ],
    },
}

# Node lane areas for KPI tracking
NODE_LANE_AREAS = [
    "projects/NodeModulesSearch",
    "projects/jsFileCompilation",
    "node",
    "declarationEmit",
    "moduleResolution",
    "externalModules",
    "externalModules/typeOnly",
]


def load_detail():
    if not DETAIL_FILE.exists():
        print(f"Error: {DETAIL_FILE} not found.")
        print("Run: ./scripts/conformance/conformance.sh snapshot")
        sys.exit(1)
    with open(DETAIL_FILE) as f:
        return json.load(f)


def basename(path):
    return path.rsplit("/", 1)[-1] if "/" in path else path


def area_of(path):
    markers = [
        "/cases/compiler/",
        "/cases/conformance/compiler/",
    ]
    for marker in markers:
        if marker in path:
            rest = path.split(marker, 1)[1]
            parts = rest.split("/")
            if len(parts) >= 2:
                return "/".join(parts[:-1])
            return "compiler"
    return ""


def count_codes(failure):
    counts = Counter()
    for code in failure.get("m", []):
        counts[code] += 1
    for code in failure.get("x", []):
        counts[code] += 1
    return counts


def code_counts(codes):
    return Counter(codes)


def is_fingerprint_only(failure):
    expected = failure.get("e", [])
    actual = failure.get("a", [])
    return bool(expected) and bool(actual) and code_counts(expected) == code_counts(actual)


def is_same_code_count_drift(failure):
    expected = failure.get("e", [])
    actual = failure.get("a", [])
    if not expected or not actual:
        return False
    expected_counts = code_counts(expected)
    actual_counts = code_counts(actual)
    return expected_counts != actual_counts and set(expected_counts) == set(actual_counts)


def match_campaign(path, failure, config):
    # Fingerprint-only campaigns match only FP-only tests containing their codes
    if config.get("match_mode") == "fingerprint_only":
        if not is_fingerprint_only(failure):
            return False, 0, [], area_of(path)
        codes_in_test = set(failure.get("e", []))
        matched_codes = [c for c in config.get("codes", []) if c in codes_in_test]
        if not matched_codes:
            return False, 0, [], area_of(path)
        score = len(matched_codes) * 3
        return True, score, matched_codes, area_of(path)

    if config.get("match_mode") == "same_code_count_drift":
        if not is_same_code_count_drift(failure):
            return False, 0, [], area_of(path)
        expected_counts = code_counts(failure.get("e", []))
        actual_counts = code_counts(failure.get("a", []))
        drift_codes = [
            code
            for code in config.get("codes", [])
            if expected_counts.get(code, 0) != actual_counts.get(code, 0)
        ]
        if not drift_codes:
            return False, 0, [], area_of(path)
        score = len(drift_codes) * 3
        return True, score, drift_codes, area_of(path)

    low = path.lower()
    area = area_of(path)
    score = 0
    matched_codes = []
    for code in config.get("codes", []):
        if code in failure.get("m", []) or code in failure.get("x", []):
            score += 3
            matched_codes.append(code)

    for keyword in config.get("keywords", []):
        if keyword.lower() in low:
            score += 1

    area_match = False
    for prefix in config.get("areas", []):
        if area == prefix or area.startswith(prefix + "/"):
            score += 2
            area_match = True

    matched = bool(matched_codes) or area_match
    return matched, score, matched_codes, area


def build_campaign_result(data, name):
    config = CAMPAIGNS[name]
    failures = data["failures"]
    matched_tests = []
    code_counts = Counter()
    area_counts = Counter()
    for path, failure in failures.items():
        matched, score, matched_codes, area = match_campaign(path, failure, config)
        if not matched:
            continue
        diff = len(failure.get("m", [])) + len(failure.get("x", []))
        matched_tests.append((score, diff, path, failure))
        code_counts.update(count_codes(failure))
        if area:
            area_counts[area] += 1

    matched_tests.sort(key=lambda item: (-item[0], item[1], basename(item[2]).lower(), item[2]))
    return {
        "name": name,
        "config": config,
        "tests": matched_tests,
        "code_counts": code_counts,
        "area_counts": area_counts,
    }


# =============================================================================
# KPI Dashboard — the primary daily signal (replaces overall %)
# =============================================================================


def show_dashboard(data):
    """Show the KPI dashboard that replaces overall conformance % as the daily signal."""
    s = data["summary"]
    failures = data["failures"]

    print("=" * 70)
    print("  TSZ CONFORMANCE KPI DASHBOARD")
    print("=" * 70)
    print()
    print(f"  Overall: {s['passed']}/{s['total']} ({s['passed']/s['total']*100:.1f}%)")
    print()

    # KPI 1: Wrong-code count for big3
    big3_codes = ["TS2322", "TS2339", "TS2345"]
    big3_wrong = Counter()
    big3_missing = Counter()
    big3_extra = Counter()
    for path, f in failures.items():
        for code in big3_codes:
            if code in f.get("m", []):
                big3_missing[code] += f["m"].count(code)
            if code in f.get("x", []):
                big3_extra[code] += f["x"].count(code)
            if code in f.get("m", []) or code in f.get("x", []):
                big3_wrong[code] += 1

    big3_total = sum(big3_wrong.values())
    print(f"  KPI 1: Big3 wrong-code problems: {big3_total}")
    for code in big3_codes:
        m = big3_missing.get(code, 0)
        x = big3_extra.get(code, 0)
        w = big3_wrong.get(code, 0)
        print(f"    {code}: {w} tests ({m} missing, {x} extra)")
    print()

    # KPI 1b: Other tracked codes per ROADMAP.md workstream 1 ("Track top-code
    # deltas for TS2322, TS2345, TS2339, TS1005, and TS2353"). TS1005 is the
    # parser-recovery catch-all; TS2353 is excess-property emission. Visible
    # deltas here let parser-recovery and excess-prop work show up next to
    # Big3 in the daily dashboard signal.
    other_codes = ["TS1005", "TS2353"]
    other_wrong = Counter()
    other_missing = Counter()
    other_extra = Counter()
    for path, f in failures.items():
        for code in other_codes:
            if code in f.get("m", []):
                other_missing[code] += f["m"].count(code)
            if code in f.get("x", []):
                other_extra[code] += f["x"].count(code)
            if code in f.get("m", []) or code in f.get("x", []):
                other_wrong[code] += 1

    other_total = sum(other_wrong.values())
    print(f"  KPI 1b: Other tracked codes (TS1005 + TS2353): {other_total}")
    for code in other_codes:
        m = other_missing.get(code, 0)
        x = other_extra.get(code, 0)
        w = other_wrong.get(code, 0)
        print(f"    {code}: {w} tests ({m} missing, {x} extra)")
    print()

    # KPI 2: Crash count (tests where we emit 0 diagnostics but tsc expects some)
    crashes = sum(1 for f in failures.values() if not f.get("a") and f.get("e"))
    print(f"  KPI 2: Likely crashes (0 actual, >0 expected): {crashes}")
    print()

    # KPI 3: Node lane pass rate
    all_tests = set(failures.keys())
    # We need to compute from snapshot if available
    a = data.get("aggregates", {})
    node_areas = {}
    areas_data = a.get("areas_by_pass_rate", [])
    for area_info in areas_data:
        name = area_info.get("area", "")
        for node_area in NODE_LANE_AREAS:
            if name == node_area:
                node_areas[name] = area_info
    if node_areas:
        print("  KPI 3: Node lane pass rates:")
        total_p, total_t = 0, 0
        for name in NODE_LANE_AREAS:
            if name in node_areas:
                info = node_areas[name]
                p = info.get("passed", 0)
                t = info.get("total", 0)
                rate = (p / t * 100) if t > 0 else 0
                print(f"    {name}: {p}/{t} ({rate:.1f}%)")
                total_p += p
                total_t += t
        if total_t > 0:
            print(f"    TOTAL: {total_p}/{total_t} ({total_p/total_t*100:.1f}%)")
    else:
        # Estimate from failures
        node_fail = sum(
            1 for p in failures
            if any(na in p for na in ["NodeModulesSearch", "jsFileCompilation", "/node/", "declarationEmit"])
        )
        print(f"  KPI 3: Node lane failures (estimate): {node_fail}")
    print()

    # KPI 4: Close-to-passing
    close_0 = sum(1 for f in failures.values() if len(f.get("m", [])) + len(f.get("x", [])) == 0)
    close_1 = sum(1 for f in failures.values() if len(f.get("m", [])) + len(f.get("x", [])) == 1)
    close_2 = sum(1 for f in failures.values() if len(f.get("m", [])) + len(f.get("x", [])) == 2)
    fp_only = sum(1 for f in failures.values() if is_fingerprint_only(f))
    count_drift = sum(1 for f in failures.values() if is_same_code_count_drift(f))
    print(f"  KPI 4: Close-to-passing:")
    print(f"    Fingerprint-only (same codes, wrong tuple): {fp_only}")
    print(f"    Same-code count drift (compact codes): {count_drift}")
    print(f"    Diff = 1: {close_1}")
    print(f"    Diff = 2: {close_2}")
    print(f"    Total diff <= 2: {close_1 + close_2}")
    print()

    # KPI 5: Failure categories
    cats = a.get("categories", {})
    if cats:
        print("  Failure categories:")
        print(f"    False positives (expected 0, we emit):  {cats.get('false_positive', '?')}")
        print(f"    All missing (expected errors, we emit 0): {cats.get('all_missing', '?')}")
        print(f"    Fingerprint-only (same codes, wrong tuple): {cats.get('fingerprint_only', '?')}")
        print(f"    Wrong codes (both have, codes differ):  {cats.get('wrong_code', '?')}")
        print(f"    Same-code count drift (computed):       {count_drift}")
        print(f"    Close to passing (diff <= 2):           {cats.get('close_to_passing', '?')}")
    print()

    # KPI 6: Fingerprint breakdown (the dominant failure category)
    fp_tests = [f for f in failures.values() if is_fingerprint_only(f)]
    fp_code_counts = Counter()
    for f in fp_tests:
        fp_code_counts.update(f.get("e", []))
    big3_fp = sum(fp_code_counts.get(c, 0) for c in ["TS2322", "TS2345", "TS2339"])
    print(f"  KPI 6: Fingerprint-only breakdown ({len(fp_tests)} tests, {len(fp_tests)/max(len(failures),1)*100:.0f}% of failures):")
    print(f"    TS2322+TS2345+TS2339 in FP-only: {big3_fp} test-codes")
    for code in ["TS2322", "TS2345", "TS2339", "TS2564", "TS2454"]:
        count = fp_code_counts.get(code, 0)
        if count > 0:
            print(f"    {code}: {count} tests")
    print()

    # Campaign impact summary
    print("  Campaign impact (Tier 1 — fingerprint parity):")
    print(f"    type-display-parity: ~{len(fp_tests) * 50 // 100} est. tests (50% of FP-only)")
    print(f"    diagnostic-count: ~{len(fp_tests) * 21 // 100} est. tests (21% of FP-only)")
    print()
    print("  Campaign impact (Tier 2 — wrong-code):")
    for name in ["big3", "contextual-typing", "narrowing-flow"]:
        result = build_campaign_result(data, name)
        print(f"    {name}: {len(result['tests'])} failing tests")
    print()

    print("=" * 70)
    print("  Fingerprint parity is 73.6% of remaining work.")
    print("  Track KPIs, not overall %. Fix patterns, not individual tests.")
    print("=" * 70)


def show_overview(data):
    s = data["summary"]
    a = data["aggregates"]
    print(f"Conformance: {s['passed']}/{s['total']} ({s['passed']/s['total']*100:.1f}%)")
    print()

    print("Recommended campaigns (root-cause first):")
    show_campaigns(data, top_n=5, sample_n=4, include_header=False)
    print()

    cats = a["categories"]
    count_drift = sum(1 for f in data["failures"].values() if is_same_code_count_drift(f))
    print("Failure categories:")
    print(f"  False positives (expected 0, we emit):  {cats['false_positive']}")
    print(f"  All missing (expected errors, we emit 0): {cats['all_missing']}")
    print(f"  Fingerprint-only (same codes, wrong tuple): {cats.get('fingerprint_only', 0)}")
    print(f"  Wrong codes (both have, codes differ):  {cats['wrong_code']}")
    print(f"  Same-code count drift (compact codes):  {count_drift}")
    print(f"  Close to passing (diff <= 2):           {cats['close_to_passing']}")
    print()

    print("Quick wins — add 1 missing code, 0 extra (instant pass):")
    for item in a["one_missing_zero_extra"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    print("Quick wins — remove 1 extra code, 0 missing (instant pass):")
    for item in a["one_extra_zero_missing"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    print("Not implemented codes (never emitted by tsz):")
    for item in a["not_implemented_codes"][:15]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests need it")
    print()

    print("Partially implemented (emitted sometimes, missing others):")
    for item in a["partial_codes"][:15]:
        print(f"  {item['code']:>8s}: missing in {item['count']:>3d} tests")


def show_one_missing(data):
    a = data["aggregates"]
    items = a["one_missing_zero_extra"]
    if not items:
        print("No tests are exactly 1 missing code away from passing.")
        return
    total = sum(i["count"] for i in items)
    print(f"Tests fixable by adding 1 missing code (0 extra): {total} total")
    print()
    for item in items:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests would pass")


def show_one_extra(data):
    a = data["aggregates"]
    items = a["one_extra_zero_missing"]
    if not items:
        print("No tests are exactly 1 extra code away from passing.")
        return
    total = sum(i["count"] for i in items)
    print(f"Tests fixable by removing 1 extra code (0 missing): {total} total")
    print()
    for item in items:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests would pass")


def show_false_positives(data):
    a = data["aggregates"]
    failures = data["failures"]
    print(f"False positives: {a['categories']['false_positive']} tests")
    print()
    print("Top codes emitted incorrectly:")
    for item in a["false_positive_codes"][:20]:
        print(f"  {item['code']:>8s}: {item['count']:>3d} tests")
    print()

    # List actual false positive tests grouped by code
    fp_tests = {}
    for path, f in failures.items():
        if not f.get("e") and f.get("a"):
            for code in set(f["a"]):
                fp_tests.setdefault(code, []).append(path)

    for item in a["false_positive_codes"][:5]:
        code = item["code"]
        tests = fp_tests.get(code, [])
        print(f"\n{code} false positives ({len(tests)} tests):")
        for t in sorted(tests)[:10]:
            basename = t.rsplit("/", 1)[-1] if "/" in t else t
            print(f"  {basename}")
        if len(tests) > 10:
            print(f"  ... and {len(tests) - 10} more")


def show_code(data, code, paths_only=False):
    failures = data["failures"]
    missing_tests = []
    extra_tests = []
    for path, f in sorted(failures.items()):
        if code in f.get("m", []):
            missing_tests.append((path, f))
        if code in f.get("x", []):
            extra_tests.append((path, f))

    if paths_only:
        for path, _ in missing_tests + extra_tests:
            print(path)
        return

    print(f"Code {code}:")
    print(f"  Missing in {len(missing_tests)} tests (need to add)")
    print(f"  Extra in {len(extra_tests)} tests (need to remove)")
    print()

    if missing_tests:
        # Sub-categorize missing tests
        only_this = [(p, f) for p, f in missing_tests if f.get("m") == [code] and not f.get("x")]
        print(f"  Would-pass-if-added (only missing code, 0 extra): {len(only_this)}")
        for p, f in only_this[:20]:
            basename = p.rsplit("/", 1)[-1] if "/" in p else p
            exp = ",".join(f.get("e", []))
            print(f"    {basename}  expected=[{exp}]")
        if len(only_this) > 20:
            print(f"    ... and {len(only_this) - 20} more")
        print()

        also_need = [(p, f) for p, f in missing_tests if f.get("m") != [code] or f.get("x")]
        if also_need:
            print(f"  Also missing {code} but need other fixes too: {len(also_need)}")
            for p, f in also_need[:10]:
                basename = p.rsplit("/", 1)[-1] if "/" in p else p
                m = ",".join(f.get("m", []))
                x = ",".join(f.get("x", []))
                print(f"    {basename}  missing=[{m}]  extra=[{x}]")
            if len(also_need) > 10:
                print(f"    ... and {len(also_need) - 10} more")

    if extra_tests:
        print(f"\n  Extra {code} in {len(extra_tests)} tests:")
        only_this = [(p, f) for p, f in extra_tests if f.get("x") == [code] and not f.get("m")]
        print(f"    Would-pass-if-removed (only extra code, 0 missing): {len(only_this)}")
        for p, f in only_this[:10]:
            basename = p.rsplit("/", 1)[-1] if "/" in p else p
            print(f"      {basename}")


def show_fingerprint_only(data, code=None, paths_only=False, top=40):
    failures = data["failures"]
    matches = []
    code_counts = Counter()
    area_counts = Counter()

    for path, failure in sorted(failures.items()):
        if not is_fingerprint_only(failure):
            continue
        codes = failure.get("e", [])
        if code and code not in codes:
            continue
        matches.append((path, failure))
        code_counts.update(codes)
        area = area_of(path)
        if area:
            area_counts[area] += 1

    if paths_only:
        for path, _ in matches:
            print(path)
        return

    scope = f" for {code}" if code else ""
    print(f"Fingerprint-only failures{scope}: {len(matches)}")
    if code_counts:
        print()
        print("Top codes:")
        for item_code, count in code_counts.most_common(10):
            print(f"  {item_code:>8s}: {count:>3d}")
    if area_counts:
        print()
        print("Most affected areas:")
        for area, count in area_counts.most_common(10):
            print(f"  {area}: {count}")

    print()
    print("Representative tests:")
    for path, failure in matches[:top]:
        name = basename(path)
        codes = ",".join(failure.get("e", []))
        print(f"  {name}  codes=[{codes}]")
    if len(matches) > top:
        print(f"  ... and {len(matches) - top} more")


def show_count_drift(data, code=None, paths_only=False, top=40):
    failures = data["failures"]
    matches = []
    under_counts = Counter()
    over_counts = Counter()

    for path, failure in sorted(failures.items()):
        if not is_same_code_count_drift(failure):
            continue
        expected_counts = code_counts(failure.get("e", []))
        actual_counts = code_counts(failure.get("a", []))
        drift_codes = []
        for item_code in sorted(set(expected_counts) | set(actual_counts)):
            diff = actual_counts.get(item_code, 0) - expected_counts.get(item_code, 0)
            if diff < 0:
                under_counts[item_code] += -diff
                drift_codes.append(item_code)
            elif diff > 0:
                over_counts[item_code] += diff
                drift_codes.append(item_code)
        if code and code not in drift_codes:
            continue
        matches.append((path, failure, expected_counts, actual_counts, drift_codes))

    if paths_only:
        for path, *_ in matches:
            print(path)
        return

    scope = f" for {code}" if code else ""
    print(f"Same-code count drift failures{scope}: {len(matches)}")
    print()
    if under_counts or over_counts:
        print("Top compact count deltas:")
        for item_code in sorted(set(under_counts) | set(over_counts)):
            print(
                f"  {item_code:>8s}: "
                f"under={under_counts.get(item_code, 0):>3d} "
                f"over={over_counts.get(item_code, 0):>3d}"
            )
        print()

    print("Representative tests:")
    for path, failure, expected_counts, actual_counts, drift_codes in matches[:top]:
        name = basename(path)
        deltas = []
        for item_code in drift_codes:
            deltas.append(
                f"{item_code} {expected_counts.get(item_code, 0)}->{actual_counts.get(item_code, 0)}"
            )
        print(f"  {name}  {', '.join(deltas)}")
    if len(matches) > top:
        print(f"  ... and {len(matches) - top} more")


def show_extra_code(data, code):
    failures = data["failures"]
    tests = []
    for path, f in sorted(failures.items()):
        if code in f.get("x", []):
            tests.append((path, f))

    print(f"Tests where {code} is emitted as EXTRA ({len(tests)} tests):")
    for p, f in tests[:30]:
        basename = p.rsplit("/", 1)[-1] if "/" in p else p
        m = ",".join(f.get("m", []))
        x = ",".join(f.get("x", []))
        e = ",".join(f.get("e", []))
        print(f"  {basename}  expected=[{e}]  missing=[{m}]  extra=[{x}]")
    if len(tests) > 30:
        print(f"  ... and {len(tests) - 30} more")


def show_close(data, max_diff):
    failures = data["failures"]
    close = []
    for path, f in failures.items():
        missing = f.get("m", [])
        extra = f.get("x", [])
        diff = len(missing) + len(extra)
        if 0 < diff <= max_diff:
            close.append((diff, path, f))
    close.sort()
    print(f"Tests within diff <= {max_diff} of passing: {len(close)}")
    for diff, path, f in close[:40]:
        basename = path.rsplit("/", 1)[-1] if "/" in path else path
        m = ",".join(f.get("m", []))
        x = ",".join(f.get("x", []))
        print(f"  [diff={diff}] {basename}  missing=[{m}]  extra=[{x}]")
    if len(close) > 40:
        print(f"  ... and {len(close) - 40} more")


def show_campaigns(data, top_n=5, sample_n=5, include_header=True):
    if include_header:
        print("Recommended campaigns (root-cause first):")
    results = []
    for name in CAMPAIGNS:
        result = build_campaign_result(data, name)
        results.append(result)

    results.sort(key=lambda item: (-len(item["tests"]), item["name"]))
    for index, result in enumerate(results[:top_n], start=1):
        config = result["config"]
        print(f"{index}. {result['name']} - {config['title']}")
        print(f"   impact: {len(result['tests'])} failing tests")
        top_codes = ", ".join(
            f"{code}={count}" for code, count in result["code_counts"].most_common(5)
        )
        if top_codes:
            print(f"   codes: {top_codes}")
        print(f"   why: {config['description']}")
        samples = [basename(path) for _, _, path, _ in result["tests"][:sample_n]]
        if samples:
            print(f"   samples: {', '.join(samples)}")


def show_campaign(data, name, top_n=15):
    if name not in CAMPAIGNS:
        print(f"Unknown campaign '{name}'.")
        print("Available campaigns:")
        for key in CAMPAIGNS:
            print(f"  {key}")
        return

    result = build_campaign_result(data, name)
    config = result["config"]
    print(f"Campaign: {name}")
    print(f"Title: {config['title']}")
    print(f"Impact: {len(result['tests'])} failing tests")
    print(f"Why: {config['description']}")
    print()
    print("Focus:")
    for item in config["focus"]:
        print(f"  - {item}")
    print()
    if result["code_counts"]:
        print("Top codes in this campaign:")
        for code, count in result["code_counts"].most_common(10):
            print(f"  {code:>8s}: {count:>3d}")
        print()
    if result["area_counts"]:
        print("Most affected areas:")
        for area, count in result["area_counts"].most_common(10):
            print(f"  {area}: {count}")
        print()
    print("Representative failing tests:")
    for score, diff, path, failure in result["tests"][:top_n]:
        m = ",".join(failure.get("m", []))
        x = ",".join(failure.get("x", []))
        print(f"  [score={score} diff={diff}] {basename(path)}  missing=[{m}]  extra=[{x}]")
    if len(result["tests"]) > top_n:
        print(f"  ... and {len(result['tests']) - top_n} more")


def main():
    parser = argparse.ArgumentParser(description="Query conformance snapshot offline")
    parser.add_argument("--dashboard", action="store_true", help="Show KPI dashboard (primary daily signal)")
    parser.add_argument("--campaigns", action="store_true", help="Show root-cause campaigns")
    parser.add_argument("--campaign", type=str, help="Show one root-cause campaign in detail")
    parser.add_argument("--one-missing", action="store_true", help="Show 1-missing-0-extra tests")
    parser.add_argument("--one-extra", action="store_true", help="Show 1-extra-0-missing tests")
    parser.add_argument("--false-positives", action="store_true", help="Show false positive breakdown")
    parser.add_argument("--code", type=str, help="Show tests involving a specific error code (e.g., TS2454)")
    parser.add_argument("--extra-code", type=str, help="Show tests where a code is emitted as extra")
    parser.add_argument("--close", type=int, help="Show tests within diff <= N of passing")
    parser.add_argument(
        "--fingerprint-only",
        action="store_true",
        help="Show failures where expected and actual code lists already match",
    )
    parser.add_argument(
        "--count-drift",
        action="store_true",
        help="Show failures whose compact expected/actual lists contain the same codes with different counts",
    )
    parser.add_argument("--paths-only", action="store_true", help="Output only test paths (for piping)")
    parser.add_argument("--top", type=int, default=20, help="Limit rows shown in detailed views")
    parser.add_argument(
        "--category",
        type=str,
        help="Legacy alias: false-positive, close, one-missing, one-extra, campaigns",
    )
    args = parser.parse_args()

    data = load_detail()

    if args.dashboard:
        show_dashboard(data)
    elif args.category == "false-positive":
        show_false_positives(data)
    elif args.category == "close":
        show_close(data, 2)
    elif args.category == "one-missing":
        show_one_missing(data)
    elif args.category == "one-extra":
        show_one_extra(data)
    elif args.category == "campaigns":
        show_campaigns(data, top_n=min(args.top, len(CAMPAIGNS)), sample_n=4)
    elif args.campaigns:
        show_campaigns(data, top_n=min(args.top, len(CAMPAIGNS)), sample_n=4)
    elif args.campaign:
        show_campaign(data, args.campaign, top_n=args.top)
    elif args.one_missing:
        show_one_missing(data)
    elif args.one_extra:
        show_one_extra(data)
    elif args.false_positives:
        show_false_positives(data)
    elif args.fingerprint_only:
        show_fingerprint_only(data, code=args.code, paths_only=args.paths_only, top=args.top)
    elif args.count_drift:
        show_count_drift(data, code=args.code, paths_only=args.paths_only, top=args.top)
    elif args.code:
        show_code(data, args.code, args.paths_only)
    elif args.extra_code:
        show_extra_code(data, args.extra_code)
    elif args.close is not None:
        show_close(data, args.close)
    else:
        show_overview(data)


if __name__ == "__main__":
    main()
