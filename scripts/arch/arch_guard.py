#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Optional

ARCH_DIR = Path(__file__).resolve().parent
if str(ARCH_DIR) not in sys.path:
    sys.path.insert(0, str(ARCH_DIR))

import arch_guard_project as _arch_guard_project
from arch_guard_counts import *  # noqa: F401,F403
from arch_guard_file_limits import *  # noqa: F401,F403
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


def write_json_report(report_path: Path, payload: dict) -> None:
    report_path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = report_path.with_name(f".{report_path.name}.tmp")
    temp_path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    temp_path.replace(report_path)


def _run_git(root: Path, args: list[str]) -> Optional[str]:
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


def build_git_context(root: Path = ROOT, run_git=_run_git) -> dict:
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


def build_json_payload(
    failures: list[tuple[str, list[str]]],
    total_hits: int,
    git_context: Optional[dict] = None,
) -> dict:
    ok = not failures
    failed_hit_count = sum(len(hits) for _, hits in failures)
    return {
        "ok": ok,
        "status": "failed" if failures else "passed",
        "arch_guard_status": "failed" if failures else "passed",
        "git_context": git_context if git_context is not None else build_git_context(),
        "total_hits": total_hits,
        "failure_count": len(failures),
        "failed_hit_count": failed_hit_count,
        "failures": [{"name": name, "hits": hits} for name, hits in failures],
    }


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
    parser.add_argument(
        "--size-only",
        action="store_true",
        help=(
            "Run ONLY the physical-line-count checks (the per-crate LOC cap and "
            "per-file size ratchets) and skip the #8225 quarantine and other "
            "architecture-health metrics. Used by the CI size gate so a file "
            "crossing its size ceiling fails without redding main on pre-existing "
            "quarantine debt (which carries arch-owner-domain baselines)."
        ),
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

    if args.size_only:
        # CI size gate: run ONLY the physical-line-count checks (the per-crate
        # LOC cap in LINE_LIMIT_CHECKS + the per-file size ratchets in
        # FILE_LINE_LIMIT_CHECKS). This lets a merge_group/push CI job block a
        # file crossing its size ceiling — the gap that let context/mod.rs land
        # at 2030 lines with all jobs green — without failing on the #8225
        # quarantine and other health metrics, whose baselines are arch-owner
        # domain and are tracked/paid down separately.
        size_failures = []
        size_total = 0
        for name, base, limit, *rest in LINE_LIMIT_CHECKS:
            if not base.exists():
                continue
            exclude_files = rest[0] if rest else None
            hits = scan_line_limits(base, limit, exclude_files)
            size_total += len(hits)
            if hits:
                size_failures.append((name, hits))
        coverage_missing = scan_line_limit_coverage()
        size_total += len(coverage_missing)
        if coverage_missing:
            size_failures.append((LINE_LIMIT_COVERAGE_NAME, coverage_missing))
        for name, base, limit, *rest in TESTS_LINE_LIMIT_CHECKS:
            if not base.exists():
                continue
            exclude_files = rest[0] if rest else None
            hits = scan_line_limits(base, limit, exclude_files)
            size_total += len(hits)
            if hits:
                size_failures.append((name, hits))
        tests_coverage_missing = scan_tests_line_limit_coverage()
        size_total += len(tests_coverage_missing)
        if tests_coverage_missing:
            size_failures.append((TESTS_LINE_LIMIT_COVERAGE_NAME, tests_coverage_missing))
        for name, base, limit, *rest in SCRIPTS_LINE_LIMIT_CHECKS:
            if not base.exists():
                continue
            exclude_files = rest[0] if rest else None
            hits = scan_script_line_limits(base, limit, exclude_files)
            size_total += len(hits)
            if hits:
                size_failures.append((name, hits))
        for name, path, limit in FILE_LINE_LIMIT_CHECKS:
            # The #8225 common-quarantine ratchets carry their own arch-owner
            # green-campaign headroom policy and are tracked as debt separately;
            # the campaign does not bump or pay them down. The CI size gate
            # enforces the general per-file size ceilings, not the quarantine, so
            # skip #8225 entries — the gate still blocks ordinary size growth (the
            # per-crate LOC cap plus non-quarantine ratchets) and stays green on
            # main instead of failing on pre-existing quarantine debt.
            if "#8225" in name:
                continue
            hits = scan_file_line_limit(path, limit)
            size_total += len(hits)
            if hits:
                size_failures.append((name, hits))

        allowlist_gap = scan_allowlist_ratchet_coverage()
        size_total += len(allowlist_gap)
        if allowlist_gap:
            size_failures.append((ALLOWLIST_RATCHET_COVERAGE_NAME, allowlist_gap))

        ceiling_violations = scan_ceiling_contract_violations()
        size_total += len(ceiling_violations)
        if ceiling_violations:
            size_failures.append(
                (CEILING_CONTRACT_VIOLATION_NAME, ceiling_violations)
            )

        payload = build_json_payload(size_failures, size_total)
        if args.json_report:
            write_json_report(Path(args.json_report), payload)
        if args.json:
            print(json.dumps(payload, indent=2))
            return 0 if not size_failures else 1
        if size_failures:
            print("ARCH GUARD FAILURES (size-only):")
            for name, hits in size_failures:
                print(f"- {name}:")
                for hit in hits:
                    print(f"  - {hit}")
            return 1
        print("Architecture size guardrails passed (size-only).")
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

    coverage_missing = scan_line_limit_coverage()
    total_hits += len(coverage_missing)
    if coverage_missing:
        failures.append((LINE_LIMIT_COVERAGE_NAME, coverage_missing))

    for name, base, limit, *rest in TESTS_LINE_LIMIT_CHECKS:
        if not base.exists():
            continue
        exclude_files = rest[0] if rest else None
        hits = scan_line_limits(base, limit, exclude_files)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    tests_coverage_missing = scan_tests_line_limit_coverage()
    total_hits += len(tests_coverage_missing)
    if tests_coverage_missing:
        failures.append((TESTS_LINE_LIMIT_COVERAGE_NAME, tests_coverage_missing))

    for name, base, limit, *rest in SCRIPTS_LINE_LIMIT_CHECKS:
        if not base.exists():
            continue
        exclude_files = rest[0] if rest else None
        hits = scan_script_line_limits(base, limit, exclude_files)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    for name, path, limit in FILE_LINE_LIMIT_CHECKS:
        hits = scan_file_line_limit(path, limit)
        total_hits += len(hits)
        if hits:
            failures.append((name, hits))

    allowlist_gap = scan_allowlist_ratchet_coverage()
    total_hits += len(allowlist_gap)
    if allowlist_gap:
        failures.append((ALLOWLIST_RATCHET_COVERAGE_NAME, allowlist_gap))

    ceiling_violations = scan_ceiling_contract_violations()
    total_hits += len(ceiling_violations)
    if ceiling_violations:
        failures.append((CEILING_CONTRACT_VIOLATION_NAME, ceiling_violations))

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
        search_roots,
        exclude_path_prefixes,
        max_references,
    ) in MODULE_PATH_SOLVER_COMPUTATION_IMPORT_COUNT_CHECKS:
        hits = scan_module_path_solver_computation_import_count(
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

    for name, lib_rs_paths in CFG_TEST_GATED_PATH_MOD_CHECKS:
        hits = scan_cfg_test_gated_path_mod(lib_rs_paths)
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

    payload = build_json_payload(failures, total_hits)

    if args.json_report:
        write_json_report(Path(args.json_report), payload)

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
