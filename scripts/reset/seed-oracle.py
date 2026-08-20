#!/usr/bin/env python3
"""Compare the R0 seed matrix with the pinned TypeScript 7 oracle."""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


DIAGNOSTIC_RE = re.compile(
    r"^(?P<file>.+)\((?P<line>\d+),(?P<column>\d+)\): "
    r"(?P<category>error|warning|suggestion|message) "
    r"TS(?P<code>\d+): (?P<message>.*)$"
)


@dataclasses.dataclass(frozen=True)
class Diagnostic:
    file: str
    category: str
    code: int
    line: int
    column: int
    message: str


@dataclasses.dataclass
class ProcessResult:
    command: list[str]
    exit_status: int
    stdout: str
    stderr: str
    diagnostics: list[Diagnostic]
    unexpected_stdout: list[str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Compare R0 seed diagnostics and JavaScript with the pinned "
            "TypeScript 7.0.2 oracle."
        )
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--tsz",
        type=Path,
        help="replacement tsz binary (default: TSZ_BIN or .target/debug/tsz)",
    )
    parser.add_argument(
        "--case",
        action="append",
        dest="cases",
        metavar="NAME",
        help="run only a named matrix case; repeat to select more than one",
    )
    return parser.parse_args()


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"expected a JSON object in {path}")
    return value


def pinned_version(repo_root: Path) -> str:
    versions = load_json(repo_root / "scripts/conformance/typescript-versions.json")
    current = versions.get("current")
    mappings = versions.get("mappings", {})
    current_entry = mappings.get(current, {}) if isinstance(mappings, dict) else {}
    version = current_entry.get("npm") if isinstance(current_entry, dict) else None
    if not version:
        default = versions.get("default", {})
        version = default.get("npm") if isinstance(default, dict) else None
    if not isinstance(version, str) or not version:
        raise ValueError("could not resolve the pinned TypeScript npm version")
    return version


def parse_diagnostics(stdout: str) -> tuple[list[Diagnostic], list[str]]:
    diagnostics: list[Diagnostic] = []
    unexpected: list[str] = []
    for line in stdout.splitlines():
        if not line:
            continue
        match = DIAGNOSTIC_RE.fullmatch(line)
        if match is None:
            unexpected.append(line)
            continue
        diagnostics.append(
            Diagnostic(
                file=match.group("file"),
                category=match.group("category"),
                code=int(match.group("code")),
                line=int(match.group("line")),
                column=int(match.group("column")),
                message=match.group("message"),
            )
        )
    return diagnostics, unexpected


def run_process(command: list[str], cwd: Path) -> ProcessResult:
    completed = subprocess.run(
        command,
        cwd=cwd,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=300,
        check=False,
    )
    diagnostics, unexpected = parse_diagnostics(completed.stdout)
    return ProcessResult(
        command=command,
        exit_status=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
        diagnostics=diagnostics,
        unexpected_stdout=unexpected,
    )


def format_diagnostics(diagnostics: list[Diagnostic]) -> str:
    return json.dumps(
        [dataclasses.asdict(diagnostic) for diagnostic in diagnostics],
        indent=2,
        ensure_ascii=False,
        sort_keys=True,
    )


def report_process_difference(
    name: str, oracle: ProcessResult, candidate: ProcessResult
) -> list[str]:
    failures: list[str] = []
    if oracle.unexpected_stdout:
        failures.append(
            f"{name}: oracle produced unparsed stdout: {oracle.unexpected_stdout!r}"
        )
    if candidate.unexpected_stdout:
        failures.append(
            f"{name}: tsz produced unparsed stdout: {candidate.unexpected_stdout!r}"
        )
    if candidate.stderr:
        failures.append(f"{name}: tsz produced stderr:\n{candidate.stderr.rstrip()}")
    if oracle.exit_status != candidate.exit_status:
        failures.append(
            f"{name}: exit status differs: oracle={oracle.exit_status}, "
            f"tsz={candidate.exit_status}"
        )
    if oracle.diagnostics != candidate.diagnostics:
        failures.append(
            f"{name}: diagnostics differ\n"
            f"oracle:\n{format_diagnostics(oracle.diagnostics)}\n"
            f"tsz:\n{format_diagnostics(candidate.diagnostics)}"
        )
    return failures


def compare_outputs(
    name: str,
    output_names: list[str],
    oracle_dir: Path,
    candidate_dir: Path,
) -> list[str]:
    failures: list[str] = []
    expected_paths = sorted(output_names)
    oracle_paths = sorted(
        path.relative_to(oracle_dir).as_posix() for path in oracle_dir.rglob("*.js")
    )
    candidate_paths = sorted(
        path.relative_to(candidate_dir).as_posix()
        for path in candidate_dir.rglob("*.js")
    )
    if oracle_paths != expected_paths:
        failures.append(
            f"{name}: oracle JavaScript output set differs: "
            f"matrix={expected_paths!r}, oracle={oracle_paths!r}"
        )
    if candidate_paths != oracle_paths:
        failures.append(
            f"{name}: JavaScript output set differs: "
            f"oracle={oracle_paths!r}, tsz={candidate_paths!r}"
        )
    for output_name in output_names:
        oracle_path = oracle_dir / output_name
        candidate_path = candidate_dir / output_name
        if not oracle_path.is_file():
            failures.append(f"{name}: oracle did not emit {output_name}")
            continue
        if not candidate_path.is_file():
            failures.append(f"{name}: tsz did not emit {output_name}")
            continue
        oracle_bytes = oracle_path.read_bytes()
        candidate_bytes = candidate_path.read_bytes()
        if oracle_bytes != candidate_bytes:
            failures.append(
                f"{name}: {output_name} differs byte-for-byte\n"
                f"oracle ({len(oracle_bytes)} bytes):\n"
                f"{oracle_bytes.decode('utf-8', errors='replace')}\n"
                f"tsz ({len(candidate_bytes)} bytes):\n"
                f"{candidate_bytes.decode('utf-8', errors='replace')}"
            )
    return failures


def run_case(
    case: dict[str, Any],
    fixtures: Path,
    oracle_script: Path,
    tsz_binary: Path,
    common_flags: list[str],
    scratch_root: Path,
) -> list[str]:
    name = str(case["name"])
    mode = str(case["mode"])
    source = (fixtures / str(case["source"])).resolve()
    fixtures_root = fixtures.resolve()
    if fixtures_root not in source.parents or not source.is_file():
        return [f"{name}: invalid or missing source fixture: {source}"]

    case_root = scratch_root / name
    oracle_dir = case_root / "oracle"
    candidate_dir = case_root / "tsz"
    oracle_dir.mkdir(parents=True)
    candidate_dir.mkdir(parents=True)
    shutil.copy2(source, oracle_dir / "case.ts")
    shutil.copy2(source, candidate_dir / "case.ts")

    if mode == "diagnostics":
        oracle_args = [str(oracle_script), "case.ts", *common_flags]
        candidate_args = [str(tsz_binary), "case.ts", "--noEmit", *common_flags]
    elif mode == "emit":
        # oracle.sh deliberately supplies --noEmit. TypeScript's boolean
        # option parser accepts the later explicit false, retaining the pinned
        # install/threading machinery while enabling this one emit comparison.
        oracle_args = [
            str(oracle_script),
            "case.ts",
            "--noEmit",
            "false",
            *common_flags,
        ]
        candidate_args = [str(tsz_binary), "case.ts", *common_flags]
    else:
        return [f"{name}: unsupported matrix mode {mode!r}"]

    oracle = run_process(oracle_args, oracle_dir)
    candidate = run_process(candidate_args, candidate_dir)
    failures = report_process_difference(name, oracle, candidate)
    if mode == "emit":
        outputs = case.get("outputs")
        if not isinstance(outputs, list) or not all(
            isinstance(output, str) for output in outputs
        ):
            failures.append(f"{name}: emit case must declare string outputs")
        else:
            failures.extend(
                compare_outputs(name, outputs, oracle_dir, candidate_dir)
            )
    return failures


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    fixtures = repo_root / "tests/rewrite-seed"
    matrix = load_json(fixtures / "matrix.json")
    expected_version = matrix.get("pinned_typescript")
    actual_version = pinned_version(repo_root)
    if actual_version != expected_version:
        print(
            f"FAIL pinned oracle mismatch: matrix={expected_version!r}, "
            f"repository={actual_version!r}",
            file=sys.stderr,
        )
        return 1

    default_binary = os.environ.get("TSZ_BIN") or str(
        repo_root / ".target/debug/tsz"
    )
    tsz_binary = (args.tsz or Path(default_binary)).expanduser().resolve()
    if not tsz_binary.is_file() or not os.access(tsz_binary, os.X_OK):
        print(
            f"FAIL tsz binary is not executable: {tsz_binary}\n"
            "Build it with: cargo build -p tsz-cli --bin tsz",
            file=sys.stderr,
        )
        return 2

    oracle_script = repo_root / "scripts/conformance/oracle.sh"
    if not oracle_script.is_file() or not os.access(oracle_script, os.X_OK):
        print(f"FAIL oracle wrapper is not executable: {oracle_script}", file=sys.stderr)
        return 2

    raw_flags = matrix.get("common_flags")
    raw_cases = matrix.get("cases")
    if not isinstance(raw_flags, list) or not all(
        isinstance(flag, str) for flag in raw_flags
    ):
        print("FAIL matrix common_flags must be strings", file=sys.stderr)
        return 2
    if not isinstance(raw_cases, list) or not all(
        isinstance(case, dict) for case in raw_cases
    ):
        print("FAIL matrix cases must be objects", file=sys.stderr)
        return 2

    selected = set(args.cases or [])
    known = {str(case.get("name")) for case in raw_cases}
    unknown = selected - known
    if unknown:
        print(f"FAIL unknown case(s): {', '.join(sorted(unknown))}", file=sys.stderr)
        return 2
    cases = [case for case in raw_cases if not selected or case.get("name") in selected]

    print(f"R0 seed oracle: TypeScript {actual_version}")
    print(f"candidate: {tsz_binary}")
    print(
        "diagnostic span length: CLI oracle does not expose it; "
        f"Rust test: {matrix.get('rust_span_length_test')}"
    )

    all_failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="tsz-rewrite-seed-") as scratch:
        scratch_root = Path(scratch)
        for case in cases:
            name = str(case["name"])
            failures = run_case(
                case,
                fixtures,
                oracle_script,
                tsz_binary,
                raw_flags,
                scratch_root,
            )
            if failures:
                print(f"FAIL {name}")
                all_failures.extend(failures)
            else:
                print(f"PASS {name}")

    if all_failures:
        print("\nSeed oracle differences:", file=sys.stderr)
        for failure in all_failures:
            print(f"\n{failure}", file=sys.stderr)
        print(
            f"\nFAIL {len(all_failures)} difference(s) across {len(cases)} case(s)",
            file=sys.stderr,
        )
        return 1

    print(f"PASS {len(cases)} seed case(s): diagnostics, exits, and emit agree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
