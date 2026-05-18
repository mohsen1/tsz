#!/usr/bin/env python3
"""
Checker field-lifetime + capability-group inventory CI guard.

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

   and to one of the capability groups (§8224):

       CheckerInputs       — core config/input for the current file session
       ProgramLookup       — program-wide shared indices and module-resolution
       FileTypeCache       — per-file type/symbol/class/namespace/JSX caches
       SpeculationState    — rollback-sensitive state for speculative checking
       DiagnosticState     — diagnostic emission, suppression, and accumulation
       FlowSession         — control-flow analysis caches and worklists
       RelationSession     — type relation, evaluation, and instantiation
       EmitSummary         — declaration-emit output tables

3. Verify every CheckerContext field is present in the manifest with a
   non-`Unknown` lifetime classification and a valid capability group.
   Exit non-zero on:
   - Field defined in struct but missing from manifest.
   - Field in manifest but no longer in struct (stale entry).
   - Field classified as `Unknown`.
   - Field missing capability_group or with invalid capability_group.

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

VALID_CAPABILITY_GROUPS = frozenset(
    {
        "CheckerInputs",    # core config/input for the current file session
        "ProgramLookup",    # program-wide shared indices and module-resolution data
        "FileTypeCache",    # per-file type/symbol/class/namespace/JSX caches
        "SpeculationState", # rollback-sensitive state for speculative checking
        "DiagnosticState",  # diagnostic emission, suppression, and accumulation
        "FlowSession",      # control-flow analysis caches and worklists
        "RelationSession",  # type relation, evaluation, and instantiation state
        "EmitSummary",      # declaration-emit output tables
    }
)
VALID_CAPABILITY_GROUPS_STR = ", ".join(sorted(VALID_CAPABILITY_GROUPS))

# Python < 3.11 fallback: matches the 3-field inline-table format used in the manifest.
# Field order must be: lifetime, capability_group, reason.
SIMPLE_INLINE_ENTRY_RE = re.compile(
    r'^\s*([A-Za-z_][A-Za-z_0-9]*)\s*=\s*\{\s*'
    r'lifetime\s*=\s*"([^"]*)"\s*,\s*'
    r'capability_group\s*=\s*"([^"]*)"\s*,\s*'
    r'reason\s*=\s*"([^"]*)"\s*'
    r"\}\s*(?:#.*)?$"
)

# Also accept the legacy 2-field format (lifetime + reason only) for backwards
# compatibility while manifests are being migrated; capability_group will be
# reported as missing in check_inventory.
SIMPLE_INLINE_ENTRY_LEGACY_RE = re.compile(
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
    """Load the manifest. Returns `{ field_name: {lifetime, capability_group, reason} }`.

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
        capability_group = entry.get("capability_group", "")
        reason = entry.get("reason", "")
        out[field_name] = {
            "lifetime": lifetime,
            "capability_group": capability_group,
            "reason": reason,
        }
    return out


def load_simple_inline_manifest(toml_path: pathlib.Path) -> dict[str, dict[str, str]]:
    """Parse the simple inline-table manifest shape on Python < 3.11.

    The checked-in manifest uses one inline table per field:
    `field = { lifetime = "...", capability_group = "...", reason = "..." }`.
    """
    out: dict[str, dict[str, str]] = {}
    lines = toml_path.read_text(encoding="utf-8").splitlines()
    for line_number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = SIMPLE_INLINE_ENTRY_RE.match(line)
        if match is not None:
            field_name, lifetime, capability_group, reason = match.groups()
            out[field_name] = {
                "lifetime": lifetime,
                "capability_group": capability_group,
                "reason": reason,
            }
            continue
        # Accept legacy 2-field format; capability_group will be flagged as missing.
        legacy_match = SIMPLE_INLINE_ENTRY_LEGACY_RE.match(line)
        if legacy_match is not None:
            field_name, lifetime, reason = legacy_match.groups()
            out[field_name] = {"lifetime": lifetime, "capability_group": "", "reason": reason}
            continue
        raise RuntimeError(
            f"{toml_path.relative_to(ROOT)}:{line_number}: unsupported TOML syntax "
            "for Python < 3.11 fallback parser; use one inline table with "
            '`lifetime`, `capability_group`, and `reason`, or run with Python 3.11+.'
        )
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

    bad_lifetime: list[tuple[str, str]] = []
    unknown_lifetime: list[str] = []
    missing_group: list[str] = []
    bad_group: list[tuple[str, str]] = []

    for name in sorted(field_names & manifest.keys()):
        entry = manifest[name]
        lifetime = entry["lifetime"]
        capability_group = entry.get("capability_group", "")

        if lifetime == "Unknown":
            unknown_lifetime.append(name)
        elif lifetime not in VALID_LIFETIMES:
            bad_lifetime.append((name, lifetime))

        if not capability_group:
            missing_group.append(name)
        elif capability_group not in VALID_CAPABILITY_GROUPS:
            bad_group.append((name, capability_group))

    if unknown_lifetime:
        failures.append(
            f"{len(unknown_lifetime)} field(s) classified as `Unknown` (must classify "
            "before merge per PERFORMANCE_PLAN.md §6):"
        )
        for name in unknown_lifetime:
            failures.append(f"  - {name}")

    if bad_lifetime:
        valid = ", ".join(sorted(VALID_LIFETIMES)) + ", or `Unknown`"
        failures.append(
            f"{len(bad_lifetime)} field(s) with invalid lifetime class "
            f"(must be one of: {valid}):"
        )
        for name, cls in bad_lifetime:
            failures.append(f"  - {name}: {cls!r}")

    if missing_group:
        failures.append(
            f"{len(missing_group)} field(s) missing capability_group "
            f"(must be one of: {VALID_CAPABILITY_GROUPS_STR}):"
        )
        for name in missing_group:
            failures.append(f"  - {name}")

    if bad_group:
        failures.append(
            f"{len(bad_group)} field(s) with invalid capability_group "
            f"(must be one of: {VALID_CAPABILITY_GROUPS_STR}):"
        )
        for name, grp in bad_group:
            failures.append(f"  - {name}: {grp!r}")

    return failures


def render_markdown(
    fields: list[Field],
    manifest: dict[str, dict[str, str]],
) -> str:
    """Render markdown tables grouped by capability group and lifetime class."""
    by_group: dict[str, list[tuple[str, str, str]]] = {}
    for f in fields:
        entry = manifest.get(
            f.name, {"lifetime": "Unknown", "capability_group": "Unknown", "reason": ""}
        )
        group = entry.get("capability_group") or "Unknown"
        by_group.setdefault(group, []).append(
            (f.name, entry.get("lifetime", ""), entry.get("reason", ""))
        )

    lines = [
        "# CheckerContext Field Inventory",
        "",
        "Auto-generated by `scripts/arch/checker_field_inventory.py --render`.",
        f"Source: `{CHECKER_CONTEXT_RS.relative_to(ROOT)}`",
        f"Manifest: `{MANIFEST_TOML.relative_to(ROOT)}`",
        "",
        f"Total fields: {len(fields)}",
        "",
    ]

    group_order = [
        "CheckerInputs",
        "ProgramLookup",
        "FileTypeCache",
        "SpeculationState",
        "DiagnosticState",
        "FlowSession",
        "RelationSession",
        "EmitSummary",
        "Unknown",
    ]
    seen_groups: set[str] = set()
    for grp in group_order:
        rows = by_group.get(grp, [])
        if not rows:
            continue
        seen_groups.add(grp)
        label = grp if grp != "Unknown" else f"{grp} — INVALID"
        lines.append(f"## {label} ({len(rows)})")
        lines.append("")
        lines.append("| Field | Lifetime | Reason |")
        lines.append("| --- | --- | --- |")
        for name, lifetime, reason in sorted(rows):
            reason_md = reason.replace("|", r"\|") if reason else ""
            lines.append(f"| `{name}` | `{lifetime}` | {reason_md} |")
        lines.append("")

    for grp in sorted(set(by_group.keys()) - seen_groups):
        rows = by_group[grp]
        lines.append(f"## {grp} — INVALID GROUP ({len(rows)})")
        lines.append("")
        for name, _lt, _reason in sorted(rows):
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
