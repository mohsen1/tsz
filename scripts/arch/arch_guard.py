#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path

ARCH_DIR = Path(__file__).resolve().parent
if str(ARCH_DIR) not in sys.path:
    sys.path.insert(0, str(ARCH_DIR))

import arch_guard_project as _arch_guard_project
from arch_guard_counts import *  # noqa: F401,F403
from arch_guard_project import *  # noqa: F401,F403
from arch_guard_rust import *  # noqa: F401,F403
from arch_guard_shared import *  # noqa: F401,F403
from arch_guard_shared import (  # noqa: F401
    _load_manifest_checks,
    _load_pattern_checks,
    _load_policy_toml,
)


def scan_project_config_writers(fixture_path, compile_guard_path, bench_path):
    _arch_guard_project.PROJECT_CONFIG_WRITERS = PROJECT_CONFIG_WRITERS
    return _arch_guard_project.scan_project_config_writers(
        fixture_path,
        compile_guard_path,
        bench_path,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run TSZ architecture guardrails"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable output instead of human-readable diagnostics.",
    )
    parser.add_argument(
        "--json-report",
        metavar="PATH",
        default="",
        help="Write machine-readable report to this path (still exits non-zero on failures).",
    )
    parser.add_argument(
        "--checker-context-lifetime-table",
        action="store_true",
        help="Print the CheckerContext lifetime manifest as a markdown table.",
    )
    args = parser.parse_args()

    if args.checker_context_lifetime_table:
        name, struct_path, struct_name, manifest_path = (
            CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS[0]
        )
        hits = scan_checker_context_lifetime_manifest(
            struct_path, struct_name, manifest_path
        )
        if hits:
            print(name)
            for hit in hits:
                print(f"  {hit}")
            return 1
        print(checker_context_lifetime_markdown(struct_path, struct_name, manifest_path))
        return 0

    failures = []
    total_hits = 0
    for name, base, pattern, excludes in CHECKS:
        if not base.exists():
            continue
        hits = scan(base, pattern, excludes)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, manifest_path, pattern in MANIFEST_CHECKS:
        if not manifest_path.exists():
            continue
        text = manifest_path.read_text(encoding="utf-8", errors="ignore")
        hits = []
        for i, line in enumerate(text.splitlines(), start=1):
            if pattern.search(line):
                rel = manifest_path.relative_to(ROOT).as_posix()
                hits.append(f"{rel}:{i}")
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, base, limit, *rest in LINE_LIMIT_CHECKS:
        if not base.exists():
            continue
        exclude_files = rest[0] if rest else None
        hits = scan_line_limits(base, limit, exclude_files)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, limit in FILE_LINE_LIMIT_CHECKS:
        hits = scan_file_line_limit(path, limit)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, struct_name, max_fields in STRUCT_FIELD_COUNT_CHECKS:
        hits = scan_struct_field_count(path, struct_name, max_fields)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, trait_name, max_methods in TRAIT_METHOD_COUNT_CHECKS:
        hits = scan_trait_method_count(path, trait_name, max_methods)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        struct_path,
        struct_name,
        manifest_path,
    ) in CHECKER_CONTEXT_LIFETIME_MANIFEST_CHECKS:
        hits = scan_checker_context_lifetime_manifest(
            struct_path, struct_name, manifest_path
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, max_pipelines in INDEPENDENT_PIPELINE_CHECKS:
        hits = scan_independent_pipelines(search_roots, max_pipelines)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_imports,
    ) in SOLVER_IMPORT_COUNT_CHECKS:
        hits = scan_solver_import_count(
            search_roots, exclude_path_prefixes, max_imports
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_references,
    ) in ROOT_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS:
        hits = scan_root_solver_computation_import_count(
            search_roots, exclude_path_prefixes, max_references
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        file_path,
        root_module_prefixes,
        max_reexports,
    ) in ROOT_SOLVER_EXPLICIT_REEXPORT_COUNT_CHECKS:
        hits = scan_solver_root_explicit_reexport_count(
            file_path, root_module_prefixes, max_reexports
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_references,
    ) in QUERY_BOUNDARY_COMMON_REFERENCE_COUNT_CHECKS:
        hits = scan_query_boundary_common_reference_count(
            search_roots, exclude_path_prefixes, max_references
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        file_path,
        max_allowances,
    ) in QUERY_BOUNDARY_MODULE_ALLOWANCE_COUNT_CHECKS:
        hits = scan_query_boundary_module_allowance_count(file_path, max_allowances)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        max_count,
    ) in WORKSPACE_CLIPPY_ALLOW_COUNT_CHECKS:
        hits = scan_workspace_clippy_allow_count(search_roots, max_count)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for (
        name,
        search_roots,
        exclude_path_prefixes,
        max_files,
    ) in SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS:
        hits = scan_snapshot_rollback_file_count(
            search_roots, exclude_path_prefixes, max_files
        )
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path, max_methods in LSP_FEATURE_METHOD_COUNT_CHECKS:
        hits = scan_lsp_feature_method_count(file_path, max_methods)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path, max_guard_count in SPECULATION_GUARD_NAME_CHECKS:
        hits = scan_speculation_guard_struct_count(file_path, max_guard_count)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, root, scan_dirs in DEBUG_PRINT_MACRO_CHECKS:
        hits = scan_debug_print_macros(root, scan_dirs)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, file_path in PROJECT_DASHBOARD_ROW_CHECKS:
        hits = scan_project_dashboard_rows(file_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, row_path, fixture_path in PROJECT_FIXTURE_SOURCE_CHECKS:
        hits = scan_project_fixture_sources(row_path, fixture_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, row_path, compile_guard_path, bench_path in PROJECT_INCLUSION_POLICY_CHECKS:
        hits = scan_project_inclusion_policy(row_path, compile_guard_path, bench_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, fixture_path, compile_guard_path, bench_path in PROJECT_CONFIG_WRITER_CHECKS:
        hits = scan_project_config_writers(fixture_path, compile_guard_path, bench_path)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, pattern, max_lines in REGEX_LINE_COUNT_CHECKS:
        hits = scan_regex_line_count(search_roots, pattern, max_lines)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, search_roots, allowlist in BRANCH_LOCAL_VISITED_CLONE_CHECKS:
        hits = scan_branch_local_visited_clones(search_roots, allowlist)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    solver_typedata_hits = scan_solver_typedata_quarantine(ROOT / "crates" / "tsz-solver")
    total_hits += len(solver_typedata_hits)
    if solver_typedata_hits:
        failures.append(
            (
                "Solver TypeData construction must stay in interner files",
                solver_typedata_hits,
            )
        )

    payload = {
        "status": "failed" if failures else "passed",
        "total_hits": total_hits,
        "failures": [{"name": name, "hits": hits} for name, hits in failures],
    }

    if args.json_report:
        report_path = Path(args.json_report)
        if report_path.parent != Path("."):
            report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if not failures else 1

    if failures:
        print("ARCH GUARD FAILURES:")
        for name, hits in failures:
            print(f"- {name}:")
            for hit in hits[:200]:
                print(f"  - {hit}")
            if len(hits) > 200:
                extra = len(hits) - 200
                print(f"  - ... and {extra} more")
        return 1

    print("Architecture guardrails passed.")
    return 0



if __name__ == "__main__":
    sys.exit(main())
