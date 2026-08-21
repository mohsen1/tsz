#!/usr/bin/env python3
"""Audit emitter and emit-harness output-surgery rewrites.

This script is intentionally conservative: harmless string-data cleanup is
allowed automatically, while semantic rewrites over already-emitted JS/DTS are
treated as ratcheted debt. Current debt is listed in
`output-surgery-allowlist.txt` by file, category, max count, and reason.

The retained emit harness is part of the truth perimeter. Its canonical row
runs TSZ once and the pinned TypeScript 7 oracle once with the same authored
options, then compares their complete product maps byte-for-byte. Retrying with
weaker flags, using checked-in baseline bytes,
repairing output, or promoting tolerant comparisons to passes is forbidden.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import re
import subprocess
import sys
from collections import Counter, defaultdict
from typing import Optional


ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_ROOT = ROOT / "crates" / "tsz-core" / "src" / "emit.rs"
ALLOWLIST_PATH = ROOT / "scripts" / "emit" / "output-surgery-allowlist.txt"
HARNESS_TRUTH_SOURCES = (
    ROOT / "scripts" / "emit" / "src" / "cli-transpiler.ts",
    ROOT / "scripts" / "emit" / "src" / "runner.ts",
    ROOT / "scripts" / "emit" / "src" / "baseline-parser.ts",
    ROOT / "scripts" / "emit" / "src" / "canonical-products.ts",
    ROOT / "scripts" / "emit" / "src" / "canonical-support.ts",
    ROOT / "scripts" / "emit" / "src" / "authored-options.ts",
    ROOT / "scripts" / "emit" / "src" / "source-test.ts",
    ROOT / "scripts" / "emit" / "src" / "artifact-state.ts",
    ROOT / "scripts" / "emit" / "src" / "oracle.ts",
    ROOT / "scripts" / "emit" / "oracle-manifest.json",
    ROOT / "scripts" / "emit" / "query-emit.py",
)
HARNESS_TRUTH_REL_PATHS = {
    path.relative_to(ROOT).as_posix() for path in HARNESS_TRUTH_SOURCES
}

REPLACE_CALL_RE = re.compile(r"(?:\.\s*)?(replace|replacen|replace_range)\s*\(")
MANUAL_DEBT_MARKER_RE = re.compile(r"OUTPUT_SURGERY_DEBT\b")
UNALLOWLISTED_FAILURE_RE = re.compile(
    r": (?P<count>\d+) unallowlisted output-surgery call\(s\)"
)
OVER_ALLOWLIST_FAILURE_RE = re.compile(
    r": (?P<count>\d+) output-surgery call\(s\), allowlist max is (?P<max_count>\d+)"
)

HARNESS_BYPASS_RULES: dict[str, tuple[tuple[str, re.Pattern[str]], ...]] = {
    "cli-transpiler.ts": (
        (
            "altered-invocation-retry",
            re.compile(r"\b(?:retryArgs|shouldRetryDeclarationFastPath)\b"),
        ),
        (
            "expected-content-control",
            re.compile(r"\b(?:expectedJsContent|expectedDtsContent|normalizeComparableOutput)\b"),
        ),
        (
            "candidate-output-search",
            re.compile(r"\b(?:jsCandidates|dtsCandidates|stemMatches|basenameMatches)\b"),
        ),
        (
            "actual-output-compensation",
            re.compile(
                r"\b(?:dedupeUseStrictPreamble|hasUseStrictPreamble|normalizeLeadingTripleSlashSpacing)\b"
            ),
        ),
        (
            "single-product-election",
            re.compile(
                r"\b(?:expectedJsFileName|expectedDtsFileName|"
                r"readCanonicalOutput|resolveCanonicalOutputPath)\b"
            ),
        ),
        (
            "arbitrary-rejection-absence-pass",
            re.compile(r"\bexpectedNoProducts\b"),
        ),
        (
            "synthesized-unchecked-invocation",
            re.compile(
                r"if\s*\(\s*!declarationRequested\s*\)\s*\{?"
                r"(?:(?!\n\s*\}).){0,300}--no(?:Check|Lib)",
                re.DOTALL,
            ),
        ),
        (
            "staged-files-used-as-root-arguments",
            re.compile(r"args\.push\(\.\.\.inputFiles\)"),
        ),
        (
            "synthesized-allow-js",
            re.compile(r"\b(?:hasJsInput|effectiveAllowJs)\b|allowJs\s*\?\?"),
        ),
    ),
    "runner.ts": (
        (
            "actual-output-compensation",
            re.compile(r"\b(?:stripSourceMapUrl|dedupeUseStrict)\b"),
        ),
        (
            "tolerant-comparison-pass",
            re.compile(
                r"\b(?:normalizeComments|normalizeWhitespace)\b|"
                r"(?:comment|whitespace)-only difference"
            ),
        ),
        (
            "canonical-result-cache",
            re.compile(r"\b(?:getCacheKey|loadCache|saveCache|buildSourceKey)\b|emit-cache\.json"),
        ),
        (
            "single-product-comparison",
            re.compile(
                r"\b(?:expectedJs|expectedDts|expectedJsFileName|expectedDtsFileName)\b|"
                r"transpileResult\.(?:js|dts)\b"
            ),
        ),
        (
            "silent-candidate-drop",
            re.compile(r"maxTests\s*\*\s*2|return\s+results\.filter\s*\("),
        ),
        (
            "legacy-baseline-byte-oracle",
            re.compile(
                r"\bnoCheckContent\b|\bexpected(?:Js|Dts)Products\b|"
                r"baseline\.(?:js|dts)\s*=|\bTSZ_EMIT_BASELINES_OVERLAY\b|"
                r"\bOVERLAY_DIR\b|baselines-ts7"
            ),
        ),
        (
            "strict-family-approximation",
            re.compile(
                r"strictNullChecks\s*=\s*[^;\n]*\bstrict\b|"
                r"\bstrict\b\s*===\s*false[^;\n]*strictNullChecks"
            ),
        ),
        (
            "baseline-derived-declaration-flag",
            re.compile(r"declaration\s*:\s*[^,\n]*dtsProductDomain|declaration\s*\?\?[^\n]*dtsProductDomain"),
        ),
        (
            "ignored-embedded-config-error",
            re.compile(r"catch\s*\{\s*/\*\s*ignore parse errors", re.IGNORECASE),
        ),
    ),
    "baseline-parser.ts": (
        (
            "non-line-ending-normalization",
            re.compile(
                r"export function normalizeEmit\([^)]*\)[^{]*\{"
                r"(?:(?!\n\}).)*?\.trim\s*\(",
                re.DOTALL,
            ),
        ),
        (
            "product-line-ending-normalization",
            re.compile(r"\bnormalizeEmit\b"),
        ),
    ),
    "canonical-products.ts": (
        (
            "product-line-ending-normalization",
            re.compile(r"\b(?:compareEmit|normalizeEmit)\b"),
        ),
        (
            "nonzero-outcome-match",
            re.compile(r"oracle\.exitCode\s*!==\s*product\.exitCode"),
        ),
    ),
    "authored-options.ts": (
        (
            "provenance-erasing-boolean-coercion",
            re.compile(r"(?:option\.)?value\.trim\(\)\.toLowerCase\(\)"),
        ),
    ),
    "source-test.ts": (
        (
            "source-byte-trim",
            re.compile(r"(?:sourceBytes|singleFileContent)[^;\n]*\.trim\s*\("),
        ),
        (
            "historical-source-fallback",
            re.compile(r"\bgit\b[^;\n]{0,160}\bshow\b|\bHEAD:"),
        ),
    ),
}

HARNESS_REQUIRED_RULES: dict[str, tuple[tuple[str, re.Pattern[str]], ...]] = {
    "cli-transpiler.ts": (
        ("missing-complete-product-collection", re.compile(r"\bcollectActualProducts\b")),
        ("missing-emit-declaration-only-forwarding", re.compile(r"--emitDeclarationOnly")),
        ("missing-authored-no-check-forwarding", re.compile(r"booleanFlag\('--noCheck', opts\.noCheck\)")),
        ("missing-authored-no-lib-forwarding", re.compile(r"booleanFlag\('--noLib', opts\.noLib\)")),
        ("missing-explicit-false-forwarding", re.compile(r"args\.push\(name, String\(value\)\)")),
        ("missing-authored-allow-js-forwarding", re.compile(
            r"booleanFlag\('--allowJs', opts\.allowJs\)"
        )),
        ("missing-staged-root-separation", re.compile(r"\brootInputFiles\b")),
        ("missing-explicit-root-vector", re.compile(r"\brootFileNames\b")),
        ("missing-lossless-product-byte-read", re.compile(r"toString\('latin1'\)")),
    ),
    "runner.ts": (
        ("missing-complete-product-comparison", re.compile(r"\bcompareCanonicalProductSets\b")),
        ("missing-compiler-outcome-comparison", re.compile(r"\bcompareCompilerOutcomes\b")),
        ("missing-pinned-oracle-resolution", re.compile(r"\bresolvePinnedOracle\b")),
        ("missing-independent-dual-invocation", re.compile(r"Promise\.all\s*\(\s*\[\s*oracleTranspiler\.transpile")),
        ("missing-oracle-result-provenance", re.compile(r"oracle:\s*oracle\.provenance")),
        ("missing-js-domain-only-projection", re.compile(r"baseline\.jsOutputs\.map\(product => product\.name\)")),
        ("missing-dts-domain-only-projection", re.compile(r"baseline\.dtsOutputs\.map\(product => product\.name\)")),
        ("missing-terminal-unsupported-row", re.compile(r"UNSUPPORTED_CANONICAL_EMIT")),
        ("missing-non-vacuity-guard", re.compile(r"No canonical emit test cases selected")),
        ("missing-authored-option-resolution", re.compile(r"\bresolveAuthoredOptions\b")),
        ("missing-authored-option-failure-boundary", re.compile(r"\bauthoredOptionFailureReasons\b")),
        ("missing-embedded-config-failure", re.compile(r"embeddedConfig\.reasons")),
        ("missing-unrepresented-declaration-domain-failure", re.compile(
            r"declaration-product-domain-without-authored-declaration"
        )),
        ("missing-mutually-exclusive-mode-guard", re.compile(
            r"config\.jsOnly\s*&&\s*config\.dtsOnly"
        )),
        ("missing-all-null-result-guard", re.compile(r"\bensureMeasuredArtifact\b")),
        ("missing-explicit-artifact-status", re.compile(r"\bartifactStatus\b")),
    ),
    "baseline-parser.ts": (
        ("missing-js-product-inventory", re.compile(r"\bjsOutputs\b")),
        ("missing-dts-product-inventory", re.compile(r"\bdtsOutputs\b")),
        ("missing-byte-exact-compare", re.compile(r"return expected === actual")),
    ),
    "canonical-products.ts": (
        ("missing-path-to-bytes-comparison", re.compile(r"\bcompareCanonicalProductSets\b")),
        ("missing-byte-exact-product-compare", re.compile(r"oracleContent !== actualContent")),
        ("missing-oracle-nonzero-rejection", re.compile(r"oracle\.exitCode !== 0")),
        ("missing-product-nonzero-rejection", re.compile(r"product\.exitCode !== 0")),
    ),
    "canonical-support.ts": (
        ("missing-source-map-quarantine", re.compile(r"source-map-products-not-compared")),
        ("missing-inventory-retention", re.compile(r"\bretainCanonicalInventory\b")),
    ),
    "authored-options.ts": (
        ("missing-option-disposition", re.compile(r"\bauthoredOptionDisposition\b")),
        ("missing-filename-variant-accounting", re.compile(r"\bextractAuthoredVariantFromFilename\b")),
        ("missing-option-precedence", re.compile(
            r"merge\(sources\.embeddedConfig[^\n]+\n\s*"
            r"merge\(sources\.directives[^\n]+\n\s*"
            r"merge\(sources\.variant"
        )),
        ("missing-invalid-option-accounting", re.compile(r"\binvalidAuthoredOptions\b")),
        ("missing-invalid-authored-option-failure", re.compile(r"invalid-authored-option:")),
        ("missing-unhandled-authored-option-failure", re.compile(r"unhandled-authored-option:")),
        ("missing-jsonc-error-accounting", re.compile(r"embedded-tsconfig-jsonc-parse-error")),
        ("missing-config-conflict-accounting", re.compile(r"conflicting-embedded-tsconfigs")),
        ("missing-config-field-accounting", re.compile(r"unhandled-embedded-tsconfig-field")),
        ("missing-provenance-strict-boolean", re.compile(r"option\.source === 'filename-variant'")),
        ("missing-embedded-lib-array-check", re.compile(
            r"option\.source === 'embedded-config'[^\n]+\n\s*return Array\.isArray"
        )),
    ),
    "source-test.ts": (
        ("missing-exact-source-byte-assembly", re.compile(r"sourceBytes\.join\(''\)")),
        ("missing-live-corpus-read", re.compile(r"fs\.promises\.readFile")),
        ("missing-harness-root-selection", re.compile(r"\bselectHarnessRootFiles\b")),
        ("missing-last-unit-root-model", re.compile(r"last-unit-no-implicit-references")),
        ("missing-config-root-quarantine", re.compile(r"embedded-tsconfig-root-selection-not-modeled")),
    ),
    "artifact-state.ts": (
        ("missing-artifact-state-domain", re.compile(
            r"'unsupported'\s*\|\s*'timeout'\s*\|\s*'crash'\s*\|\s*'incomplete'"
        )),
        ("missing-compiler-artifact-classification", re.compile(r"\bcompilerArtifactState\b")),
        ("missing-vacuous-artifact-failure", re.compile(r"INCOMPLETE_CANONICAL_EMIT")),
    ),
    "oracle.ts": (
        ("missing-version-probe", re.compile(r"verifyOracleExecutable")),
        ("missing-trusted-binary-hash", re.compile(r"trustedPlatform\.binarySha256")),
        ("missing-trusted-package-tree-hash", re.compile(r"trustedPlatform\.packageTreeSha256")),
        ("missing-trusted-package-integrity", re.compile(r"trustedPlatform\.packageIntegrity")),
    ),
    "oracle-manifest.json": (
        ("missing-local-platform-pin", re.compile(r'"darwin-arm64"')),
        ("missing-ci-platform-pin", re.compile(r'"linux-x64"')),
    ),
    "query-emit.py": (
        ("missing-terminal-artifact-statuses", re.compile(
            r"TERMINAL_STATUSES\s*=\s*\{[^}]*\"unsupported\"[^}]*\"crash\"[^}]*\"incomplete\""
        )),
        ("missing-artifact-state-fingerprint", re.compile(
            r'"artifactState":\s*result\.get\("artifactState"\)'
        )),
    ),
}


@dataclasses.dataclass(frozen=True)
class Finding:
    path: str
    line_no: int
    call: str
    text: str


@dataclasses.dataclass(frozen=True)
class AllowEntry:
    category: str
    max_count: int
    reason: str


@dataclasses.dataclass(frozen=True)
class FailureSummary:
    # Backward-compatible top-level counters:
    # - unallowlisted counts calls, matching the guardrail debt metric.
    # - over_allowlist and stale_allowlist count affected allowlist rows.
    unallowlisted: int = 0
    over_allowlist: int = 0
    stale_allowlist: int = 0
    unallowlisted_files: int = 0
    over_allowlist_files: int = 0
    over_allowlist_excess_calls: int = 0
    stale_allowlist_files: int = 0


@dataclasses.dataclass(frozen=True)
class BudgetSummary:
    allowlisted_calls: int = 0
    allowlist_cap: int = 0
    remaining_allowlist_capacity: int = 0
    allowlisted_files: int = 0
    budget_status: str = "no_allowlist"


def iter_rust_files(base: pathlib.Path = SOURCE_ROOT):
    if base.is_file():
        if base.suffix == ".rs":
            yield base
        return
    if base.is_dir():
        yield from sorted(base.rglob("*.rs"))
        return
    raise FileNotFoundError(f"emitter source path does not exist: {base}")


def is_auto_allowed_data_cleanup(path: str, line: str) -> bool:
    stripped = line.strip()

    # State mutation APIs with the same name are not output string surgery.
    if "std::mem::replace" in stripped:
        return True
    if re.search(r"\b[a-zA-Z_][a-zA-Z0-9_]*\.replace\(", stripped) and not re.search(
        r"\b(output|emitted|rewritten|type_text|constructor_type|line|assignment|remainder)\.replace",
        stripped,
    ):
        return True
    if stripped.startswith(".replace(") and not re.search(r"\.replace\([\"'&]", stripped):
        return True

    # Runtime helper source text intentionally contains JavaScript `.replace(...)`.
    if path.endswith("transforms/helpers.rs") and "return path.replace(" in stripped:
        return True

    # Escaping and literal normalization are data construction, not emitted
    # program-structure surgery.
    data_cleanup_needles = [
        ".replace('\\\\',",
        '.replace("\\\\",',
        ".replace('\"',",
        ".replace(\"\\\"\",",
        ".replace('\\'',",
        ".replace('_', \"\")",
        ".replace(\"\\r\\n\",",
        ".replace('\\r',",
        ".replace('\\n',",
        ".replace('*',",
        '.replace("*/"',
    ]
    if any(needle in stripped for needle in data_cleanup_needles):
        return True

    # Path normalization helpers often receive owned path text as data.
    if ".replace('\\\\', \"/\")" in stripped or "../node_modules/" in stripped:
        return True

    return False


def scan_harness_truth(
    paths: Optional[tuple[pathlib.Path, ...]] = None,
    *,
    enforce_required: Optional[bool] = None,
) -> list[Finding]:
    if paths is None:
        paths = HARNESS_TRUTH_SOURCES
        if enforce_required is None:
            enforce_required = True
    elif enforce_required is None:
        enforce_required = False
    findings: list[Finding] = []
    for path in paths:
        if not path.is_file():
            raise FileNotFoundError(f"emit harness truth source does not exist: {path}")
        rules = HARNESS_BYPASS_RULES.get(path.name, ())
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        for category, pattern in rules:
            for match in pattern.finditer(text):
                line_no = text.count("\n", 0, match.start()) + 1
                line = lines[line_no - 1].strip() if line_no <= len(lines) else ""
                findings.append(
                    Finding(
                        path=path.relative_to(ROOT).as_posix(),
                        line_no=line_no,
                        call=category,
                        text=line,
                    )
                )
        if enforce_required:
            for category, pattern in HARNESS_REQUIRED_RULES.get(path.name, ()):
                if pattern.search(text):
                    continue
                findings.append(
                    Finding(
                        path=path.relative_to(ROOT).as_posix(),
                        line_no=1,
                        call=category,
                        text=f"required canonical emit contract missing: {category}",
                    )
                )
    return findings


def scan(base: pathlib.Path = SOURCE_ROOT) -> list[Finding]:
    findings: list[Finding] = []
    for path in iter_rust_files(base):
        rel = path.relative_to(ROOT).as_posix()
        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if MANUAL_DEBT_MARKER_RE.search(line):
                findings.append(
                    Finding(
                        path=rel,
                        line_no=line_no,
                        call="manual",
                        text=line.strip(),
                    )
                )
            for match in REPLACE_CALL_RE.finditer(line):
                if is_auto_allowed_data_cleanup(rel, line):
                    continue
                findings.append(
                    Finding(
                        path=rel,
                        line_no=line_no,
                        call=match.group(1),
                        text=line.strip(),
                    )
                )
    if base == SOURCE_ROOT:
        findings.extend(scan_harness_truth())
    return findings


def load_allowlist(path: pathlib.Path = ALLOWLIST_PATH) -> dict[str, AllowEntry]:
    entries: dict[str, AllowEntry] = {}
    if not path.exists():
        return entries
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split("|", 3)
        if len(parts) != 4:
            raise ValueError(f"{path}:{line_no}: expected path|category|max_count|reason")
        rel_path, category, max_count, reason = [part.strip() for part in parts]
        if not rel_path or not category or not reason:
            raise ValueError(f"{path}:{line_no}: path, category, and reason are required")
        entries[rel_path] = AllowEntry(
            category=category,
            max_count=int(max_count),
            reason=reason,
        )
    return entries


def grouped_counts(findings: list[Finding]) -> Counter[str]:
    return Counter(finding.path for finding in findings)


def audit(findings: list[Finding], allowlist: dict[str, AllowEntry]) -> list[str]:
    failures: list[str] = []
    counts = grouped_counts(findings)
    for path, count in sorted(counts.items()):
        if path in HARNESS_TRUTH_REL_PATHS or pathlib.PurePath(path).name in HARNESS_BYPASS_RULES:
            failures.append(
                f"{path}: {count} forbidden canonical-emit truth bypass(es)"
            )
            continue
        entry = allowlist.get(path)
        if entry is None:
            failures.append(f"{path}: {count} unallowlisted output-surgery call(s)")
        elif count > entry.max_count:
            failures.append(
                f"{path}: {count} output-surgery call(s), allowlist max is {entry.max_count}"
            )
    for path in sorted(set(allowlist) - set(counts)):
        failures.append(f"{path}: allowlist entry is stale; no matching calls remain")
    for path in sorted(set(allowlist) & HARNESS_TRUTH_REL_PATHS):
        failures.append(f"{path}: canonical-emit truth bypasses may not be allowlisted")
    return failures


def summarize_failures(failures: list[str]) -> FailureSummary:
    summary = FailureSummary()
    for failure in failures:
        if unallowlisted_match := UNALLOWLISTED_FAILURE_RE.search(failure):
            summary = dataclasses.replace(
                summary,
                unallowlisted=summary.unallowlisted
                + int(unallowlisted_match.group("count")),
                unallowlisted_files=summary.unallowlisted_files + 1,
            )
        elif "allowlist entry is stale" in failure:
            summary = dataclasses.replace(
                summary,
                stale_allowlist=summary.stale_allowlist + 1,
                stale_allowlist_files=summary.stale_allowlist_files + 1,
            )
        elif over_allowlist_match := OVER_ALLOWLIST_FAILURE_RE.search(failure):
            count = int(over_allowlist_match.group("count"))
            max_count = int(over_allowlist_match.group("max_count"))
            summary = dataclasses.replace(
                summary,
                over_allowlist=summary.over_allowlist + 1,
                over_allowlist_files=summary.over_allowlist_files + 1,
                over_allowlist_excess_calls=summary.over_allowlist_excess_calls
                + max(0, count - max_count),
            )
    return summary


def file_status(path: str, count: int, allowlist: dict[str, AllowEntry]) -> str:
    entry = allowlist.get(path)
    if entry is None:
        return "unallowlisted"
    if count == 0:
        return "stale_allowlist"
    if count > entry.max_count:
        return "over_allowlist"
    return "allowlisted"


def build_file_summaries(
    counts: Counter[str],
    allowlist: dict[str, AllowEntry],
) -> list[dict[str, object]]:
    summaries: list[dict[str, object]] = []
    for path in sorted(set(counts) | set(allowlist)):
        entry = allowlist.get(path)
        count = counts.get(path, 0)
        summaries.append(
            {
                "path": path,
                "count": count,
                "category": entry.category if entry else "UNALLOWLISTED",
                "max_count": entry.max_count if entry else None,
                "reason": entry.reason if entry else None,
                "status": file_status(path, count, allowlist),
            }
        )
    return summaries


def build_category_summaries(file_summaries: list[dict[str, object]]) -> list[dict[str, object]]:
    categories: dict[str, dict[str, object]] = {}
    for summary in file_summaries:
        category = str(summary["category"])
        entry = categories.setdefault(
            category,
            {
                "category": category,
                "count": 0,
                "max_count": 0,
                "files": 0,
                "statuses": Counter(),
            },
        )
        entry["count"] = int(entry["count"]) + int(summary["count"])
        max_count = summary["max_count"]
        if max_count is None:
            entry["max_count"] = None
        elif entry["max_count"] is not None:
            entry["max_count"] = int(entry["max_count"]) + int(max_count)
        entry["files"] = int(entry["files"]) + 1
        entry["statuses"][str(summary["status"])] += 1

    result: list[dict[str, object]] = []
    for entry in sorted(categories.values(), key=lambda item: str(item["category"])):
        count = int(entry["count"])
        max_count = entry["max_count"]
        remaining_capacity = None
        budget_status = "unallowlisted"
        if max_count is not None:
            remaining_capacity = max(0, int(max_count) - count)
            budget_status = classify_budget_status(count, int(max_count))
        result.append(
            {
                "category": entry["category"],
                "count": count,
                "max_count": max_count,
                "remaining_capacity": remaining_capacity,
                "budget_status": budget_status,
                "files": entry["files"],
                "statuses": dict(sorted(entry["statuses"].items())),
            }
        )
    return result


def summarize_budget(file_summaries: list[dict[str, object]]) -> BudgetSummary:
    allowlisted_calls = 0
    allowlist_cap = 0
    allowlisted_files = 0
    for summary in file_summaries:
        max_count = summary["max_count"]
        if max_count is None:
            continue
        allowlisted_calls += int(summary["count"])
        allowlist_cap += int(max_count)
        allowlisted_files += 1
    return BudgetSummary(
        allowlisted_calls=allowlisted_calls,
        allowlist_cap=allowlist_cap,
        remaining_allowlist_capacity=max(0, allowlist_cap - allowlisted_calls),
        allowlisted_files=allowlisted_files,
        budget_status=classify_budget_status(allowlisted_calls, allowlist_cap),
    )


def exhausted_category_names(file_summaries: list[dict[str, object]]) -> list[str]:
    return [
        str(summary["category"])
        for summary in build_category_summaries(file_summaries)
        if summary["max_count"] is not None and summary["budget_status"] == "exhausted"
    ]


def classify_budget_status(allowlisted_calls: int, allowlist_cap: int) -> str:
    if allowlist_cap == 0:
        return "no_allowlist"
    if allowlisted_calls > allowlist_cap:
        return "over_cap"
    if allowlisted_calls == allowlist_cap:
        return "exhausted"
    return "available"


def classify_allowlist_pressure(
    budget: BudgetSummary,
    exhausted_categories: list[str],
) -> str:
    if budget.budget_status == "over_cap":
        return "over_cap"
    if budget.budget_status == "no_allowlist":
        return "no_allowlist"
    if exhausted_categories and budget.remaining_allowlist_capacity == 0:
        return "blocked"
    if exhausted_categories:
        return "category_blocked"
    return "available"


def _run_git(root: pathlib.Path, args: list[str]) -> Optional[str]:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), *args],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def build_git_context(root: pathlib.Path = ROOT, run_git=_run_git) -> dict[str, object]:
    status = run_git(root, ["status", "--porcelain"])
    branch = run_git(root, ["branch", "--show-current"])
    return {
        "repo_root": root.as_posix(),
        "head": run_git(root, ["rev-parse", "HEAD"]),
        "branch": branch or None,
        "upstream": run_git(
            root,
            ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        ),
        "dirty": None if status is None else bool(status),
        "dirty_path_count": None if status is None else len(status.splitlines()),
    }


def format_budget_metrics(budget: BudgetSummary) -> str:
    return (
        f"allowlisted_calls={budget.allowlisted_calls}, "
        f"allowlist_cap={budget.allowlist_cap}, "
        f"remaining_allowlist_capacity={budget.remaining_allowlist_capacity}, "
        f"allowlist_budget_status={budget.budget_status}"
    )


def format_category_budget_metrics(file_summaries: list[dict[str, object]]) -> str:
    parts: list[str] = []
    for summary in build_category_summaries(file_summaries):
        category = str(summary["category"])
        count = int(summary["count"])
        max_count = summary["max_count"]
        if max_count is None:
            parts.append(f"{category}=unallowlisted:{count}")
        else:
            budget_status = str(summary["budget_status"])
            parts.append(f"{category}={count}/{int(max_count)}:{budget_status}")
    return "category_budgets=" + ",".join(parts)


def format_exhausted_category_metrics(file_summaries: list[dict[str, object]]) -> str:
    names = exhausted_category_names(file_summaries)
    formatted_names = ";".join(names) if names else "none"
    return f"exhausted_category_count={len(names)}, exhausted_categories={formatted_names}"


def format_warning_metrics(file_summaries: list[dict[str, object]]) -> str:
    warning_count = len(exhausted_category_names(file_summaries))
    warning_status = "warn" if warning_count else "clear"
    return f"warning_count={warning_count}, warning_status={warning_status}"


def warning_failures(file_summaries: list[dict[str, object]]) -> list[str]:
    warnings: list[str] = []
    budget = summarize_budget(file_summaries)
    exhausted_categories = exhausted_category_names(file_summaries)
    pressure_status = classify_allowlist_pressure(budget, exhausted_categories)
    if pressure_status not in {"available", "no_allowlist"}:
        warnings.append(
            "allowlist pressure is "
            f"{pressure_status}; exhausted categories: "
            f"{';'.join(exhausted_categories) if exhausted_categories else 'none'}"
        )
    return warnings


def format_allowlist_pressure_metrics(file_summaries: list[dict[str, object]]) -> str:
    budget = summarize_budget(file_summaries)
    exhausted_categories = exhausted_category_names(file_summaries)
    return "allowlist_pressure_status=" + classify_allowlist_pressure(
        budget,
        exhausted_categories,
    )


def build_json_report(
    findings: list[Finding],
    allowlist: dict[str, AllowEntry],
    failures: list[str],
    git_context: Optional[dict[str, object]] = None,
    warning_messages: Optional[list[str]] = None,
    fail_on_warnings: bool = False,
) -> dict[str, object]:
    counts = grouped_counts(findings)
    summary = summarize_failures(failures)
    file_summaries = build_file_summaries(counts, allowlist)
    budget = summarize_budget(file_summaries)
    category_summaries = build_category_summaries(file_summaries)
    exhausted_categories = [
        str(summary["category"])
        for summary in category_summaries
        if summary["max_count"] is not None and summary["budget_status"] == "exhausted"
    ]
    warning_count = len(exhausted_categories)
    allowlist_pressure_status = classify_allowlist_pressure(budget, exhausted_categories)
    warnings = (
        warning_messages if warning_messages is not None else warning_failures(file_summaries)
    )
    strict_warning_failed = fail_on_warnings and bool(warnings)
    if failures:
        status = "failed"
    elif strict_warning_failed:
        status = "warning_failed"
    else:
        status = "passed"
    if strict_warning_failed:
        strict_warning_status = "failed"
    elif warnings:
        strict_warning_status = "warn"
    else:
        strict_warning_status = "clear"
    return {
        "ok": not failures and not strict_warning_failed,
        "status": status,
        "output_surgery_status": "failed" if failures else "passed",
        "warning_count": warning_count,
        "warning_status": "warn" if warning_count else "clear",
        "fail_on_warnings": fail_on_warnings,
        "strict_warning_status": strict_warning_status,
        "warning_failures": warnings,
        "warning_failure_count": len(warnings),
        "git_context": git_context if git_context is not None else build_git_context(),
        "total_findings": len(findings),
        "files_with_findings": len(counts),
        "allowlisted_calls": budget.allowlisted_calls,
        "allowlist_cap": budget.allowlist_cap,
        "remaining_allowlist_capacity": budget.remaining_allowlist_capacity,
        "allowlist_budget_status": budget.budget_status,
        "allowlist_pressure_status": allowlist_pressure_status,
        "exhausted_category_count": warning_count,
        "exhausted_categories": exhausted_categories,
        "unallowlisted_calls": summary.unallowlisted,
        "over_allowlist_files": summary.over_allowlist_files,
        "over_allowlist_excess_calls": summary.over_allowlist_excess_calls,
        "stale_allowlist_files": summary.stale_allowlist_files,
        "failure_summary": dataclasses.asdict(summary),
        "budget_summary": dataclasses.asdict(budget),
        "failures": failures,
        "categories": category_summaries,
        "files": file_summaries,
        "findings": [
            {
                "path": finding.path,
                "line_no": finding.line_no,
                "call": finding.call,
                "category": allowlist[finding.path].category
                if finding.path in allowlist
                else "UNALLOWLISTED",
                "text": finding.text,
            }
            for finding in findings
        ],
    }


def write_json_report(path: pathlib.Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = path.with_name(f".{path.name}.tmp")
    temp_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temp_path.replace(path)


def print_report(findings: list[Finding], allowlist: dict[str, AllowEntry]) -> None:
    by_path: dict[str, list[Finding]] = defaultdict(list)
    for finding in findings:
        by_path[finding.path].append(finding)

    for path in sorted(by_path):
        entry = allowlist.get(path)
        category = entry.category if entry else "UNALLOWLISTED"
        print(f"{path} [{category}] ({len(by_path[path])})")
        for finding in by_path[path]:
            print(f"  {finding.line_no}: {finding.text}")


def format_pass_summary(
    findings: list[Finding],
    failures: list[str],
    allowlist: dict[str, AllowEntry],
) -> str:
    summary = summarize_failures(failures)
    file_summaries = build_file_summaries(grouped_counts(findings), allowlist)
    budget = summarize_budget(file_summaries)
    return (
        "Output-surgery audit passed: "
        f"total_findings={len(findings)}, "
        f"files_with_findings={len(grouped_counts(findings))}, "
        f"{format_budget_metrics(budget)}, "
        f"{format_category_budget_metrics(file_summaries)}, "
        f"{format_exhausted_category_metrics(file_summaries)}, "
        f"{format_allowlist_pressure_metrics(file_summaries)}, "
        f"{format_warning_metrics(file_summaries)}, "
        f"unallowlisted_calls={summary.unallowlisted}, "
        f"over_allowlist_files={summary.over_allowlist_files}, "
        f"over_allowlist_excess_calls={summary.over_allowlist_excess_calls}, "
        f"stale_allowlist_files={summary.stale_allowlist_files}."
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print all tracked findings")
    parser.add_argument(
        "--json",
        action="store_true",
        help="print a machine-readable report to stdout",
    )
    parser.add_argument(
        "--json-report",
        type=pathlib.Path,
        help="write a machine-readable report before exiting",
    )
    parser.add_argument(
        "--fail-on-warnings",
        action="store_true",
        help=(
            "return a non-zero status when the audit passes but warning metrics "
            "show exhausted output-surgery budget pressure"
        ),
    )
    args = parser.parse_args(argv)

    findings = scan()
    allowlist = load_allowlist()
    failures = audit(findings, allowlist)
    file_summaries = build_file_summaries(grouped_counts(findings), allowlist)
    warnings = warning_failures(file_summaries)
    json_report = build_json_report(
        findings,
        allowlist,
        failures,
        warning_messages=warnings,
        fail_on_warnings=args.fail_on_warnings,
    )

    if args.json_report is not None:
        write_json_report(args.json_report, json_report)

    if args.json:
        print(json.dumps(json_report, indent=2, sort_keys=True))

    if not args.json and (args.list or failures):
        print_report(findings, allowlist)

    if failures:
        summary = summarize_failures(failures)
        file_summaries = build_file_summaries(grouped_counts(findings), allowlist)
        budget = summarize_budget(file_summaries)
        print(
            "\nOutput-surgery audit summary: "
            f"{format_budget_metrics(budget)}, "
            f"{format_category_budget_metrics(file_summaries)}, "
            f"{format_exhausted_category_metrics(file_summaries)}, "
            f"{format_warning_metrics(file_summaries)}, "
            f"unallowlisted_calls={summary.unallowlisted}, "
            f"unallowlisted_files={summary.unallowlisted_files}, "
            f"over_allowlist_files={summary.over_allowlist_files}, "
            f"over_allowlist_excess_calls={summary.over_allowlist_excess_calls}, "
            f"stale_allowlist_files={summary.stale_allowlist_files}",
            file=sys.stderr,
        )
        print("\nOutput-surgery audit failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    if not args.json:
        pass_summary = format_pass_summary(findings, failures, allowlist)
        print(pass_summary)
    if args.fail_on_warnings and warnings:
        print("\nOutput-surgery audit warnings failed by --fail-on-warnings:", file=sys.stderr)
        for warning in warnings:
            print(f"  - {warning}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
