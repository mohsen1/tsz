import importlib.util
import pathlib
import re
import shlex
import subprocess
import sys
from typing import Optional

from arch_guard_rust import relative_path
from arch_guard_shared import (
    BENCHMARK_ONLY_PROJECT_ROWS,
    COMPILE_GUARD_ONLY_PROJECT_ROWS,
    DEBUG_PRINT_REPORT_PATH,
    GENERATED_PROJECT_ROWS_WITHOUT_PINNED_SOURCE,
    PROJECT_CONFIG_WRITERS,
    ROOT,
)

_DEBUG_PRINT_REPORT_MODULE = None


def _load_debug_print_report_module():
    """Load the debug-print scanner that owns comment/string parsing."""
    global _DEBUG_PRINT_REPORT_MODULE
    if _DEBUG_PRINT_REPORT_MODULE is not None:
        return _DEBUG_PRINT_REPORT_MODULE
    spec = importlib.util.spec_from_file_location(
        "debug_print_report", DEBUG_PRINT_REPORT_PATH
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {DEBUG_PRINT_REPORT_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    _DEBUG_PRINT_REPORT_MODULE = module
    return module


def scan_debug_print_macros(root: pathlib.Path, scan_dirs: tuple[str, ...]) -> list[str]:
    """Report non-test compiler-internal `println!`, `eprintln!`, and `dbg!`."""
    report = _load_debug_print_report_module()
    hits = report.scan(root, scan_dirs)
    return [f"{hit.path}:{hit.line} {hit.macro}: {hit.text}" for hit in hits]


def extract_js_array_strings(text: str, const_name: str) -> Optional[list[str]]:
    """Extract string literals from a simple JS `NAME = [...]` array."""
    match = re.search(
        rf"\b(?:export\s+)?const\s+{re.escape(const_name)}\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None
    return re.findall(r'"([^"]+)"', match.group("body"))


def extract_project_dashboard_row_names(text: str) -> Optional[list[str]]:
    """Extract `name` fields from `COMPATIBILITY_CORPUS_ROWS` objects."""
    match = re.search(
        r"\b(?:export\s+)?const\s+COMPATIBILITY_CORPUS_ROWS\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None
    return re.findall(r'\bname:\s*"([^"]+)"', match.group("body"))


def extract_project_row_definitions(text: str) -> Optional[list[dict[str, Optional[str]]]]:
    """Extract project row metadata from `PROJECT_ROW_DEFINITIONS`.

    The architecture guard intentionally stays lightweight and avoids executing
    project scripts. This parser only reads the scalar fields needed by the
    Track 1 drift checks.
    """
    match = re.search(
        r"\b(?:export\s+)?const\s+PROJECT_ROW_DEFINITIONS\s*=\s*\[(?P<body>.*?)\]\s*;",
        text,
        re.DOTALL,
    )
    if match is None:
        return None

    rows: list[dict[str, Optional[str]]] = []
    for object_match in re.finditer(r"\{(?P<object>.*?)\}", match.group("body"), re.DOTALL):
        body = object_match.group("object")
        name_match = re.search(r'\bname:\s*"([^"]+)"', body)
        if name_match is None:
            continue

        row: dict[str, Optional[str]] = {"name": name_match.group(1)}
        for field in ("benchmark_set", "guard_set"):
            field_match = re.search(rf'\b{field}:\s*(?:"([^"]+)"|null)', body)
            if field_match is None:
                row[field] = None
            else:
                row[field] = field_match.group(1)
        rows.append(row)

    return rows


def project_rows_by_field(
    definitions: Optional[list[dict[str, Optional[str]]]],
    field: str,
    value: str,
) -> list[str]:
    if definitions is None:
        return []
    return [row["name"] for row in definitions if row.get(field) == value and row.get("name")]


def project_row_names(definitions: Optional[list[dict[str, Optional[str]]]]) -> list[str]:
    if definitions is None:
        return []
    return [row["name"] for row in definitions if row.get("name")]


def project_required_rows(
    text: str,
    definitions: Optional[list[dict[str, Optional[str]]]],
) -> Optional[list[str]]:
    rows = extract_js_array_strings(text, "REQUIRED_PROJECT_ROWS")
    if rows is not None:
        return rows
    return project_rows_by_field(definitions, "benchmark_set", "required") or None


def project_compile_canary_rows(
    text: str,
    definitions: Optional[list[dict[str, Optional[str]]]],
) -> Optional[list[str]]:
    rows = extract_js_array_strings(text, "COMPILE_CANARY_PROJECT_ROWS")
    if rows is not None:
        return rows
    return project_rows_by_field(definitions, "guard_set", "canary") or None


def scan_project_dashboard_rows(path: pathlib.Path) -> list[str]:
    """Ensure every expected project benchmark row is present in the dashboard.

    `scripts/bench/project-rows.mjs` owns the shared row inventories:

    - `REQUIRED_PROJECT_ROWS` / `COMPILE_CANARY_PROJECT_ROWS` define the
      project rows that must exist as benchmark/CI compatibility records.
    - `COMPATIBILITY_CORPUS_ROWS` defines the rows rendered by the public
      project compatibility dashboard.

    Track 1 requires public rows for every required project. This guard keeps
    the render inventory in lockstep with the expected benchmark/canary row
    inventories without hard-coding the project names here.
    """
    if not path.exists():
        return [f"{relative_path(path)}:0 benchmark data file is missing"]

    text = path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(text)
    expected = project_required_rows(text, definitions)
    canary = project_compile_canary_rows(text, definitions)
    dashboard = extract_project_dashboard_row_names(text)
    if dashboard is None:
        dashboard = project_row_names(definitions) or None
    rel = relative_path(path)
    hits: list[str] = []

    if expected is None:
        hits.append(f"{rel}:0 missing REQUIRED_PROJECT_ROWS array")
        expected = []
    if canary is None:
        hits.append(f"{rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []
    if dashboard is None:
        hits.append(f"{rel}:0 missing COMPATIBILITY_CORPUS_ROWS array")
        dashboard = []

    required = sorted(set(expected) | set(canary))
    dashboard_set = set(dashboard)
    required_set = set(required)

    for name in required:
        if name not in dashboard_set:
            hits.append(f"{rel}:0 missing compatibility dashboard row for {name}")

    for name in sorted(dashboard_set - required_set):
        hits.append(f"{rel}:0 stale compatibility dashboard row for {name}")

    duplicates = sorted({name for name in dashboard if dashboard.count(name) > 1})
    for name in duplicates:
        hits.append(f"{rel}:0 duplicate compatibility dashboard row for {name}")

    return hits


def extract_project_fixture_source_case_names(text: str) -> Optional[list[str]]:
    """Extract row names handled by `tsz_project_fixture_sources`."""
    match = re.search(
        r"\btsz_project_fixture_sources\s*\(\)\s*\{(?P<body>.*?)^\}",
        text,
        re.DOTALL | re.MULTILINE,
    )
    if match is None:
        return None

    names: list[str] = []
    for line in match.group("body").splitlines():
        case_match = re.match(r"^\s*([A-Za-z0-9_.-]+(?:\|[A-Za-z0-9_.-]+)*)\)\s*$", line)
        if case_match is None:
            continue
        names.extend(case_match.group(1).split("|"))
    return names


def emitted_project_fixture_sources(
    fixture_path: pathlib.Path,
    row_name: str,
) -> tuple[list[tuple[str, str, str]], Optional[str]]:
    """Run `tsz_project_fixture_sources` and validate emitted metadata lines."""
    command = (
        "set -euo pipefail; "
        f"source {shlex.quote(str(fixture_path))}; "
        f"tsz_project_fixture_sources {shlex.quote(row_name)}"
    )
    try:
        result = subprocess.run(
            ["bash", "-c", command],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return [], f"could not run fixture source metadata for {row_name}: {exc}"

    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        suffix = f": {detail}" if detail else ""
        return [], f"could not run fixture source metadata for {row_name}{suffix}"

    sources: list[tuple[str, str, str]] = []
    for line_number, raw_line in enumerate(result.stdout.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        fields = [field.strip() for field in line.split("|")]
        if len(fields) != 3 or any(field == "" for field in fields):
            return (
                [],
                f"malformed fixture source metadata for {row_name} line {line_number}: {line}",
            )
        sources.append((fields[0], fields[1], fields[2]))

    if not sources:
        return [], f"empty fixture source metadata for {row_name}"

    return sources, None


def scan_project_fixture_sources(
    row_path: pathlib.Path,
    fixture_path: pathlib.Path,
) -> list[str]:
    """Ensure pinned project rows have fixture source/ref metadata.

    `scripts/bench/project-rows.mjs` owns the public project row inventory.
    `scripts/bench/project-fixtures.sh` owns the pinned external fixture refs
    and exposes `tsz_project_fixture_sources` so benchmark/CI compatibility
    rows carry reproducibility metadata.
    """
    hits: list[str] = []
    row_rel = relative_path(row_path)
    fixture_rel = relative_path(fixture_path)

    if not row_path.exists():
        return [f"{row_rel}:0 project row manifest is missing"]
    if not fixture_path.exists():
        return [f"{fixture_rel}:0 project fixture metadata file is missing"]

    row_text = row_path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(row_text)
    required = project_required_rows(row_text, definitions)
    canary = project_compile_canary_rows(row_text, definitions)
    if required is None:
        hits.append(f"{row_rel}:0 missing REQUIRED_PROJECT_ROWS array")
        required = []
    if canary is None:
        hits.append(f"{row_rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []

    fixture_text = fixture_path.read_text(encoding="utf-8", errors="ignore")
    source_cases = extract_project_fixture_source_case_names(fixture_text)
    if source_cases is None:
        hits.append(f"{fixture_rel}:0 missing tsz_project_fixture_sources function")
        source_cases = []

    registered_rows = set(required) | set(canary)
    expected_rows = sorted(registered_rows - GENERATED_PROJECT_ROWS_WITHOUT_PINNED_SOURCE)
    expected_set = set(expected_rows)
    source_set = set(source_cases)

    for name in expected_rows:
        if name not in source_set:
            hits.append(f"{fixture_rel}:0 missing fixture source metadata for {name}")
            continue
        _, error = emitted_project_fixture_sources(fixture_path, name)
        if error is not None:
            hits.append(f"{fixture_rel}:0 {error}")

    for name in sorted(source_set - registered_rows):
        hits.append(f"{fixture_rel}:0 stale fixture source metadata for {name}")

    duplicates = sorted({name for name in source_cases if source_cases.count(name) > 1})
    for name in duplicates:
        hits.append(f"{fixture_rel}:0 duplicate fixture source metadata for {name}")

    return hits


def extract_project_compile_guard_rows(text: str) -> list[str]:
    """Extract row names routed through `should_check_project`."""
    return [
        name
        for name in re.findall(r'\bshould_check_project\s+"([^"]+)"', text)
        if not name.startswith("$")
    ]


def extract_project_benchmark_rows(text: str) -> list[str]:
    """Extract project row names registered in the benchmark runner."""
    return re.findall(r'\brun_isolated\s+"([^"]+)"\s+run_[A-Za-z0-9_]+_benchmarks', text)


def scan_project_inclusion_policy(
    row_path: pathlib.Path,
    compile_guard_path: pathlib.Path,
    bench_path: pathlib.Path,
) -> list[str]:
    """Ensure project row inventories match the shell inclusion policies."""
    hits: list[str] = []
    row_rel = relative_path(row_path)
    compile_rel = relative_path(compile_guard_path)
    bench_rel = relative_path(bench_path)

    if not row_path.exists():
        return [f"{row_rel}:0 project row manifest is missing"]
    if not compile_guard_path.exists():
        return [f"{compile_rel}:0 project compile guard is missing"]
    if not bench_path.exists():
        return [f"{bench_rel}:0 benchmark runner is missing"]

    row_text = row_path.read_text(encoding="utf-8", errors="ignore")
    definitions = extract_project_row_definitions(row_text)
    required = project_required_rows(row_text, definitions)
    canary = project_compile_canary_rows(row_text, definitions)
    if required is None:
        hits.append(f"{row_rel}:0 missing REQUIRED_PROJECT_ROWS array")
        required = []
    if canary is None:
        hits.append(f"{row_rel}:0 missing COMPILE_CANARY_PROJECT_ROWS array")
        canary = []

    manifest_rows = sorted(set(required) | set(canary))

    compile_text = compile_guard_path.read_text(encoding="utf-8", errors="ignore")
    compile_rows = extract_project_compile_guard_rows(compile_text)
    if 'for name in "${TSZ_COMPILE_GUARD_REQUIRED_ROWS[@]}"' in compile_text:
        compile_rows.extend(project_rows_by_field(definitions, "guard_set", "required"))
    if 'for name in "${TSZ_COMPILE_GUARD_CANARY_ROWS[@]}"' in compile_text:
        compile_rows.extend(project_rows_by_field(definitions, "guard_set", "canary"))
    compile_set = set(compile_rows)
    manifest_set = set(manifest_rows)
    expected_compile_rows = sorted(set(manifest_rows) - BENCHMARK_ONLY_PROJECT_ROWS)
    expected_compile_set = set(expected_compile_rows)
    for name in expected_compile_rows:
        if name not in compile_set:
            hits.append(f"{compile_rel}:0 missing project compile guard inclusion for {name}")
    for name in sorted(compile_set - expected_compile_set):
        hits.append(f"{compile_rel}:0 stale project compile guard inclusion for {name}")

    bench_text = bench_path.read_text(encoding="utf-8", errors="ignore")
    bench_rows = extract_project_benchmark_rows(bench_text)
    bench_set = set(bench_rows)
    expected_bench_rows = sorted(manifest_set - COMPILE_GUARD_ONLY_PROJECT_ROWS)
    expected_bench_set = set(expected_bench_rows)
    for name in expected_bench_rows:
        if name not in bench_set:
            hits.append(f"{bench_rel}:0 missing project benchmark inclusion for {name}")

    return hits


def scan_project_config_writers(
    fixture_path: pathlib.Path,
    compile_guard_path: pathlib.Path,
    bench_path: pathlib.Path,
) -> list[str]:
    """Ensure shared project rows use shared config writer functions."""
    hits: list[str] = []
    fixture_rel = relative_path(fixture_path)
    compile_rel = relative_path(compile_guard_path)
    bench_rel = relative_path(bench_path)

    if not fixture_path.exists():
        return [f"{fixture_rel}:0 project fixture metadata file is missing"]
    if not compile_guard_path.exists():
        return [f"{compile_rel}:0 project compile guard is missing"]
    if not bench_path.exists():
        return [f"{bench_rel}:0 benchmark runner is missing"]

    fixture_text = fixture_path.read_text(encoding="utf-8", errors="ignore")
    compile_text = compile_guard_path.read_text(encoding="utf-8", errors="ignore")
    bench_text = bench_path.read_text(encoding="utf-8", errors="ignore")

    for row, writer in sorted(PROJECT_CONFIG_WRITERS.items()):
        if not re.search(rf"\b{re.escape(writer)}\s*\(\)", fixture_text):
            hits.append(f"{fixture_rel}:0 missing shared config writer {writer} for {row}")

        if row not in BENCHMARK_ONLY_PROJECT_ROWS and not re.search(
            rf"\b{re.escape(writer)}\b",
            compile_text,
        ):
            hits.append(f"{compile_rel}:0 {row} does not use shared config writer {writer}")

        if row not in COMPILE_GUARD_ONLY_PROJECT_ROWS and not re.search(
            rf"\b{re.escape(writer)}\b",
            bench_text,
        ):
            hits.append(f"{bench_rel}:0 {row} does not use shared config writer {writer}")

    return hits


