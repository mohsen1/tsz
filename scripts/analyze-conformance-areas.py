#!/usr/bin/env python3
"""Analyze conformance test results by directory area to identify feature gaps.

Groups PASS/FAIL/SKIP/CRASH/TIMEOUT results by test directory path segments
to show which areas (parser, salsa, types/tuple, etc.) need the most attention.
"""

import sys
import re
from collections import defaultdict


# ANSI colors
GREEN = "\033[0;32m"
YELLOW = "\033[1;33m"
RED = "\033[0;31m"
CYAN = "\033[0;36m"
DIM = "\033[2m"
BOLD = "\033[1m"
NC = "\033[0m"

# Common prefix stripped from all test paths
PATH_PREFIX = "TypeScript/tests/cases/"


def parse_results(tmpfile):
    """Parse runner output into a list of (path, status) tuples."""
    results = []
    with open(tmpfile) as f:
        for line in f:
            line = line.rstrip()
            m = re.match(r"^(PASS|FAIL|SKIP|CRASH)\s+(.+?)(?:\s+\(.+\))?$", line)
            if m:
                results.append((m.group(2), m.group(1)))
                continue
            m = re.match(r"^⏱️\s+TIMEOUT\s+(.+?)(?:\s+\(.+\))?$", line)
            if m:
                results.append((m.group(1), "TIMEOUT"))
    return results


def extract_area(path, depth):
    """Extract area name from path at the given depth.

    Examples (depth=1):
      conformance/parser/ecmascript5/foo.ts -> parser
      conformance/salsa/foo.ts -> salsa
      compiler/foo.ts -> compiler

    Examples (depth=2):
      conformance/types/literal/foo.ts -> types/literal
      conformance/parser/ecmascript5/foo.ts -> parser/ecmascript5
      compiler/foo.ts -> compiler  (no deeper segment)
    """
    stripped = path
    if stripped.startswith(PATH_PREFIX):
        stripped = stripped[len(PATH_PREFIX):]

    # Strip "conformance/" prefix for cleaner names
    if stripped.startswith("conformance/"):
        stripped = stripped[len("conformance/"):]

    parts = stripped.split("/")
    # parts[-1] is the filename, so directories are parts[:-1]
    dirs = parts[:-1]
    if not dirs:
        return "(root)"
    return "/".join(dirs[:depth])


def bar_chart(ratio, width=20):
    """Create a simple bar chart string."""
    filled = int(ratio * width)
    return f"{GREEN}{'█' * filled}{DIM}{'░' * (width - filled)}{NC}"


def pass_rate_color(rate):
    """Color-code pass rate."""
    if rate >= 80:
        return GREEN
    if rate >= 50:
        return YELLOW
    return RED


def main():
    if len(sys.argv) < 2:
        print("Usage: analyze-conformance-areas.py <raw-output-file> [depth] [min-tests] [drilldown-area]")
        print()
        print("  depth          Grouping depth (1=top-level, 2=sub-areas) [default: 1]")
        print("  min-tests      Minimum tests in area to show [default: 5]")
        print("  drilldown-area Drill into a specific area (e.g., 'types')")
        sys.exit(1)

    tmpfile = sys.argv[1]
    depth = int(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] else 1
    min_tests = int(sys.argv[3]) if len(sys.argv) > 3 and sys.argv[3] else 5
    drilldown = sys.argv[4] if len(sys.argv) > 4 and sys.argv[4] else ""

    results = parse_results(tmpfile)
    if not results:
        print("No test results found in output file.")
        sys.exit(1)

    # Group by area
    areas = defaultdict(lambda: {"pass": 0, "fail": 0, "skip": 0, "crash": 0, "timeout": 0})

    for path, status in results:
        # If drilling down, filter to that area
        if drilldown:
            area_check = extract_area(path, 1)
            if area_check != drilldown:
                continue

        area = extract_area(path, depth + 1 if drilldown else depth)
        # When drilling down, strip the parent prefix for cleaner display
        if drilldown and area.startswith(drilldown + "/"):
            area = area[len(drilldown) + 1:]
        elif drilldown and area == drilldown:
            area = "(top-level files)"

        status_key = status.lower()
        if status_key in areas[area]:
            areas[area][status_key] += 1

    if not areas:
        print(f"No tests found{' in area: ' + drilldown if drilldown else ''}.")
        sys.exit(1)

    # Compute totals
    area_stats = []
    total_pass = 0
    total_fail = 0
    total_all = 0

    for area, counts in areas.items():
        total = counts["pass"] + counts["fail"] + counts["crash"] + counts["timeout"]
        if total < min_tests:
            continue
        rate = (counts["pass"] / total * 100) if total > 0 else 0
        area_stats.append({
            "area": area,
            "total": total,
            "pass": counts["pass"],
            "fail": counts["fail"],
            "crash": counts["crash"],
            "timeout": counts["timeout"],
            "skip": counts["skip"],
            "rate": rate,
            "opportunity": counts["fail"] + counts["crash"] + counts["timeout"],
        })
        total_pass += counts["pass"]
        total_fail += counts["fail"] + counts["crash"] + counts["timeout"]
        total_all += total

    # Sort by opportunity (most failures first), then by name
    area_stats.sort(key=lambda x: (-x["opportunity"], x["area"]))

    # Print header
    title = f"Drilldown: {drilldown}" if drilldown else "Conformance Area Analysis"
    print()
    print(f"{BOLD}{YELLOW}{title}{NC}")
    if drilldown:
        print(f"{DIM}Showing sub-areas within '{drilldown}' (depth={depth + 1}){NC}")
    else:
        print(f"{DIM}Grouped by directory (depth={depth}, min {min_tests} tests){NC}")
    print()

    # Column widths
    max_area_len = max(len(s["area"]) for s in area_stats) if area_stats else 20
    max_area_len = max(max_area_len, 4)  # minimum "Area"
    max_area_len = min(max_area_len, 40)  # cap at 40

    # Header row
    hdr = (
        f"  {'Area':<{max_area_len}}  "
        f"{'Total':>5}  "
        f"{'Pass':>5}  "
        f"{'Fail':>5}  "
        f"{'Rate':>6}  "
        f"{'Progress':>20}"
    )
    print(f"{BOLD}{hdr}{NC}")
    print(f"  {'─' * (max_area_len + 48)}")

    for s in area_stats:
        rate_color = pass_rate_color(s["rate"])
        bar = bar_chart(s["rate"] / 100)
        suffix = ""
        if s["crash"] > 0:
            suffix += f" {RED}({s['crash']} crash){NC}"
        if s["timeout"] > 0:
            suffix += f" {YELLOW}({s['timeout']} timeout){NC}"

        area_display = s["area"][:max_area_len]
        print(
            f"  {area_display:<{max_area_len}}  "
            f"{s['total']:>5}  "
            f"{s['pass']:>5}  "
            f"{s['opportunity']:>5}  "
            f"{rate_color}{s['rate']:>5.1f}%{NC}  "
            f"{bar}"
            f"{suffix}"
        )

    # Summary
    overall_rate = (total_pass / total_all * 100) if total_all > 0 else 0
    print(f"  {'─' * (max_area_len + 48)}")
    rate_color = pass_rate_color(overall_rate)
    print(
        f"  {'TOTAL':<{max_area_len}}  "
        f"{total_all:>5}  "
        f"{total_pass:>5}  "
        f"{total_fail:>5}  "
        f"{rate_color}{overall_rate:>5.1f}%{NC}  "
        f"{bar_chart(overall_rate / 100)}"
    )
    print()

    # Top opportunity areas (brief summary)
    top_n = min(10, len(area_stats))
    if top_n > 0 and not drilldown:
        print(f"{BOLD}{YELLOW}Top {top_n} areas by opportunity (most failures):{NC}")
        for i, s in enumerate(area_stats[:top_n], 1):
            rate_color = pass_rate_color(s["rate"])
            print(
                f"  {i:>2}. {s['area']:<{max_area_len}} "
                f"{s['opportunity']:>4} failing  "
                f"{rate_color}{s['rate']:>5.1f}% pass{NC}"
            )
        print()

    # Suggest drilldown for large areas
    if not drilldown:
        large_areas = [s for s in area_stats if s["total"] >= 50 and s["opportunity"] >= 20]
        if large_areas:
            print(f"{DIM}Tip: Drill into large areas for more detail:{NC}")
            for s in large_areas[:5]:
                print(f"  {DIM}./scripts/conformance.sh areas --drilldown {s['area']}{NC}")
            print()


if __name__ == "__main__":
    main()
