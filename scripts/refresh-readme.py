#!/usr/bin/env python3
"""Refresh README.md progress blocks from the latest suite artifact JSON files.

Reads:
  - .ci-metrics/conformance.json (or scripts/conformance/conformance-detail.json)
  - .ci-metrics/emit.json (or scripts/emit/emit-detail.json)
  - .ci-metrics/fourslash.json (or scripts/fourslash/fourslash-detail.json)
  - https://tsz.dev/benchmark-data/latest.json

Updates the progress blocks between marker comments in README.md:
  <!-- PERFORMANCE_START --> ... <!-- PERFORMANCE_END -->
  <!-- CONFORMANCE_START --> ... <!-- CONFORMANCE_END -->
  <!-- EMIT_START --> ... <!-- EMIT_END -->
  <!-- FOURSLASH_START --> ... <!-- FOURSLASH_END -->

Usage:
  python3 scripts/refresh-readme.py                           # dry-run
  python3 scripts/refresh-readme.py --write                   # write README.md and performance PNG
  python3 scripts/refresh-readme.py --benchmark-json bench.json --write
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).parent.parent
README = ROOT / "README.md"
PERFORMANCE_PNG_DIR = ROOT / "crates" / "tsz-website" / "static" / "benchmark-data"
PERFORMANCE_PNG_LIGHT = PERFORMANCE_PNG_DIR / "readme-perf-light.png"
PERFORMANCE_PNG_DARK = PERFORMANCE_PNG_DIR / "readme-perf-dark.png"
PERFORMANCE_BENCHMARK_URL = "https://tsz.dev/benchmark-data/latest.json"


def progress_bar(current, total, width=20):
    if total == 0:
        return "[" + "░" * width + "] 0.0%"
    pct = current / total
    filled = round(pct * width)
    empty = width - filled
    return f"[{'█' * filled}{'░' * empty}] {pct * 100:.1f}%"


def load_metric_json(path):
    with open(path) as f:
        return json.load(f)


def normalize_suite_summary(data, suite):
    summary = data.get("summary")
    if summary:
        return {
            "passed": summary.get("passed"),
            "total": summary.get("total", summary.get("total_tests")),
        }

    if data.get("suite") != suite:
        return None

    return {
        "passed": data.get("passed"),
        "total": data.get("total"),
    }


def suite_metric_candidates(explicit_path, default_paths):
    candidates = []
    if explicit_path is not None:
        path = explicit_path
        if not path.is_absolute():
            path = ROOT / path
        candidates.append(path)
    candidates.extend(default_paths)
    return candidates


def load_suite_counts(suite, explicit_path, default_paths):
    for p in suite_metric_candidates(explicit_path, default_paths):
        if not p.exists():
            continue
        summary = normalize_suite_summary(load_metric_json(p), suite)
        if summary is None:
            continue
        passed = summary.get("passed")
        total = summary.get("total")
        if passed is not None and total is not None:
            return passed, total
    return None, None


def load_conformance(args):
    return load_suite_counts(
        "conformance",
        args.conformance_metrics_json,
        [
            ROOT / ".ci-metrics" / "conformance.json",
            ROOT / "scripts" / "conformance" / "conformance-snapshot.json",
            ROOT / "scripts" / "conformance" / "conformance-detail.json",
        ],
    )


def normalize_emit_summary(data):
    summary = data.get("summary")
    if summary:
        return summary

    if data.get("suite") != "emit":
        return None

    return {
        "jsPass": data.get("js_passed"),
        "jsTotal": data.get("js_total"),
        "jsSkip": data.get("js_skipped", 0),
        "jsTimeout": data.get("js_timeouts", 0),
        "dtsPass": data.get("dts_passed"),
        "dtsTotal": data.get("dts_total"),
        "dtsSkip": data.get("dts_skipped", 0),
    }


def emit_summary_from_readme(text):
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

    if {"jsPass", "jsTotal", "dtsPass", "dtsTotal"}.issubset(summary):
        summary.setdefault("jsSkip", 0)
        summary.setdefault("jsTimeout", 0)
        summary.setdefault("dtsSkip", 0)
        return summary
    return None


def prefer_readme_emit_summary(candidate, readme_summary):
    if readme_summary is None:
        return candidate

    same_domain = (
        readme_summary.get("jsTotal") == candidate.get("jsTotal")
        and readme_summary.get("dtsTotal") == candidate.get("dtsTotal")
    )
    ahead_or_equal = (
        readme_summary.get("jsPass", 0) >= candidate.get("jsPass", 0)
        and readme_summary.get("dtsPass", 0) >= candidate.get("dtsPass", 0)
    )
    return readme_summary if same_domain and ahead_or_equal else candidate


def load_emit(args, readme_text):
    readme_summary = emit_summary_from_readme(readme_text)
    candidates = []
    if args.emit_metrics_json is not None:
        path = args.emit_metrics_json
        if not path.is_absolute():
            path = ROOT / path
        candidates.append(path)
    candidates.extend([
        ROOT / ".ci-metrics" / "emit.json",
        ROOT / "scripts" / "emit" / "emit-detail.json",
    ])

    for p in candidates:
        if not p.exists():
            continue
        with open(p) as f:
            data = json.load(f)
        summary = normalize_emit_summary(data)
        if summary is not None:
            if args.emit_metrics_json is None and p == ROOT / "scripts" / "emit" / "emit-detail.json":
                return prefer_readme_emit_summary(summary, readme_summary)
            return summary
    return None


def load_fourslash(args):
    passed, total = load_suite_counts(
        "fourslash",
        args.fourslash_metrics_json,
        [
            ROOT / ".ci-metrics" / "fourslash.json",
            ROOT / "scripts" / "fourslash" / "fourslash-detail.json",
            ROOT / "scripts" / "fourslash" / "fourslash-snapshot.json",
        ],
    )
    if passed is None or total is None:
        return None
    return {"passed": passed, "total": total}


def replace_block(text, start_marker, end_marker, new_content):
    start_idx = text.find(start_marker)
    end_idx = text.find(end_marker)
    if start_idx == -1 or end_idx == -1:
        return text
    after_start = start_idx + len(start_marker)
    return text[:after_start] + "\n" + new_content + "\n" + text[end_idx:]


def performance_block():
    return (
        '<p align="left">\n'
        '  <a href="https://tsz.dev/benchmarks/">\n'
        '    <picture>\n'
        '      <source media="(prefers-color-scheme: dark)" '
        'srcset="crates/tsz-website/static/benchmark-data/readme-perf-dark.png">\n'
        '      <source media="(prefers-color-scheme: light)" '
        'srcset="crates/tsz-website/static/benchmark-data/readme-perf-light.png">\n'
        '      <img src="crates/tsz-website/static/benchmark-data/readme-perf-light.png" '
        'alt="Latest tsz vs tsgo benchmark performance" width="760">\n'
        '    </picture>\n'
        '  </a>\n'
        '</p>'
    )


def parse_args():
    parser = argparse.ArgumentParser(
        description="Refresh generated README.md blocks and benchmark chart assets.",
    )
    parser.add_argument("--write", action="store_true", help="write README.md and generated assets")
    parser.add_argument(
        "--benchmark-json",
        type=Path,
        help="use a local benchmark artifact instead of fetching the published latest.json",
    )
    parser.add_argument(
        "--benchmark-url",
        default=PERFORMANCE_BENCHMARK_URL,
        help="published benchmark artifact URL used for the README performance chart",
    )
    parser.add_argument(
        "--skip-performance",
        action="store_true",
        help="skip the README performance chart block and PNG generation",
    )
    parser.add_argument(
        "--emit-metrics-json",
        type=Path,
        help="use a local emit metrics artifact instead of the default .ci-metrics/emit.json",
    )
    parser.add_argument(
        "--conformance-metrics-json",
        type=Path,
        help="use a local conformance metrics artifact instead of the default .ci-metrics/conformance.json",
    )
    parser.add_argument(
        "--fourslash-metrics-json",
        type=Path,
        help="use a local fourslash metrics artifact instead of the default .ci-metrics/fourslash.json",
    )
    return parser.parse_args()


def load_benchmark_json(args):
    if args.skip_performance:
        return None, None
    if args.benchmark_json is not None:
        path = args.benchmark_json
        if not path.is_absolute():
            path = ROOT / path
        if not path.exists():
            raise SystemExit(f"Error: benchmark artifact not found: {path}")
        return path, None

    try:
        request = urllib.request.Request(
            args.benchmark_url,
            headers={"User-Agent": "tsz-refresh-readme/1.0"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read()
    except Exception as exc:
        print(
            f"Warning: unable to fetch benchmark artifact from {args.benchmark_url}: {exc}",
            file=sys.stderr,
        )
        return None, None

    temp = tempfile.NamedTemporaryFile(
        prefix="tsz-readme-benchmark-",
        suffix=".json",
        delete=False,
    )
    try:
        temp.write(data)
        return Path(temp.name), Path(temp.name)
    finally:
        temp.close()


def write_performance_png(benchmark_json):
    PERFORMANCE_PNG_DIR.mkdir(parents=True, exist_ok=True)
    for output, theme in [
        (PERFORMANCE_PNG_LIGHT, "light"),
        (PERFORMANCE_PNG_DARK, "dark"),
    ]:
        subprocess.run(
            [
                "node",
                str(ROOT / "scripts" / "bench" / "readme-perf-svg.mjs"),
                "--theme",
                theme,
                str(benchmark_json),
                str(output),
            ],
            cwd=ROOT,
            env={**os.environ, "TSZ_README_PERF_REQUIRE_SHARP": "1"},
            check=True,
        )


def main():
    args = parse_args()
    write = args.write

    if not README.exists():
        print(f"Error: {README} not found")
        sys.exit(1)

    benchmark_json, temp_benchmark_json = load_benchmark_json(args)

    original = README.read_text()
    text = original

    # Performance
    if benchmark_json is not None:
        text = replace_block(
            text,
            "<!-- PERFORMANCE_START -->",
            "<!-- PERFORMANCE_END -->",
            performance_block(),
        )

    # Conformance
    passed, total = load_conformance(args)
    if passed is not None:
        bar = progress_bar(passed, total)
        block = f"```\nProgress: {bar} ({passed:,}/{total:,} tests)\n```"
        text = replace_block(text, "<!-- CONFORMANCE_START -->", "<!-- CONFORMANCE_END -->", block)

    # Emit
    emit = load_emit(args, original)
    if emit is not None:
        js_bar = progress_bar(emit["jsPass"], emit["jsTotal"])
        dts_bar = progress_bar(emit["dtsPass"], emit["dtsTotal"])
        block = (
            f"```\n"
            f"JavaScript:  {js_bar} ({emit['jsPass']:,} / {emit['jsTotal']:,} tests)\n"
            f"Declaration: {dts_bar} ({emit['dtsPass']:,} / {emit['dtsTotal']:,} tests)\n"
            f"```"
        )
        text = replace_block(text, "<!-- EMIT_START -->", "<!-- EMIT_END -->", block)

    # Fourslash
    fs = load_fourslash(args)
    if fs is not None:
        bar = progress_bar(fs["passed"], fs["total"])
        block = f"```\nProgress: {bar} ({fs['passed']:,} / {fs['total']:,} tests)\n```"
        text = replace_block(text, "<!-- FOURSLASH_START -->", "<!-- FOURSLASH_END -->", block)

    try:
        if text == original and not (write and benchmark_json is not None):
            print("README.md is already up to date (or no artifact files found).")
            return

        if write:
            if benchmark_json is not None:
                write_performance_png(benchmark_json)
                print(f"{PERFORMANCE_PNG_LIGHT.relative_to(ROOT)} updated.")
                print(f"{PERFORMANCE_PNG_DARK.relative_to(ROOT)} updated.")
            if text != original:
                README.write_text(text)
                print("README.md updated.")
            elif benchmark_json is None:
                print("README.md is already up to date (or no artifact files found).")
        else:
            # Show what would change
            import difflib
            diff = difflib.unified_diff(
                original.splitlines(keepends=True),
                text.splitlines(keepends=True),
                fromfile="README.md (before)",
                tofile="README.md (after)",
            )
            sys.stdout.writelines(diff)
            print("\nDry run. Pass --write to apply changes.")
    finally:
        if temp_benchmark_json is not None:
            temp_benchmark_json.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
