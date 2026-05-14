#!/usr/bin/env python3
"""
Checker field-lifetime inventory + CI guard.

Implements `T2.1.A` from `docs/plan/PERFORMANCE_PLAN.md`:

    Add field inventory, manifest, ProgramContext/WorkerContext/FileSession
    shells. CI fails on unknown fields; no behavior change.

What this script does:

1. Parse `crates/tsz-checker/src/context/mod.rs` for the `CheckerContext<'a>`
   struct definition and extract every `pub <field>: <type>` or
   `pub(crate) <field>: <type>` line.
2. Load the manifest at
   `crates/tsz-checker/src/context/checker_context_lifetimes.toml` which maps
   each field to one of the lifetime classes described in PERFORMANCE_PLAN.md
   §6 ("Lifetime Classes"):

       ProgramStable       — immutable for one compilation
       WorkerReusable      — owned by one worker, reusable across files
       FileLocalReset      — initialized for one file, reset/dropped after
       SpeculationScoped   — must roll back on speculation abort
       DiagnosticsOnly     — affects reporting/suppression, not type answers
       LspPersistent       — survives requests, invalidated by version
       Unknown             — CI failure (must be classified)

3. Verify every CheckerContext field is present in the manifest with a
   non-`Unknown` classification. Exit non-zero on:
   - Field defined in struct but missing from manifest.
   - Field in manifest but no longer in struct (stale entry).
   - Field classified as `Unknown`.

4. Optionally generate a markdown table of the classification (`--render`).

Usage:
    python3 scripts/arch/checker_field_inventory.py            # CI guard
    python3 scripts/arch/checker_field_inventory.py --render   # markdown
    python3 scripts/arch/checker_field_inventory.py --list     # raw fields

This script intentionally lives outside `arch_guard.py` because the rule it
enforces is structurally specific (CheckerContext fields), not a generic
pattern. It is wired through `scripts/arch/check-checker-boundaries.sh` so the
field lifetime manifest stays current in the architecture guard.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
from dataclasses import dataclass

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 on some CI/self-hosted images.
    tomllib = None

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
CHECKER_CONTEXT_RS = ROOT / "crates" / "tsz-checker" / "src" / "context" / "mod.rs"
MANIFEST_TOML = (
    ROOT / "crates" / "tsz-checker" / "src" / "context" / "checker_context_lifetimes.toml"
)

VALID_LIFETIMES = frozenset(
    {
        "ProgramStable",
        "WorkerReusable",
        "FileLocalReset",
        "SpeculationScoped",
        "DiagnosticsOnly",
        "LspPersistent",
        # `Unknown` is intentionally NOT in this set: it is the explicit
        # "must classify before merge" sentinel that triggers CI failure.
    }
)

SIMPLE_INLINE_ENTRY_RE = re.compile(
    r'^\s*([A-Za-z_][A-Za-z_0-9]*)\s*=\s*\{\s*'
    r'lifetime\s*=\s*"([^"]*)"\s*,\s*'
    r'reason\s*=\s*"([^"]*)"\s*'
    r"\}\s*(?:#.*)?$"
)


@dataclass(frozen=True)
class Field:
    """One `pub <name>: <type>` line in `CheckerContext<'a>`."""

    name: str
    rust_type: str


def parse_checker_context_fields(rs_path: pathlib.Path) -> list[Field]:
    """Extract public-visible field lines from `pub struct CheckerContext<'a>`.

    Whitespace-tolerant. Strips trailing commas. Skips inner `// ---` section
    headers and fully private fields.
    """
    text = rs_path.read_text(encoding="utf-8")

    start_marker = re.compile(r"^pub struct CheckerContext<'a>\s*\{\s*$", re.MULTILINE)
    start = start_marker.search(text)
    if start is None:
        raise RuntimeError(
            f"Could not locate `pub struct CheckerContext<'a>` in {rs_path}; "
            "field-inventory script needs that exact header to anchor parsing."
        )
    body_start = start.end()

    depth = 1
    i = body_start
    while i < len(text) and depth > 0:
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    if depth != 0:
        raise RuntimeError("Unbalanced braces while scanning CheckerContext body.")

    body = text[body_start:i]

    fields: list[Field] = []
    field_start_pattern = re.compile(
        r"^\s*pub(?:\(\s*crate\s*\))?\s+([a-z_][a-z_0-9]*)\s*:\s*(.*)$",
    )
    lines = body.splitlines()
    line_idx = 0
    while line_idx < len(lines):
        match = field_start_pattern.match(lines[line_idx])
        if match is None:
            line_idx += 1
            continue

        name = match.group(1)
        type_parts = [match.group(2).strip()]
        while type_parts[-1].endswith(",") is False and line_idx + 1 < len(lines):
            line_idx += 1
            type_parts.append(lines[line_idx].strip())
        rust_type = " ".join(type_parts).removesuffix(",").strip()
        fields.append(Field(name=name, rust_type=rust_type))
        line_idx += 1
    return fields


def load_manifest(toml_path: pathlib.Path) -> dict[str, dict[str, str]]:
    """Load the lifetime manifest. Returns `{ field_name: {lifetime, reason} }`.

    Returns an empty dict if the file does not exist (first-run case before
    the manifest is created in T2.1.A.2).
    """
    if not toml_path.exists():
        return {}
    if tomllib is not None:
        with toml_path.open("rb") as fh:
            raw = tomllib.load(fh)
    else:
        raw = load_simple_inline_manifest(toml_path)
    out: dict[str, dict[str, str]] = {}
    for field_name, entry in raw.items():
        if not isinstance(entry, dict):
            continue
        lifetime = entry.get("lifetime", "")
        reason = entry.get("reason", "")
        out[field_name] = {"lifetime": lifetime, "reason": reason}
    return out


def load_simple_inline_manifest(toml_path: pathlib.Path) -> dict[str, dict[str, str]]:
    """Parse the simple inline-table manifest shape on Python < 3.11.

    The checked-in manifest intentionally uses one inline table per field:
    `field = { lifetime = "...", reason = "..." }`.
    """
    out: dict[str, dict[str, str]] = {}
    lines = toml_path.read_text(encoding="utf-8").splitlines()
    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = SIMPLE_INLINE_ENTRY_RE.match(line)
        if match is None:
            raise RuntimeError(
                f"{toml_path.relative_to(ROOT)}:{line_number}: unsupported TOML syntax "
                "for Python < 3.11 fallback parser; use one inline table with "
                '`lifetime` and `reason`, or run with Python 3.11+.'
            )
        field_name, lifetime, reason = match.groups()
        out[field_name] = {"lifetime": lifetime, "reason": reason}
    return out


def check_inventory(
    fields: list[Field],
    manifest: dict[str, dict[str, str]],
) -> list[str]:
    """Return a list of failure messages. Empty list means PASS."""
    failures: list[str] = []
    field_names = {f.name for f in fields}

    missing = sorted(field_names - manifest.keys())
    if missing:
        failures.append(
            f"{len(missing)} CheckerContext field(s) missing from manifest "
            f"({MANIFEST_TOML.relative_to(ROOT)}):"
        )
        for name in missing:
            failures.append(f"  - {name}")

    stale = sorted(manifest.keys() - field_names)
    if stale:
        failures.append(
            f"{len(stale)} stale manifest entry/entries (field no longer in struct):"
        )
        for name in stale:
            failures.append(f"  - {name}")

    bad_class: list[tuple[str, str]] = []
    unknown: list[str] = []
    for name in sorted(field_names & manifest.keys()):
        lifetime = manifest[name]["lifetime"]
        if lifetime == "Unknown":
            unknown.append(name)
        elif lifetime not in VALID_LIFETIMES:
            bad_class.append((name, lifetime))

    if unknown:
        failures.append(
            f"{len(unknown)} field(s) classified as `Unknown` (must classify "
            "before merge per PERFORMANCE_PLAN.md §6):"
        )
        for name in unknown:
            failures.append(f"  - {name}")

    if bad_class:
        valid = ", ".join(sorted(VALID_LIFETIMES)) + ", or `Unknown`"
        failures.append(f"{len(bad_class)} field(s) with invalid lifetime class (must be one of: {valid}):")
        for name, cls in bad_class:
            failures.append(f"  - {name}: {cls!r}")

    return failures


def render_markdown(
    fields: list[Field],
    manifest: dict[str, dict[str, str]],
) -> str:
    """Render a markdown table grouped by lifetime class for PR review."""
    by_class: dict[str, list[tuple[str, str, str]]] = {}
    for f in fields:
        entry = manifest.get(f.name, {"lifetime": "Unknown", "reason": ""})
        by_class.setdefault(entry["lifetime"], []).append(
            (f.name, f.rust_type, entry.get("reason", ""))
        )

    lines = [
        "# CheckerContext Field Lifetime Inventory",
        "",
        f"Auto-generated by `scripts/arch/checker_field_inventory.py --render`.",
        f"Source: `{CHECKER_CONTEXT_RS.relative_to(ROOT)}`",
        f"Manifest: `{MANIFEST_TOML.relative_to(ROOT)}`",
        "",
        f"Total fields: {len(fields)}",
        "",
    ]

    order = [
        "ProgramStable",
        "WorkerReusable",
        "FileLocalReset",
        "SpeculationScoped",
        "DiagnosticsOnly",
        "LspPersistent",
        "Unknown",
    ]
    seen_classes = set()
    for cls in order:
        rows = by_class.get(cls, [])
        if not rows:
            continue
        seen_classes.add(cls)
        lines.append(f"## {cls} ({len(rows)})")
        lines.append("")
        lines.append("| Field | Type | Reason |")
        lines.append("| --- | --- | --- |")
        for name, ty, reason in sorted(rows):
            ty_md = ty.replace("|", r"\|")
            reason_md = reason.replace("|", r"\|") if reason else ""
            lines.append(f"| `{name}` | `{ty_md}` | {reason_md} |")
        lines.append("")

    extra = sorted(set(by_class.keys()) - seen_classes)
    for cls in extra:
        rows = by_class[cls]
        lines.append(f"## {cls} ({len(rows)}) — INVALID CLASS")
        lines.append("")
        for name, _ty, _reason in sorted(rows):
            lines.append(f"- `{name}`")
        lines.append("")

    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--render", action="store_true", help="Print markdown report.")
    ap.add_argument("--list", action="store_true", help="Print raw field list.")
    args = ap.parse_args()

    fields = parse_checker_context_fields(CHECKER_CONTEXT_RS)
    manifest = load_manifest(MANIFEST_TOML)

    if args.list:
        for f in fields:
            print(f"{f.name}\t{f.rust_type}")
        return 0

    if args.render:
        print(render_markdown(fields, manifest))
        return 0

    failures = check_inventory(fields, manifest)
    if failures:
        print("Checker field-lifetime inventory FAILED:", file=sys.stderr)
        for line in failures:
            print(line, file=sys.stderr)
        print(
            "\nFix by editing the manifest at "
            f"{MANIFEST_TOML.relative_to(ROOT)} per PERFORMANCE_PLAN.md §6.",
            file=sys.stderr,
        )
        return 1

    print(
        f"Checker field-lifetime inventory passed: {len(fields)} field(s) "
        "all classified."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
