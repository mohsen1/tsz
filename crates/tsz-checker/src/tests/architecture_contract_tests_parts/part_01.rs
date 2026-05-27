/// Enforce the `query_boundaries` policy: checker files outside `query_boundaries`/ and tests/
/// should only import "SAFE" items (type handles, structural shapes, visitors) from `tsz_solver`.
/// All computation/construction imports should go through `query_boundaries` wrappers.
///
/// The allowlist below enumerates items that are read-only type handles, structural shapes,
/// or visitor functions that don't perform computation. Everything else must be wrapped.
#[test]
fn test_solver_imports_go_through_query_boundaries() {
    // ── SAFE imports: type handles, structural shapes, visitor functions ──
    // These are read-only identity types or inspection functions that don't
    // perform computation. They may be imported directly by any checker file.
    //
    // Maintain this list alphabetically for easy auditing.
    const SAFE_IMPORTS: &[&str] = &[
        // Type identity handles
        "TypeId",
        "MappedTypeId",
        // Structural shape types (read-only data)
        "CachedPropertyType",
        "CallSignature",
        "CallableShape",
        "FunctionShape",
        "IndexKind",
        "IndexSignature",
        "ObjectShape",
        "ParamInfo",
        "PropertyInfo",
        "TupleElement",
        "TypeParamInfo",
        "TypePredicate",
        "TypePredicateTarget",
        "Visibility",
        // Narrowing/flow data types
        "GuardSense",
        "NarrowingContext",
        "SymbolRef",
        "TypeGuard",
        "TypeofKind",
        // Definition system types
        "def::DefId",
        "def::DefKind",
        "def::DefinitionInfo",
        "def::DefinitionStore",
        // Recursion control
        "recursion::DepthCounter",
        "recursion::RecursionGuard",
        "recursion::RecursionProfile",
        "recursion::RecursionResult",
        // Misc free functions used by a small number of checker files
        // (all others must go through query_boundaries/)
        "is_compiler_managed_type",
        "type_contains_undefined",
    ];

    // ── TODO: These imports bypass query_boundaries but wrappers don't exist yet. ──
    // Each entry is (item, list of files using it). When a wrapper is created,
    // remove the entry and let the test enforce the boundary.
    const TEMPORARILY_ALLOWED: &[&str] = &[
        // All solver imports now go through query_boundaries — list is empty.
    ];

    fn walk_rs(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs(&checker_src, &mut files);

    /// Recursively expand a `use tsz_solver::...` import statement into
    /// individual canonical item paths. Handles nested brace groups like:
    ///   `{TypeId, type_queries::{Foo, Bar}}` -> ["TypeId", "`type_queries::Foo`", "`type_queries::Bar`"]
    /// Also strips `as Alias` suffixes so `CallSignature as SolverCallSignature`
    /// is checked as just `CallSignature`.
    fn expand_import(raw: &str) -> Vec<String> {
        let raw = raw.trim().trim_end_matches(';').trim();

        // Split a top-level comma-separated list respecting brace nesting.
        fn split_top_level(s: &str) -> Vec<String> {
            let mut items = Vec::new();
            let mut depth = 0;
            let mut start = 0;
            for (i, c) in s.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        let item = s[start..i].trim();
                        if !item.is_empty() {
                            items.push(item.to_string());
                        }
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            let last = s[start..].trim();
            if !last.is_empty() {
                items.push(last.to_string());
            }
            items
        }

        fn expand_with_prefix(prefix: &str, body: &str) -> Vec<String> {
            let body = body.trim();
            if body.starts_with('{') && body.ends_with('}') {
                let inner = &body[1..body.len() - 1];
                let parts = split_top_level(inner);
                let mut result = Vec::new();
                for part in parts {
                    result.extend(expand_with_prefix(prefix, &part));
                }
                return result;
            }

            // Check for nested module path: `mod::{A, B}` or `mod::Item`
            if let Some(brace_start) = body.find('{') {
                // Find the `::` before the brace
                let before_brace = body[..brace_start].trim_end_matches(':');
                let sub_prefix = if prefix.is_empty() {
                    before_brace.trim_end_matches(':').to_string()
                } else {
                    format!("{}::{}", prefix, before_brace.trim_end_matches(':'))
                };
                let rest = &body[brace_start..];
                return expand_with_prefix(&sub_prefix, rest);
            }

            // Strip `as Alias` suffix
            let item = if let Some(as_pos) = body.find(" as ") {
                body[..as_pos].trim()
            } else {
                body.trim()
            };

            if item.is_empty() {
                return vec![];
            }

            if prefix.is_empty() {
                vec![item.to_string()]
            } else {
                vec![format!("{}::{}", prefix, item)]
            }
        }

        expand_with_prefix("", raw)
    }

    fn is_allowed(item: &str, safe: &[&str], temp: &[&str]) -> bool {
        safe.contains(&item) || temp.contains(&item)
    }

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip excluded directories
        if rel.starts_with("tests/")
            || rel.starts_with("query_boundaries/")
            || rel.ends_with("_tests.rs")
        {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Find all `use tsz_solver::...;` imports (handles multi-line with braces)
        // We scan for lines starting with `use tsz_solver::` and collect until `;`
        // Imports inside `mod tests { ... }` blocks are exempt (test-only setup).
        let mut in_use = false;
        let mut use_buf = String::new();
        // Track whether we are inside an inline test module (`mod tests { ... }`).
        // We use simple open/close brace counting once we see `mod tests`.
        let mut in_test_mod_depth: i32 = 0;

        for line in src.lines() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            // Detect entry into a test module and track depth.
            if in_test_mod_depth == 0
                && (trimmed.starts_with("mod tests")
                    || trimmed.starts_with("#[cfg(test)]")
                    || trimmed == "#[cfg(test)]")
            {
                // Count any opening braces on this line to start tracking.
                for ch in trimmed.chars() {
                    if ch == '{' {
                        in_test_mod_depth += 1;
                    } else if ch == '}' {
                        in_test_mod_depth -= 1;
                    }
                }
                continue;
            }
            if in_test_mod_depth > 0 {
                for ch in trimmed.chars() {
                    if ch == '{' {
                        in_test_mod_depth += 1;
                    } else if ch == '}' {
                        in_test_mod_depth -= 1;
                    }
                }
                continue; // skip all content inside test modules
            }

            if !in_use {
                if let Some(rest) = trimmed.strip_prefix("use tsz_solver::") {
                    use_buf.clear();
                    use_buf.push_str(rest);
                    if rest.contains(';') {
                        in_use = false;
                        let stmt = use_buf.trim_end_matches(';').to_string();
                        for item in expand_import(&stmt) {
                            if !is_allowed(&item, SAFE_IMPORTS, TEMPORARILY_ALLOWED) {
                                violations.push(format!(
                                    "File {rel} imports tsz_solver::{item} directly. \
                                     Add a wrapper in query_boundaries/ and use that instead."
                                ));
                            }
                        }
                    } else {
                        in_use = true;
                    }
                }
            } else {
                use_buf.push(' ');
                use_buf.push_str(trimmed);
                if trimmed.contains(';') {
                    in_use = false;
                    let stmt = use_buf.trim_end_matches(';').to_string();
                    for item in expand_import(&stmt) {
                        if !is_allowed(&item, SAFE_IMPORTS, TEMPORARILY_ALLOWED) {
                            violations.push(format!(
                                "File {rel} imports tsz_solver::{item} directly. \
                                 Add a wrapper in query_boundaries/ and use that instead."
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "query_boundaries policy violations found. Non-allowlisted tsz_solver \
         imports detected outside query_boundaries/:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.1 — Architecture Invariant Coverage Checklist
// =============================================================================
//
// CLAUDE.md Rule -> Test Coverage mapping:
//
// SECTION 3: Responsibility Split
// - [x] Scanner: no downstream imports                    -> test_scanner_must_not_import_downstream_crates
// - [x] Parser: no binder/checker/solver imports          -> test_parser_must_not_import_binder_checker_solver
// - [x] Binder: no solver imports                         -> test_binder_must_not_import_solver
// - [x] Emitter: no checker internal imports              -> test_emitter_must_not_import_checker_internals
// - [x] Solver: no parser/checker imports                 -> test_solver_sources_forbid_parser_checker_imports (existing)
//
// SECTION 4: Hard Architecture Rules
// - [x] No TypeKey in checker                             -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] No raw interner access in checker                 -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] No TypeData construction in checker               -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
// - [x] CallEvaluator quarantined to query_boundaries     -> test_direct_call_evaluator_usage_is_quarantined_to_query_boundaries (existing)
// - [x] No SubtypeChecker construction outside boundaries -> test_no_direct_subtype_checker_construction_outside_query_boundaries
// - [x] No CompatChecker::with_resolver outside boundaries-> assignability/call boundary guards (existing)
// - [x] Solver imports go through query_boundaries        -> test_solver_imports_go_through_query_boundaries (existing)
//
// SECTION 5: Judge/Lawyer Model
// - [x] No direct CompatChecker for TS2322 paths          -> test_assignment_and_binding_default_assignability_use_central_gateway_helpers (existing)
// - [x] Assignability mismatch quarantined                -> test_direct_assignability_mismatch_decision_usage_is_quarantined (existing)
//
// SECTION 6: DefId-First Semantic Type Resolution
// - [x] No ad-hoc TypeData::Lazy interning                -> test_array_helpers_avoid_direct_typekey_interning (existing)
// - [x] instanceof constructor narrowing uses real DefIds -> test_instanceof_constructor_branches_avoid_raw_symbol_reference_fallback
// - [x] ArrayBuffer.isView fallback uses real DefIds      -> test_array_buffer_is_view_avoids_raw_symbol_reference_fallback
// - [x] No new raw SymbolRef reference construction       -> test_checker_raw_symbol_reference_construction_budget
// - [x] ensure_relation_input_ready used before relations  -> test_subtype_path_establishes_preconditions_before_subtype_cache_lookup (existing)
//
// SECTION 11: Solver Contracts
// - [x] No solver cache types in checker                  -> test_no_solver_cache_types_in_checker
// - [x] TypeCache excludes eval caches                    -> test_type_cache_surface_excludes_application_and_mapped_eval_caches (existing)
//
// SECTION 12: Checker Contracts
// - [x] Checker files under 2000 LOC                      -> checker_files_stay_under_loc_limit (existing)
// - [x] All diagnostics through error_reporter            -> test_no_push_diagnostic_outside_error_reporter (existing)
// - [x] query_boundaries coverage ratio tracking          -> test_query_boundaries_coverage_ratio
//
// SECTION 13: Emitter Contracts
// - [x] error_reporter is pure formatting layer           -> test_error_reporter_does_not_perform_type_construction
//
// SECTION 15: Dependency Policy
// - [x] Dependency direction enforcement                  -> tests in Prompt 4.2 below
// - [x] No checker access to solver internals             -> test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning (existing)
//
// SECTION 22: TS2322 Priority Rules
// - [x] TS2322 paths through query_boundaries             -> test_assignment_and_binding_default_assignability_use_central_gateway_helpers (existing)
// - [x] No direct CompatChecker for TS2322                -> call boundary guard (existing)
// - [x] Centralized assignability gateways                -> multiple existing tests
//
// RATCHET GUARDS (debt tracking):
// - [x] TEMPORARILY_ALLOWED bypass list capped at 38      -> test_temporarily_allowed_bypass_list_does_not_grow
// - [x] Direct interner type construction capped at 13    -> test_direct_interner_type_construction_ceiling
// - [x] Checker file size ceiling (4 files > 2000 LOC)    -> test_checker_file_size_ceiling
// - [x] Max single file LOC ceiling (2394 lines)          -> test_checker_file_size_ceiling
// - [x] CLI must not import checker internals             -> test_cli_must_not_import_checker_internals
//

// =============================================================================
// Prompt 4.2 — Dependency Direction Tests
// =============================================================================

/// Helper: recursively walk a directory collecting production `.rs` files.
fn walk_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "tests" {
                continue;
            }
            walk_rs_files_recursive(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if path.file_name().is_some_and(|name| name == "tests.rs") {
                continue;
            }
            files.push(path);
        }
    }
}

/// CLAUDE.md §4: Binder must not import Solver.
/// The binder produces symbols, scopes, and flow graphs without type computation.
#[test]
fn test_binder_must_not_import_solver() {
    let binder_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-binder/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&binder_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_solver") || line.contains("tsz_solver::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Binder must not import Solver (CLAUDE.md §4). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Emitter must not import Checker internals.
/// The emitter prints/transforms output; no on-the-fly semantic type validation.
#[test]
fn test_emitter_must_not_import_checker_internals() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-emitter/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_checker") || line.contains("tsz_checker::") {
                violations.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Emitter must not import Checker internals (CLAUDE.md §4/§13). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Track 9/10 ratchet: direct `tsz_solver` access from the emitter must not grow.
///
/// Existing declaration emit code still reaches into solver APIs while the
/// `DeclarationSummary`/`PublicApiSummary` boundary is being introduced. This
/// guard keeps that debt measurable and forces new emit/DTS work to either use a
/// semantic summary/query boundary or explicitly lower this ceiling after
/// removing old reach-through.
#[test]
fn test_emitter_direct_solver_access_does_not_grow() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-emitter/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut direct_solver_lines = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("use tsz_solver") || line.contains("tsz_solver::") {
                direct_solver_lines.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    const DIRECT_SOLVER_ACCESS_LINE_CEILING: usize = 478;
    assert!(
        direct_solver_lines.len() <= DIRECT_SOLVER_ACCESS_LINE_CEILING,
        "Emitter direct solver access grew to {} lines (ceiling: {}). \
         Route new emit/DTS semantic reads through a compiler semantic view, \
         declaration summary, or query boundary before increasing this debt. \
         Direct access lines:\n  {}",
        direct_solver_lines.len(),
        DIRECT_SOLVER_ACCESS_LINE_CEILING,
        direct_solver_lines.join("\n  ")
    );
}

/// Track 9/10 ratchet: emitter source-text recovery must not grow.
///
/// Existing JS/DTS emit still uses source text for parser-recovery and legacy
/// transform details. New recovery facts should come from parser/lowering
/// structures instead of adding more emitter substring scans.
#[test]
fn test_emitter_source_text_recovery_surface_does_not_grow() {
    let emitter_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-emitter/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&emitter_src, &mut files);

    let mut source_text_lines = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&emitter_src)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.ends_with("tests.rs") || rel.contains("/tests/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("source_text") {
                source_text_lines.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    // Bumped 828→852 for emitter helpers growth in helpers.rs; 852→865 for
    // additional emit fixes (pre-existing on main). Track a follow-up to route
    // new recovery through parser/lowering facts.
    const SOURCE_TEXT_RECOVERY_LINE_CEILING: usize = 865;
    assert!(
        source_text_lines.len() <= SOURCE_TEXT_RECOVERY_LINE_CEILING,
        "Emitter source-text recovery surface grew to {} lines (ceiling: {}). \
         Route new malformed-syntax or transform recovery through parser/lowering \
         facts instead of adding emitter substring scans. Source-text lines:\n  {}",
        source_text_lines.len(),
        SOURCE_TEXT_RECOVERY_LINE_CEILING,
        source_text_lines.join("\n  ")
    );
}

/// Track 10 ratchet: rendered type strings must not become new semantic inputs.
///
/// Existing checker code still has a small number of one-line decisions that
/// call `format_type`/`format_type_diagnostic` and immediately inspect the
/// rendered string. New decisions should use structural solver/query-boundary
/// facts instead.
#[test]
fn test_rendered_type_decision_patterns_do_not_grow() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut rendered_decisions = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            let formats_type =
                line.contains("format_type(") || line.contains("format_type_diagnostic(");
            let inspects_rendered = line.contains(".contains(")
                || line.contains(".starts_with(")
                || line.contains(".ends_with(")
                || line.contains(".as_str()");
            if formats_type && inspects_rendered {
                rendered_decisions.push(format!("{}:{}", path.display(), line_num + 1));
            }
        }
    }

    const RENDERED_TYPE_DECISION_LINE_CEILING: usize = 0;
    assert!(
        rendered_decisions.len() == RENDERED_TYPE_DECISION_LINE_CEILING,
        "Rendered-type semantic decision patterns grew to {} lines (ceiling: {}). \
         Route new decisions through structural solver/query-boundary facts instead \
         of inspecting formatted type strings. Rendered decision lines:\n  {}",
        rendered_decisions.len(),
        RENDERED_TYPE_DECISION_LINE_CEILING,
        rendered_decisions.join("\n  ")
    );
}

#[test]
fn test_top_rest_any_callable_policy_avoids_rendered_signature_prefixes() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/call_checker/diagnostics.rs");
    let src =
        fs::read_to_string(&path).unwrap_or_else(|_| panic!("failed to read {}", path.display()));
    let forbidden = [
        "format_type(type_id)",
        ".starts_with(\"(...args: Array<any>) =>\")",
        "(...args: Array<any>) =>",
    ];

    let violations = forbidden
        .iter()
        .filter(|pattern| src.contains(**pattern))
        .copied()
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "top rest-any callable policy must use TypeId facts, not rendered \
         signature prefixes. Violations in {}:\n  {}",
        path.display(),
        violations.join("\n  ")
    );
}

#[test]
fn test_callable_missing_property_suppression_uses_signature_queries() {
    let src = fs::read_to_string("src/error_reporter/assignability_callable_suppression.rs")
        .expect("failed to read assignability callable suppression source");

    for forbidden in [
        "format_type",
        "format_type_diagnostic",
        "is_function_type_display",
    ] {
        assert!(
            !src.contains(forbidden),
            "callable missing-property suppression must use TypeId signature queries, \
             not rendered type text: found {forbidden}"
        );
    }

    assert!(
        src.contains("get_call_signatures") && src.contains("get_construct_signatures"),
        "callable missing-property suppression should query call and construct signatures"
    );
}

#[test]
fn test_numeric_literal_union_display_policy_avoids_rendered_union_separator_decisions() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/error_reporter/assignability_numeric_display.rs");
    let src =
        fs::read_to_string(&path).unwrap_or_else(|_| panic!("failed to read {}", path.display()));
    let forbidden = [
        "source_order.contains(\" | \")",
        "member.contains(\" | \")",
        "member_displays.iter().all(|member| !member.contains(\" | \"))",
    ];

    let violations = forbidden
        .iter()
        .filter(|pattern| src.contains(**pattern))
        .copied()
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "numeric literal union display policy must use TypeId facts, not rendered \
         union separators. Violations in {}:\n  {}",
        path.display(),
        violations.join("\n  ")
    );
}

#[test]
fn test_assignability_literal_widening_uses_function_shape_queries() {
    let src = fs::read_to_string("src/error_reporter/assignability.rs")
        .expect("failed to read assignability reporter source");
    let helper_src = fs::read_to_string("src/error_reporter/assignability_type_helpers.rs")
        .expect("failed to read assignability type helper source");

    for forbidden in [
        "source_display.contains(\"=>\")",
        "target_display.contains(\"=>\")",
    ] {
        assert!(
            !src.contains(forbidden),
            "assignability literal-member widening must use TypeId function/callable \
             facts, not rendered arrow text: found {forbidden}"
        );
    }

    assert!(
        src.contains("is_function_like_for_literal_member_widening")
            && helper_src.contains("function_shape_for_type")
            && helper_src.contains("callable_shape_for_type"),
        "assignability literal-member widening should query function/callable shapes"
    );
}

/// CLAUDE.md §4: Scanner must not import downstream crates (Parser/Binder/Checker/Solver).
/// The scanner is the leaf of the pipeline; it only does lexing and string interning.
#[test]
fn test_scanner_must_not_import_downstream_crates() {
    let scanner_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-scanner/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&scanner_src, &mut files);

    let downstream_crates = [
        "tsz_parser",
        "tsz_binder",
        "tsz_checker",
        "tsz_solver",
        "tsz_emitter",
    ];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Scanner must not import downstream crates (CLAUDE.md §4/§8). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4: Parser must not import Binder/Checker/Solver.
/// The parser produces syntax-only AST; no semantic awareness.
#[test]
fn test_parser_must_not_import_binder_checker_solver() {
    let parser_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-parser/src");

    let mut files = Vec::new();
    walk_rs_files_recursive(&parser_src, &mut files);

    let downstream_crates = ["tsz_binder", "tsz_checker", "tsz_solver", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &downstream_crates {
                if line.contains(&format!("use {crate_name}"))
                    || line.contains(&format!("{crate_name}::"))
                {
                    violations.push(format!(
                        "{}:{}: imports {}",
                        path.display(),
                        line_num + 1,
                        crate_name
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Parser must not import Binder/Checker/Solver (CLAUDE.md §4/§9). Violations:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.3 — Solver Encapsulation Tests
// =============================================================================

/// CLAUDE.md §4/§6: No `TypeKey` usage in checker code.
/// `TypeKey` is solver-internal (crate-private); checker must use TypeId/TypeData.
#[test]
fn test_no_typekey_in_checker_code() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // Check for TypeKey as a distinct identifier (not part of another word)
            if line
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .any(|token| token == "TypeKey")
            {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "TypeKey is solver-internal and must not appear in checker code (CLAUDE.md §4/§6). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §11: No solver cache access types (`RelationCacheProbe`, etc.) in checker code.
/// Solver owns algorithmic caches; checker must not access them directly.
#[test]
fn test_no_solver_cache_types_in_checker() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let cache_types = [
        "RelationCacheProbe",
        "EvaluationCache",
        "InstantiationCache",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("*") {
                continue;
            }
            for cache_type in &cache_types {
                if line
                    .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                    .any(|token| token == *cache_type)
                {
                    violations.push(format!("{}:{}: uses {}", rel, line_num + 1, cache_type));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Solver cache types must not appear in checker code (CLAUDE.md §11). Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// CLAUDE.md §4/§22: No direct `SubtypeChecker` construction outside `query_boundaries`.
/// Relation checks should go through boundary helpers.
#[test]
fn test_no_direct_subtype_checker_construction_outside_query_boundaries() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if line.contains("SubtypeChecker::new(") || line.contains("SubtypeChecker {") {
                violations.push(format!("{}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "SubtypeChecker must not be constructed outside query_boundaries (CLAUDE.md §4/§22). \
         Route relation checks through boundary helpers instead. Violations:\n  {}",
        violations.join("\n  ")
    );
}

// =============================================================================
// Prompt 4.4 — Structural Health Tests
// =============================================================================

/// CLAUDE.md §12: Track `query_boundaries` coverage ratio.
/// This is a directional metric -- warns if the ratio of direct solver imports
/// to `query_boundaries` usage is too high.
#[test]
fn test_query_boundaries_coverage_ratio() {
    let src_dir = Path::new("src");
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(src_dir, &mut files);

    let mut direct_solver_importers = 0u32;
    let mut boundary_users = 0u32;

    for path in &files {
        let rel = path.display().to_string();
        if rel.contains("/tests/") || rel.contains("/query_boundaries/") {
            continue;
        }
        let src = fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        let has_direct = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//")
                && (line.contains("use tsz_solver::") || line.contains("tsz_solver::"))
        });
        let has_boundary = src.lines().any(|line| {
            let t = line.trim_start();
            !t.starts_with("//") && line.contains("query_boundaries::")
        });

        if has_direct {
            direct_solver_importers += 1;
        }
        if has_boundary {
            boundary_users += 1;
        }
    }

    // This is a directional metric. We want the ratio to decrease over time.
    // Current target: direct importers should be < 4x boundary users.
    let ratio = if boundary_users == 0 {
        f64::INFINITY
    } else {
        direct_solver_importers as f64 / boundary_users as f64
    };

    // Warn but don't fail -- this is a tracking metric
    // Tracking metric: warn threshold at 4.0 (currently informational only)
    let _ = ratio > 4.0;

    // Hard fail if the ratio degrades catastrophically
    assert!(
        ratio < 10.0,
        "query_boundaries coverage ratio has degraded to {ratio:.1}:1 \
         ({direct_solver_importers} direct solver importers vs {boundary_users} boundary users). \
         This indicates systematic boundary bypass. Target: < 4:1"
    );
}

// ========================================================================
// Ambient context transport: TypingRequest migration contract tests
// ========================================================================
//
// These tests enforce that files fully migrated to the TypingRequest API
// do not regress by re-introducing raw mutations of the ambient context
// fields: `ctx.contextual_type =`, `ctx.contextual_type_is_assertion =`,
// and `ctx.skip_flow_narrowing =`.
//
// Legacy ambient state still exists in a few non-migrated subsystems, but
// the request-first hot path must not regress.

/// Migrated files must not contain raw `ctx.contextual_type =` assignments.
/// They should use `get_type_of_node_with_request` instead.
#[test]
fn migrated_files_no_raw_contextual_type_mutation() {
    let migrated_files = &[
        "types/computation/object_literal_context.rs",
        "types/computation/array_literal.rs",
        "types/queries/binding.rs",
        "types/type_checking/core.rs",
        "declarations/import/core/mod.rs",
        "assignability/assignment_checker/mod.rs",
        // property_access_type.rs migrated skip_flow_narrowing, not contextual_type
        // Wave 2 migrations:
        "assignability/compound_assignment.rs",
        "error_reporter/call_errors/mod.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking/property.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core_statement_checks.rs",
        "types/computation/binary.rs",
        "types/computation/access.rs",
        "types/computation/tagged_template.rs",
        // Wave 3 migrations:
        "types/computation/call_helpers.rs",
        "checkers/parameter_checker.rs",
        "types/utilities/return_type.rs",
        "checkers/call_checker/mod.rs",
        "checkers/call_checker/applicability.rs",
        "checkers/call_checker/candidate_collection.rs",
        "checkers/call_checker/diagnostics.rs",
        "checkers/call_checker/overload_resolution.rs",
        "types/computation/call_inference.rs",
        "dispatch.rs",
        "checkers/jsx/orchestration",
        "checkers/jsx/children.rs",
        "checkers/jsx/props/mod.rs",
        "checkers/jsx/props/resolution.rs",
        "checkers/jsx/props/validation.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call/mod.rs",
        "types/computation/object_literal/mod.rs",
        "types/computation/helpers.rs",
        "types/computation/call_display.rs",
        "types/function_type.rs",
        "types/class_type/constructor.rs",
        "state/state.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        // Count raw mutations (exclude comments and the TypingRequest module itself)
        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                // Skip comments
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                // Detect raw mutation patterns
                trimmed.contains("ctx.contextual_type =") || trimmed.contains(".contextual_type = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.skip_flow_narrowing =` assignments.
#[test]
fn migrated_files_no_raw_skip_flow_narrowing_mutation() {
    let migrated_files = &[
        "types/property_access_type/helpers.rs",
        "types/property_access_type/resolve.rs",
        "types/computation/access.rs",
        "types/computation/helpers.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        // Wave 3: call_checker and call_inference migrated skip_flow via TypingRequest
        "checkers/call_checker/mod.rs",
        "checkers/call_checker/applicability.rs",
        "checkers/call_checker/candidate_collection.rs",
        "checkers/call_checker/diagnostics.rs",
        "checkers/call_checker/overload_resolution.rs",
        "types/computation/call_inference.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.skip_flow_narrowing =")
                    || trimmed.contains(".skip_flow_narrowing = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `skip_flow_narrowing =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated helper files must not read request intent from ambient checker fields.
#[test]
fn migrated_helper_files_no_raw_ambient_request_reads() {
    let migrated_files = &[
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/type_analysis/computed_helpers.rs",
        "types/property_access_type/helpers.rs",
        "types/property_access_type/resolve.rs",
        "state/variable_checking/destructuring.rs",
        "state/variable_checking/core.rs",
        "types/type_checking/core.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("self.ctx.contextual_type")
                    || trimmed.contains("self.ctx.contextual_type_is_assertion")
                    || trimmed.contains("self.ctx.skip_flow_narrowing")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not read request intent from ambient checker state:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Migrated files must not contain raw `ctx.contextual_type_is_assertion =` assignments.
#[test]
fn migrated_files_no_raw_contextual_assertion_mutation() {
    let migrated_files = &[
        "dispatch.rs",
        "checkers/jsx/orchestration",
        "checkers/jsx/children.rs",
        "checkers/jsx/props/mod.rs",
        "checkers/jsx/props/resolution.rs",
        "checkers/jsx/props/validation.rs",
        "checkers/jsx/runtime.rs",
        "checkers/jsx/diagnostics.rs",
        "types/computation/call/mod.rs",
        "types/computation/helpers.rs",
        "types/computation/object_literal/mod.rs",
        "types/function_type.rs",
        "state/state_checking_members/ambient_signature_checks.rs",
        "types/computation/tagged_template.rs",
        "types/class_type/constructor.rs",
        "state/state_checking_members/member_declaration_checks.rs",
        "state/state_checking/class.rs",
        "state/type_analysis/core.rs",
        "state/type_analysis/core_type_query.rs",
        "state/type_analysis/computed_helpers.rs",
        "state/type_analysis/computed_helpers_binding.rs",
        "state/variable_checking/destructuring.rs",
        "state/state.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in migrated_files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("ctx.contextual_type_is_assertion =")
                    || trimmed.contains(".contextual_type_is_assertion = ")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} has been migrated to TypingRequest but still contains \
             raw `contextual_type_is_assertion =` mutations:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The removed `run_with_typing_context` compatibility bridge must not reappear.
#[test]
fn no_typing_context_bridge_helper_or_calls() {
    let files = &[
        "state/state.rs",
        "dispatch.rs",
        "types/function_type.rs",
        "state/state_checking_members/statement_callback_bridge.rs",
    ];

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for file in files {
        let path = base.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        let violations: Vec<(usize, &str)> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                if trimmed.starts_with("//")
                    || trimmed.starts_with("/*")
                    || trimmed.starts_with("*")
                {
                    return false;
                }
                trimmed.contains("run_with_typing_context(")
                    || trimmed.contains("fn run_with_typing_context")
            })
            .collect();

        assert!(
            violations.is_empty(),
            "File {file} must not reintroduce the removed typing-context bridge:\n{}",
            violations
                .iter()
                .map(|(line_no, line)| format!("  line {}: {}", line_no + 1, line.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The request-aware cache bypass must stay confined to the approved entry points.
///
/// This blocks new blanket "if request is non-empty, bypass cache" logic from
/// being reintroduced into other checker main entry points.
#[test]
fn request_empty_cache_bypass_stays_confined_to_approved_entry_points() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allowlist = ["state/state.rs", "types/class_type/constructor.rs"];

    let mut checker_files = Vec::new();
    collect_checker_rs_files_recursive(&base, &mut checker_files);

    let mut violations = Vec::new();
    for path in checker_files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let relative = path
            .strip_prefix(&base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if allowlist.iter().any(|allowed| relative.ends_with(allowed)) {
            continue;
        }

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
                continue;
            }
            if trimmed.starts_with("if request.is_empty()")
                || trimmed.starts_with("let use_node_cache = request.is_empty()")
                || trimmed.starts_with("let can_use_cache = request.is_empty()")
            {
                violations.push(format!("{}:{}", relative, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "request-empty cache bypass logic must stay confined to state/state.rs and \
         types/class_type/constructor.rs; violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn request_aware_contextual_retry_hot_paths_do_not_reintroduce_recursive_cache_clears() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let whole_file_bans = [
        "assignability/assignment_checker/mod.rs",
        "state/state_checking/property.rs",
        "types/type_checking/core.rs",
    ];

    for relative in whole_file_bans {
        let path = base.join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));

        assert!(
            !source.contains("clear_type_cache_recursive("),
            "request-aware contextual retry path {relative} must use targeted invalidation helpers instead of direct recursive cache clears"
        );
    }
    let ambient_source =
        fs::read_to_string(base.join("state/state_checking_members/ambient_signature_checks.rs"))
            .expect("failed to read ambient_signature_checks.rs");
    assert!(
        ambient_source.contains("invalidate_initializer_for_context_change(prop.initializer)"),
        "ambient declared-type initializer retries must keep using the targeted invalidation helper"
    );
}

/// The `TypingRequest` type must exist and have the expected fields.
#[test]
fn typing_request_api_exists() {
    use crate::context::{ContextualOrigin, FlowIntent, TypingRequest};

    // Verify basic construction and field access
    let none = TypingRequest::NONE;
    assert!(none.is_empty());
    assert_eq!(none.contextual_type, None);
    assert_eq!(none.origin, ContextualOrigin::Normal);
    assert_eq!(none.flow, FlowIntent::Read);

    let with_ctx = TypingRequest::with_contextual_type(TypeId::STRING);
    assert_eq!(with_ctx.contextual_type, Some(TypeId::STRING));
    assert!(!with_ctx.origin.is_assertion());

    let assertion = TypingRequest::for_assertion(TypeId::NUMBER);
    assert!(assertion.origin.is_assertion());

    let write = TypingRequest::for_write_context();
    assert!(write.flow.skip_flow_narrowing());
}

/// Verify that the `statement_callback_bridge` save/restore for `check_statement`
/// is properly scoped (contextual type set only during `check_statement`, not leaked).
#[test]
fn statement_callback_bridge_contextual_type_scoping() {
    // This is a source-level check: the export clause handler must restore
    // contextual type BEFORE the assignability check, not after.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/state_checking_members/statement_callback_bridge.rs");
    let content = fs::read_to_string(&path).expect("Failed to read statement_callback_bridge.rs");

    // The file should use get_type_of_node_with_request for the get_type_of_node call
    assert!(
        content.contains("get_type_of_node_with_request"),
        "statement_callback_bridge.rs should use get_type_of_node_with_request for export clause typing"
    );
}

#[test]
fn semantic_diagnostic_reporters_must_route_primary_anchor_selection_through_fingerprint_policy() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let fingerprint_policy = fs::read_to_string(base.join("fingerprint_policy.rs"))
        .expect("failed to read src/error_reporter/fingerprint_policy.rs");
    assert!(
        fingerprint_policy.contains("enum DiagnosticAnchorKind"),
        "fingerprint_policy.rs must define the shared anchor policy"
    );
    assert!(
        fingerprint_policy.contains("resolve_diagnostic_anchor_node"),
        "fingerprint_policy.rs must provide shared anchor resolution"
    );

    let files = [
        "assignability.rs",
        "call_errors",
        "properties.rs",
        "generics.rs",
    ];
    let forbidden = [
        "assignment_diagnostic_anchor_idx(",
        "call_error_anchor_node(",
        "ts2769_first_arg_or_call(",
        "type_assertion_overlap_anchor(",
        "type_assertion_overlap_anchor_in_expression(",
        "build_related_from_failure_reason(",
    ];

    for file in files {
        let path = base.join(file);
        let content = if path.is_dir() {
            // Read all .rs files in the directory and concatenate
            let mut combined = String::new();
            for entry in fs::read_dir(&path).unwrap_or_else(|e| panic!("read dir {file}: {e}")) {
                let entry = entry.expect("failed to read dir entry");
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("rs")
                    && let Ok(c) = fs::read_to_string(&p)
                {
                    combined.push_str(&c);
                }
            }
            combined
        } else {
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {file}: {e}"))
        };
        assert!(
            content.contains("DiagnosticAnchorKind::")
                || content.contains("resolve_diagnostic_anchor(")
                || content.contains("resolve_diagnostic_anchor_node("),
            "File {file} must use the shared fingerprint policy for anchor selection"
        );

        for forbidden_pattern in forbidden {
            assert!(
                !content.contains(forbidden_pattern),
                "File {file} must not reintroduce bespoke primary-anchor helper `{forbidden_pattern}`"
            );
        }
    }
}

/// Ensures that `current_callable_type` is not reintroduced as ambient mutable state.
///
/// The callable type is now threaded explicitly via `CallableContext` through the call
/// argument collection pipeline. No file in the call-context lane should read or write
/// `ctx.current_callable_type`. The field has been removed from `CheckerContext`.
#[test]
fn no_ambient_current_callable_type() {
    let migrated_files = [
        "src/checkers/call_checker/mod.rs",
        "src/checkers/call_checker/applicability.rs",
        "src/checkers/call_checker/candidate_collection.rs",
        "src/checkers/call_checker/diagnostics.rs",
        "src/checkers/call_checker/overload_resolution.rs",
        "src/types/computation/call/mod.rs",
        "src/types/computation/call_inference.rs",
        "src/types/computation/call_display.rs",
        "src/state/type_analysis/computed_helpers.rs",
        "src/context/mod.rs",
        "src/context/constructors.rs",
    ];

    let checker_root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for file in migrated_files {
        let path = checker_root.join(file);
        let content = read_checker_source_file(&path.to_string_lossy());

        // Allow the doc comment in CallableContext's definition but forbid actual usage.
        // Filter out lines that are comments (starting with /// or //).
        let non_comment_lines: String = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("///") && !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !non_comment_lines.contains("current_callable_type"),
            "File {file} must not reference `current_callable_type` — \
             use explicit `CallableContext` threading instead"
        );
    }
}

/// Excess property classification logic (`ExcessPropertiesKind` pattern-matching)
/// must stay in the canonical path: `state/state_checking/property.rs` and
/// the `query_boundaries/assignability.rs` re-export.  Other checker files
/// must not reimplement this classification.
#[test]
fn test_excess_property_classification_quarantined_to_property_rs() {
    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let forbidden = [
        "ExcessPropertiesKind::Union",
        "ExcessPropertiesKind::Intersection",
        "ExcessPropertiesKind::Object(",
        "ExcessPropertiesKind::ObjectWithIndex(",
    ];

    let mut violations = Vec::new();
    for path in files {
        let rel = path.display().to_string();
        let allowed = rel.ends_with("state/state_checking/property.rs")
            || rel.ends_with("query_boundaries/assignability.rs")
            || rel.ends_with("assignability/assignability_diagnostics.rs") // target scoring
            || rel.ends_with("types/computation/object_literal_context.rs") // contextual type decomposition
            || rel.contains("/tests/");
        if allowed {
            continue;
        }
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for pattern in &forbidden {
            if src.contains(pattern) {
                violations.push(format!("{rel} contains {pattern}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ExcessPropertiesKind pattern-matching must stay in state/state_checking/property.rs; violations:\n{}",
        violations.join("\n")
    );
}

// ========================================================================
// Canonical RelationRequest / RelationOutcome boundary tests
// ========================================================================
//
// These tests enforce that the canonical `RelationRequest` / `RelationOutcome`
// / `execute_relation` boundary is the single authoritative path for relation
// queries that need structured failure information.

/// The `query_boundaries/assignability.rs` boundary must expose the unified
/// `execute_relation` helper and the `RelationOutcome` / `RelationRequest`
/// types that the checker uses for single-pass relation + failure collection.
#[test]
fn test_relation_request_and_outcome_live_in_query_boundaries() {
    let boundary_source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        boundary_source.contains("pub(crate) struct RelationRequest"),
        "RelationRequest must be defined in query_boundaries/assignability.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) struct RelationOutcome"),
        "RelationOutcome must be defined in query_boundaries/assignability.rs"
    );
    assert!(
        boundary_source.contains("pub(crate) fn execute_relation"),
        "execute_relation boundary helper must be defined in query_boundaries/assignability.rs"
    );

    // RelationRequest must encode all policy dimensions
    assert!(
        boundary_source.contains("pub kind: RelationKind"),
        "RelationRequest must include a RelationKind field"
    );
    assert!(
        boundary_source.contains("pub excess_property_mode: ExcessPropertyMode"),
        "RelationRequest must include an ExcessPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub missing_property_mode: MissingPropertyMode"),
        "RelationRequest must include a MissingPropertyMode field"
    );
    assert!(
        boundary_source.contains("pub source_is_fresh: bool"),
        "RelationRequest must include a source_is_fresh field"
    );

    // RelationOutcome must carry structured failure info
    assert!(
        boundary_source.contains("pub related: bool"),
        "RelationOutcome must include a `related` field"
    );
    assert!(
        boundary_source.contains("pub weak_union_violation: bool"),
        "RelationOutcome must include a `weak_union_violation` field"
    );
    assert!(
        boundary_source.contains("pub failure: Option<super::relation_types::RelationFailure>"),
        "RelationOutcome must include a structured `failure` field"
    );
}

/// The canonical request surface must continue exposing the full relation and
/// property-policy enum vocabulary, not implicit booleans.
#[test]
fn test_relation_request_policy_enums_cover_canonical_modes() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    for variant in [
        "Assign",
        "CallArg",
        "Return",
        "JsxProps",
        "Destructuring",
        "Satisfies",
    ] {
        assert!(
            source.contains(&"enum RelationKind".to_string()) && source.contains(variant),
            "RelationKind must include the `{variant}` variant"
        );
    }

    for variant in ["Skip", "Check", "CheckExplicitOnly"] {
        assert!(
            source.contains(&"enum ExcessPropertyMode".to_string()) && source.contains(variant),
            "ExcessPropertyMode must include the `{variant}` variant"
        );
    }

    for variant in ["Report", "Suppress"] {
        assert!(
            source.contains(&"enum MissingPropertyMode".to_string()) && source.contains(variant),
            "MissingPropertyMode must include the `{variant}` variant"
        );
    }
}

/// The canonical `RelationRequest::new` path must keep request policy defaults
/// explicit at the boundary instead of relying on ambient caller state.
#[test]
fn test_relation_request_new_encodes_default_policy() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("fn new(source: TypeId, target: TypeId, kind: RelationKind) -> Self"),
        "RelationRequest must keep a canonical new(...) constructor for default policy"
    );
    assert!(
        source.contains("excess_property_mode: ExcessPropertyMode::Skip,"),
        "RelationRequest::new must default excess_property_mode to Skip"
    );
    assert!(
        source.contains("missing_property_mode: MissingPropertyMode::Report,"),
        "RelationRequest::new must default missing_property_mode to Report"
    );
    assert!(
        source.contains("source_is_fresh: false,"),
        "RelationRequest::new must default source_is_fresh to false"
    );
}

/// The canonical request builders must preserve explicit override hooks for
/// excess-property and missing-property policy at the boundary.
#[test]
fn test_relation_request_override_builders_remain_explicit() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("fn with_excess_property_mode(mut self, mode: ExcessPropertyMode) -> Self"),
        "RelationRequest must keep with_excess_property_mode as the explicit EPC override hook"
    );
    assert!(
        source.contains("self.excess_property_mode = mode;"),
        "with_excess_property_mode must write the requested EPC mode into the request"
    );
    assert!(
        source
            .contains("fn with_missing_property_mode(mut self, mode: MissingPropertyMode) -> Self"),
        "RelationRequest must keep with_missing_property_mode as the explicit missing-property override hook"
    );
    assert!(
        source.contains("self.missing_property_mode = mode;"),
        "with_missing_property_mode must write the requested missing-property mode into the request"
    );
}

/// The boundary-owned `RelationFlags` wrapper must continue exposing the
/// checker-safe flag surface for request-sensitive relation policy.
#[test]
fn test_relation_flags_surface_covers_checker_policy_bits() {
    let source = fs::read_to_string("src/query_boundaries/assignability.rs")
        .expect("failed to read query_boundaries/assignability.rs");

    assert!(
        source.contains("pub(crate) struct RelationFlags;"),
        "assignability boundary must define RelationFlags as the checker-safe flag surface"
    );

    for flag in [
        "STRICT_NULL_CHECKS",
        "STRICT_FUNCTION_TYPES",
        "EXACT_OPTIONAL_PROPERTY_TYPES",
        "NO_UNCHECKED_INDEXED_ACCESS",
        "NO_ERASE_GENERICS",
        "ALLOW_BIVARIANT_REST",
    ] {
        assert!(
            source.contains(flag),
            "RelationFlags must expose the `{flag}` constant"
        );
    }
}

/// Checker compiler-option packing must stay on the boundary-owned
/// `RelationFlags` wrapper rather than reaching into solver internals.
#[test]
fn test_pack_relation_flags_uses_boundary_relation_flags_surface() {
    let source = fs::read_to_string("src/context/compiler_options.rs")
        .expect("failed to read context/compiler_options.rs");

    assert!(
        source.contains("use crate::query_boundaries::assignability::RelationFlags;"),
        "pack_relation_flags must import boundary-owned RelationFlags"
    );

    for flag in [
        "RelationFlags::STRICT_NULL_CHECKS",
        "RelationFlags::STRICT_FUNCTION_TYPES",
        "RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES",
        "RelationFlags::NO_UNCHECKED_INDEXED_ACCESS",
        "RelationFlags::ALLOW_BIVARIANT_REST",
    ] {
        assert!(
            source.contains(flag),
            "pack_relation_flags must use `{flag}` when encoding checker policy"
        );
    }

    assert!(
        !source.contains("RelationCacheKey::FLAG_STRICT_NULL_CHECKS"),
        "pack_relation_flags must not reach directly into RelationCacheKey bits"
    );
}

/// The `RelationFailure` enum must live in `relation_types.rs` and provide
/// structured variant coverage for the semantic families we're unifying.
#[test]
fn test_relation_failure_covers_semantic_families() {
    let source = fs::read_to_string("src/query_boundaries/relation_types.rs")
        .expect("failed to read query_boundaries/relation_types.rs");

    // Core semantic families that must be represented
    for variant in [
        "MissingProperty",
        "MissingProperties",
        "ExcessProperty",
        "IncompatiblePropertyValue",
        "NoApplicableSignature",
        "TupleArityMismatch",
        "ReturnTypeMismatch",
        "ParameterTypeMismatch",
        "ParameterCountMismatch",
        "PropertyModifierMismatch",
        "WeakUnionViolation",
        "TypeMismatch",
    ] {
        assert!(
            source.contains(variant),
            "RelationFailure must include the `{variant}` variant for semantic coverage"
        );
    }
}

