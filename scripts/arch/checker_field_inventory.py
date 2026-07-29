#!/usr/bin/env python3
"""
Checker field-lifetime inventory + CI guard.

Implements `T2.1.A` from `docs/plan/PERFORMANCE_PLAN.md`:

    Add field inventory, manifest, ProgramContext/WorkerContext/FileSession
    shells. CI fails on unknown fields; no behavior change.

What this script does:

1. Parse `crates/tsz-checker/src/context/mod.rs` for the `CheckerContext<'a>`
   struct definition and extract every `<field>: <type>` line, regardless of
   field visibility.
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

   The manifest also records the owner capability group that should mediate
   access to the field:

       CheckerInputs
       ProgramLookupContext
       FileTypeCache
       SpeculationState
       DiagnosticState
       FlowSessionState
       RelationSessionState
       EmitSummaryState

3. Verify every CheckerContext field is present in the manifest with a
   non-`Unknown` lifetime and capability classification. Exit non-zero on:
   - Field defined in struct but missing from manifest.
   - Field in manifest but no longer in struct (stale entry).
   - Field classified as `Unknown` for lifetime or capability.

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

VALID_CAPABILITIES = frozenset(
    {
        "CheckerInputs",
        "DiagnosticState",
        "EmitSummaryState",
        "FileTypeCache",
        "FlowSessionState",
        "ProgramLookupContext",
        "RelationSessionState",
        "SpeculationState",
    }
)

# Canonical lifetime-class -> destination decomposition shell.
#
# Each field's lifetime class names the shell that owns it once the
# `CheckerContext` god-object is split per the T2.1 decomposition (see the
# `crates/tsz-checker/src/context/lifetime_shells.rs` module doc). That mapping
# previously lived only in prose, so the shells could silently drift from the
# manifest taxonomy and the field-migration PRs had no machine-checked
# destination. Encoding it here makes the decomposition target a CI-enforced
# contract: a new lifetime class must declare its destination shell, and a
# mapped shell must exist as a real `pub struct`.
LIFETIME_DESTINATION_SHELL = {
    "ProgramStable": "ProgramContext",
    "WorkerReusable": "WorkerContext",
    "FileLocalReset": "FileSession",
    "DiagnosticsOnly": "FileSession",
    "SpeculationScoped": "SpeculationScope",
    "LspPersistent": "LspPersistentCache",
}

# `ProgramContext` predates the T2.1 shells and lives in its own module; the
# remaining shells live in `lifetime_shells.rs`. Both files are scanned for the
# `pub struct <Shell>` declaration so the destination contract stays honest if a
# shell is renamed or removed.
LIFETIME_SHELLS_RS = (
    ROOT / "crates" / "tsz-checker" / "src" / "context" / "lifetime_shells.rs"
)
PROGRAM_CONTEXT_RS = (
    ROOT / "crates" / "tsz-checker" / "src" / "context" / "program_context.rs"
)
DESTINATION_SHELL_SOURCES = (LIFETIME_SHELLS_RS, PROGRAM_CONTEXT_RS)

PUB_STRUCT_RE = re.compile(r"^\s*pub struct ([A-Za-z_][A-Za-z_0-9]*)", re.MULTILINE)

SIMPLE_INLINE_ENTRY_RE = re.compile(
    r'^\s*([A-Za-z_][A-Za-z_0-9]*)\s*=\s*\{\s*'
    r'lifetime\s*=\s*"([^"]*)"\s*,\s*'
    r'capability\s*=\s*"([^"]*)"\s*,\s*'
    r'reason\s*=\s*"([^"]*)"\s*'
    r"\}\s*(?:#.*)?$"
)


@dataclass(frozen=True)
class Field:
    """One `<name>: <type>` line in `CheckerContext<'a>`."""

    name: str
    rust_type: str


def parse_checker_context_fields(rs_path: pathlib.Path) -> list[Field]:
    """Extract every field line from `pub struct CheckerContext<'a>`.

    Whitespace-tolerant. Strips trailing commas and accepts public,
    crate-visible, and private fields.
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
        r"^\s*(?:pub(?:\(\s*crate\s*\))?\s+)?"
        r"([a-z_][a-z_0-9]*)\s*:\s*(.*)$",
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
    """Load the manifest. Returns `{ field_name: {lifetime, capability, reason} }`.

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
        capability = entry.get("capability", "")
        reason = entry.get("reason", "")
        out[field_name] = {
            "lifetime": lifetime,
            "capability": capability,
            "reason": reason,
        }
    return out


def load_simple_inline_manifest(toml_path: pathlib.Path) -> dict[str, dict[str, str]]:
    """Parse the simple inline-table manifest shape on Python < 3.11.

    The checked-in manifest intentionally uses one inline table per field:
    `field = { lifetime = "...", capability = "...", reason = "..." }`.
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
                '`lifetime`, `capability`, and `reason`, or run with Python 3.11+.'
            )
        field_name, lifetime, capability, reason = match.groups()
        out[field_name] = {
            "lifetime": lifetime,
            "capability": capability,
            "reason": reason,
        }
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
    bad_capability: list[tuple[str, str]] = []
    unknown_capability: list[str] = []
    missing_capability: list[str] = []
    for name in sorted(field_names & manifest.keys()):
        lifetime = manifest[name]["lifetime"]
        if lifetime == "Unknown":
            unknown_lifetime.append(name)
        elif lifetime not in VALID_LIFETIMES:
            bad_lifetime.append((name, lifetime))
        capability = manifest[name].get("capability", "")
        if not capability:
            missing_capability.append(name)
        elif capability == "Unknown":
            unknown_capability.append(name)
        elif capability not in VALID_CAPABILITIES:
            bad_capability.append((name, capability))

    if unknown_lifetime:
        failures.append(
            f"{len(unknown_lifetime)} field(s) with lifetime classified as `Unknown` (must classify "
            "before merge per PERFORMANCE_PLAN.md §6):"
        )
        for name in unknown_lifetime:
            failures.append(f"  - {name}")

    if bad_lifetime:
        valid = ", ".join(sorted(VALID_LIFETIMES)) + ", or `Unknown`"
        failures.append(f"{len(bad_lifetime)} field(s) with invalid lifetime class (must be one of: {valid}):")
        for name, cls in bad_lifetime:
            failures.append(f"  - {name}: {cls!r}")

    if missing_capability:
        failures.append(
            f"{len(missing_capability)} field(s) missing capability group:"
        )
        for name in missing_capability:
            failures.append(f"  - {name}")

    if unknown_capability:
        failures.append(
            f"{len(unknown_capability)} field(s) with capability classified as `Unknown`:"
        )
        for name in unknown_capability:
            failures.append(f"  - {name}")

    if bad_capability:
        valid = ", ".join(sorted(VALID_CAPABILITIES)) + ", or `Unknown`"
        failures.append(f"{len(bad_capability)} field(s) with invalid capability group (must be one of: {valid}):")
        for name, cls in bad_capability:
            failures.append(f"  - {name}: {cls!r}")

    return failures


def declared_shell_structs(
    sources: tuple[pathlib.Path, ...] = DESTINATION_SHELL_SOURCES,
) -> set[str]:
    """Return the set of `pub struct <Name>` declarations across `sources`.

    Used to prove that every destination shell named by
    `LIFETIME_DESTINATION_SHELL` actually exists as a real type, so the
    decomposition target cannot silently drift from the manifest taxonomy.
    """
    names: set[str] = set()
    for path in sources:
        if not path.exists():
            continue
        names.update(PUB_STRUCT_RE.findall(path.read_text(encoding="utf-8")))
    return names


def check_destination_shells(declared_shells: set[str]) -> list[str]:
    """Enforce the lifetime-class -> destination-shell decomposition contract.

    Returns a list of failure messages (empty means PASS). Fails when a valid
    lifetime class has no destination shell mapped, or when a mapped shell does
    not exist as a `pub struct`. This keeps the `lifetime_shells.rs`
    decomposition target in lockstep with the manifest's lifetime taxonomy.
    """
    failures: list[str] = []

    unmapped = sorted(VALID_LIFETIMES - LIFETIME_DESTINATION_SHELL.keys())
    if unmapped:
        failures.append(
            f"{len(unmapped)} lifetime class(es) with no destination shell in "
            "LIFETIME_DESTINATION_SHELL (every valid lifetime must name its "
            "decomposition target shell):"
        )
        for cls in unmapped:
            failures.append(f"  - {cls}")

    missing_shells = sorted(
        {
            shell
            for shell in LIFETIME_DESTINATION_SHELL.values()
            if shell not in declared_shells
        }
    )
    if missing_shells:
        shell_files = " / ".join(
            str(path.relative_to(ROOT)) for path in DESTINATION_SHELL_SOURCES
        )
        failures.append(
            f"{len(missing_shells)} destination shell(s) mapped by a lifetime "
            f"class but not declared as `pub struct` (expected in {shell_files}):"
        )
        for shell in missing_shells:
            owners = sorted(
                cls
                for cls, dest in LIFETIME_DESTINATION_SHELL.items()
                if dest == shell
            )
            failures.append(f"  - {shell} (owns: {', '.join(owners)})")

    return failures


def render_shell_progress(
    fields: list[Field],
    manifest: dict[str, dict[str, str]],
) -> str:
    """Render the per-shell migration burndown: how many `CheckerContext`
    fields still live in the god-object for each destination shell."""
    counts: dict[str, int] = {}
    for f in fields:
        entry = manifest.get(f.name)
        if entry is None:
            continue
        shell = LIFETIME_DESTINATION_SHELL.get(entry["lifetime"], "UNMAPPED")
        counts[shell] = counts.get(shell, 0) + 1

    lines = [
        "# CheckerContext Decomposition Burndown",
        "",
        "Fields still owned by the `CheckerContext` god-object, grouped by the",
        "destination shell that will own them after the T2.1 split.",
        "",
        f"Total fields: {len(fields)}",
        "",
        "| Destination shell | Fields remaining in CheckerContext |",
        "| --- | ---: |",
    ]
    for shell in sorted(counts, key=lambda s: (-counts[s], s)):
        lines.append(f"| `{shell}` | {counts[shell]} |")
    return "\n".join(lines)


def render_markdown(
    fields: list[Field],
    manifest: dict[str, dict[str, str]],
) -> str:
    """Render a markdown table grouped by lifetime class for PR review."""
    by_class: dict[str, list[tuple[str, str, str]]] = {}
    for f in fields:
        entry = manifest.get(
            f.name,
            {"lifetime": "Unknown", "capability": "Unknown", "reason": ""},
        )
        by_class.setdefault(entry["lifetime"], []).append(
            (f.name, f.rust_type, entry.get("capability", "Unknown"), entry.get("reason", ""))
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
        lines.append("| Field | Type | Capability | Reason |")
        lines.append("| --- | --- | --- | --- |")
        for name, ty, capability, reason in sorted(rows):
            ty_md = ty.replace("|", r"\|")
            capability_md = capability.replace("|", r"\|") if capability else ""
            reason_md = reason.replace("|", r"\|") if reason else ""
            lines.append(f"| `{name}` | `{ty_md}` | `{capability_md}` | {reason_md} |")
        lines.append("")

    extra = sorted(set(by_class.keys()) - seen_classes)
    for cls in extra:
        rows = by_class[cls]
        lines.append(f"## {cls} ({len(rows)}) — INVALID CLASS")
        lines.append("")
        for name, _ty, _capability, _reason in sorted(rows):
            lines.append(f"- `{name}`")
        lines.append("")

    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--render", action="store_true", help="Print markdown report.")
    ap.add_argument(
        "--shells",
        action="store_true",
        help="Print the per-shell decomposition burndown table.",
    )
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

    if args.shells:
        print(render_shell_progress(fields, manifest))
        return 0

    failures = check_inventory(fields, manifest)
    failures += check_destination_shells(declared_shell_structs())
    if failures:
        print("Checker field-lifetime inventory FAILED:", file=sys.stderr)
        for line in failures:
            print(line, file=sys.stderr)
        print(
            "\nFix by editing the manifest at "
            f"{MANIFEST_TOML.relative_to(ROOT)} (lifetime/capability classes) or "
            f"the destination shells in {LIFETIME_SHELLS_RS.relative_to(ROOT)} "
            "(decomposition contract).",
            file=sys.stderr,
        )
        return 1

    print(
        f"Checker field-lifetime inventory passed: {len(fields)} field(s) "
        "all classified; destination-shell contract intact."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
