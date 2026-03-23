#!/usr/bin/env python3
"""Refresh README.md progress blocks from the latest suite artifact JSON files.

Reads:
  - scripts/conformance/conformance-detail.json  (or conformance-snapshot.json)
  - scripts/emit/emit-detail.json
  - scripts/fourslash/fourslash-detail.json

Updates the progress blocks between marker comments in README.md:
  <!-- CONFORMANCE_START --> ... <!-- CONFORMANCE_END -->
  <!-- EMIT_START --> ... <!-- EMIT_END -->
  <!-- FOURSLASH_START --> ... <!-- FOURSLASH_END -->

Usage:
  python3 scripts/refresh-readme.py          # dry-run (show diff)
  python3 scripts/refresh-readme.py --write  # write changes to README.md
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
README = ROOT / "README.md"


def progress_bar(current, total, width=20):
    if total == 0:
        return "[" + "░" * width + "] 0.0%"
    pct = current / total
    filled = round(pct * width)
    empty = width - filled
    return f"[{'█' * filled}{'░' * empty}] {pct * 100:.1f}%"


def load_conformance():
    for name in ["conformance-detail.json", "conformance-snapshot.json"]:
        p = ROOT / "scripts" / "conformance" / name
        if p.exists():
            with open(p) as f:
                data = json.load(f)
            s = data.get("summary", {})
            total = s.get("total", s.get("total_tests", 0))
            passed = s.get("passed", 0)
            return passed, total
    return None, None


def load_emit():
    p = ROOT / "scripts" / "emit" / "emit-detail.json"
    if not p.exists():
        return None
    with open(p) as f:
        data = json.load(f)
    return data.get("summary", {})


def load_fourslash():
    p = ROOT / "scripts" / "fourslash" / "fourslash-detail.json"
    if not p.exists():
        return None
    with open(p) as f:
        data = json.load(f)
    return data.get("summary", {})


def replace_block(text, start_marker, end_marker, new_content):
    start_idx = text.find(start_marker)
    end_idx = text.find(end_marker)
    if start_idx == -1 or end_idx == -1:
        return text
    after_start = start_idx + len(start_marker)
    return text[:after_start] + "\n" + new_content + "\n" + text[end_idx:]


def main():
    write = "--write" in sys.argv

    if not README.exists():
        print(f"Error: {README} not found")
        sys.exit(1)

    original = README.read_text()
    text = original

    # Conformance
    passed, total = load_conformance()
    if passed is not None:
        bar = progress_bar(passed, total)
        block = f"```\nProgress: {bar} ({passed:,}/{total:,} tests)\n```"
        text = replace_block(text, "<!-- CONFORMANCE_START -->", "<!-- CONFORMANCE_END -->", block)

    # Emit
    emit = load_emit()
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
    fs = load_fourslash()
    if fs is not None:
        bar = progress_bar(fs["passed"], fs["total"])
        block = f"```\nProgress: {bar} ({fs['passed']:,} / {fs['total']:,} tests)\n```"
        text = replace_block(text, "<!-- FOURSLASH_START -->", "<!-- FOURSLASH_END -->", block)

    if text == original:
        print("README.md is already up to date (or no artifact files found).")
        return

    if write:
        README.write_text(text)
        print("README.md updated.")
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


if __name__ == "__main__":
    main()
