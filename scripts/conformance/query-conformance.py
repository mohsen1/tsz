#!/usr/bin/env python3
"""Query conformance snapshot data without re-running tests.

Reads from scripts/conformance/conformance-detail.json (produced by `conformance.sh snapshot`).

Usage:
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

  # Export test paths for a code to feed into conformance runner
  python3 scripts/conformance/query-conformance.py --code TS2454 --paths-only
"""

import sys
import json
import argparse
from collections import Counter
from pathlib import Path

DETAIL_FILE = Path(__file__).parent / "conformance-detail.json"

CAMPAIGNS = {
    "big3": {
        "title": "Big 3 assignability/property/call compatibility",
        "description": "Shared root causes behind TS2322/TS2339/TS2345 in both missing and extra form.",
        "codes": ["TS2322", "TS2339", "TS2345", "TS7006", "TS2769"],
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
        ],
        "areas": ["types", "expressions", "controlFlow", "classes", "jsx", "jsdoc", "salsa"],
        "focus": [
            "Route TS2322/TS2345-family checks through one assignability boundary and remove checker-local forks.",
            "Audit any-propagation, union/intersection compatibility, and contextual inference before relation checks.",
            "Prefer fixes that reduce both missing and extra diagnostics in the same family.",
        ],
    },
    "contextual-typing": {
        "title": "Contextual typing and inference transport",
        "description": "Contextual types fail to reach callbacks, object literals, rest tuples, and instantiated applications.",
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
            "Ensure Lazy/Application/ThisType wrappers are evaluated before extracting contextual parameter or prop types.",
            "Keep contextual information flowing through tuple-rest, overload selection, and callback inference.",
            "Treat TS7006 and TS2769 as secondary signals of the same transport bug, not separate work items.",
        ],
    },
    "property-resolution": {
        "title": "Property and index resolution on unions/intersections",
        "description": "Property lookup and index access diverge from tsc on merged shapes, symbols, and partial member presence.",
        "codes": ["TS2339", "TS7053", "TS2538", "TS7017", "TS2304"],
        "keywords": [
            "property",
            "index",
            "computed",
            "symbol",
            "intersection",
            "union",
            "member",
            "indexed",
        ],
        "areas": ["types", "expressions", "classes", "externalModules"],
        "focus": [
            "Centralize property/index lookup semantics in solver visitors instead of checker-side shape matching.",
            "Match tsc precedence rules for numeric vs string index signatures and partial union member absence.",
            "Use one path for property lookup so TS2339/TS7053/TS7017 move together.",
        ],
    },
    "module-resolution": {
        "title": "Node/module-resolution/declaration-emit coordination",
        "description": "NodeNext/package-exports/import-mode failures cluster around resolver-owned diagnostics, driver-provided mode facts, and declaration-emit coordination rather than core subtype semantics.",
        "codes": ["TS2307", "TS2835", "TS2792", "TS1479", "TS2883", "TS5107"],
        "keywords": [
            "node",
            "nodenext",
            "exports",
            "imports",
            "self",
            "specifier",
            "resolution-mode",
            "declarationemit",
            "symlink",
            "packagejson",
        ],
        "areas": ["externalModules", "node", "compiler", "declarationEmit"],
        "focus": [
            "Keep TS2307/TS2792/TS2834/TS2835 selection owned by resolver+driver plumbing instead of checker-local heuristics.",
            "Treat file-format facts, import-mode attributes, and package exports/self-name semantics as coordination inputs, not core relation work.",
            "Use declaration-emit regressions as a signal that resolver and driver facts are not being preserved end-to-end.",
        ],
    },
    "narrowing-flow": {
        "title": "Control-flow and narrowing transport",
        "description": "Narrowing information is lost across aliases, predicates, optional chains, and assignment edges.",
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
            "Transport narrowing through optional-chain prefixes, aliases, and destructuring targets.",
            "Unify predicate, equality, and nullish narrowing so intermediate references narrow consistently.",
            "Treat TS2454/TS18048/TS2339 clusters as one CFA problem, not isolated diagnostics.",
        ],
    },
    "parser-recovery": {
        "title": "Parser recovery diagnostic selection",
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
        "title": "Semantic integration areas: JSDoc, JSX, Salsa",
        "description": "These areas are broad consumers of the same solver/checker mechanics and should be worked as campaigns.",
        "codes": ["TS2322", "TS2339", "TS2345", "TS7006", "TS2353", "TS2786"],
        "keywords": ["jsdoc", "jsx", "salsa", "defaultprops", "component", "typedef", "callback"],
        "areas": ["jsdoc", "jsx", "salsa"],
        "focus": [
            "Do not chase these by area-only pass rate; bucket failures by shared semantics first.",
            "Use these suites as regression baskets for contextual typing, assignability, and property lookup fixes.",
            "Reserve area-local fixes for true syntax or feature-surface gaps after semantic root causes are addressed.",
        ],
    },
}


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


def match_campaign(path, failure, config):
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


def show_overview(data):
    s = data["summary"]
    a = data["aggregates"]
    print(f"Conformance: {s['passed']}/{s['total']} ({s['passed']/s['total']*100:.1f}%)")
    print()

    print("Recommended campaigns (root-cause first):")
    show_campaigns(data, top_n=5, sample_n=4, include_header=False)
    print()

    cats = a["categories"]
    print("Failure categories:")
    print(f"  False positives (expected 0, we emit):  {cats['false_positive']}")
    print(f"  All missing (expected errors, we emit 0): {cats['all_missing']}")
    print(f"  Fingerprint-only (same codes, wrong tuple): {cats.get('fingerprint_only', 0)}")
    print(f"  Wrong codes (both have, codes differ):  {cats['wrong_code']}")
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


def is_fingerprint_only(failure):
    return bool(failure.get("e")) and failure.get("e") == failure.get("a", [])


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
    parser.add_argument("--paths-only", action="store_true", help="Output only test paths (for piping)")
    parser.add_argument("--top", type=int, default=20, help="Limit rows shown in detailed views")
    parser.add_argument(
        "--category",
        type=str,
        help="Legacy alias: false-positive, close, one-missing, one-extra, campaigns",
    )
    args = parser.parse_args()

    data = load_detail()

    if args.category == "false-positive":
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
