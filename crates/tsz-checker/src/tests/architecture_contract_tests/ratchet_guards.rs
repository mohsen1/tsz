use super::*;
// Ratchet guards: prevent architecture debt from growing
// =============================================================================

/// Guard that the `TEMPORARILY_ALLOWED` bypass list in the solver-imports test
/// does not silently grow. When someone wraps a solver API in `query_boundaries`,
/// they should remove it from `TEMPORARILY_ALLOWED`, shrinking the count.
/// Adding new bypasses requires updating this ceiling (which reviewers will see).
///
/// Current ceiling: 0 items — the bypass list is empty.
#[test]
fn test_temporarily_allowed_bypass_list_does_not_grow() {
    // The authoritative list lives in test_solver_imports_go_through_query_boundaries.
    // We cannot inspect it at runtime, so we count the items in source.
    let src = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/tests/architecture_contract_tests/prompt_4_1.rs"),
    )
    .expect("failed to read architecture_contract_tests/prompt_4_1.rs");

    // Find the TEMPORARILY_ALLOWED block and count non-comment, non-empty entries
    let mut in_block = false;
    let mut count = 0usize;
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.contains("const TEMPORARILY_ALLOWED") {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == "];" {
                break;
            }
            // Count lines that are quoted string entries (start with `"`)
            if trimmed.starts_with('"') {
                count += 1;
            }
        }
    }

    const CEILING: usize = 0;
    assert_eq!(
        count, CEILING,
        "TEMPORARILY_ALLOWED bypass list has grown to {count} items (ceiling: {CEILING}). \
         Do not add new solver import bypasses — create a query_boundaries wrapper instead. \
         If a wrapper was created, remove the old entry and lower CEILING in this test."
    );
}

/// Guard that direct type-construction calls (`interner.union()`, `interner.intersection()`,
/// `interner.object()`, `interner.array()`, `interner.tuple()`, `interner.function()`)
/// in checker source files outside `query_boundaries/` and `tests/` do not increase.
///
/// These calls bypass the `query_boundaries` layer and should be migrated to use
/// `flow_analysis::union_types()` or equivalent boundary helpers.
///
/// Current ceiling: 14 occurrences. This number must only decrease over time.
#[test]
fn test_direct_interner_type_construction_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    const CONSTRUCTION_METHODS: &[&str] = &[
        "interner.union(",
        "interner.intersection(",
        "interner.object(",
        "interner.array(",
        "interner.tuple(",
        "interner.function(",
    ];

    let mut violations = Vec::new();
    let mut total_count = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip excluded directories
        if rel.starts_with("tests/") || rel.starts_with("query_boundaries/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for method in CONSTRUCTION_METHODS {
                if line.contains(method) {
                    violations.push(format!("  {}:{}", rel, line_num + 1));
                    total_count += 1;
                }
            }
        }
    }

    // Ceiling: current count of direct interner type-construction calls.
    // This number must only shrink as calls are migrated to query_boundaries.
    const CEILING: usize = 0;
    assert!(
        total_count == CEILING,
        "Direct interner type-construction calls outside query_boundaries have increased \
         to {total_count} (ceiling: {CEILING}). Use query_boundaries helpers \
         (e.g., flow_analysis::union_types, ::array_type, ::tuple_type, ::intersection_types). \
         Current occurrences:\n{}",
        violations.join("\n")
    );
}

/// Guard that `error_reporter/` modules remain a pure diagnostic formatting layer.
/// They must not perform type construction (no `interner.union()`, `interner.object()`, etc.)
/// or type evaluation (no `TypeEvaluator::new()`, `TypeInstantiator::new()`).
///
/// Error reporters should only read type data and format diagnostics.
#[test]
fn test_error_reporter_does_not_perform_type_construction() {
    let error_reporter_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter");
    let mut files = Vec::new();
    walk_rs_files_recursive(&error_reporter_dir, &mut files);

    const FORBIDDEN_PATTERNS: &[(&[&str], &str)] = &[
        (
            &[
                "interner.union(",
                "interner.intersection(",
                "interner.object(",
                "interner.array(",
                "interner.tuple(",
                "interner.function(",
            ],
            "direct type construction via interner",
        ),
        (
            &["TypeEvaluator::new("],
            "type evaluation (should be in checker/query_boundaries)",
        ),
    ];

    let mut violations = Vec::new();
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for (patterns, description) in FORBIDDEN_PATTERNS {
                for pattern in *patterns {
                    if line.contains(pattern) {
                        violations.push(format!("  {}:{} — {}", rel, line_num + 1, description,));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "error_reporter modules must remain a pure formatting layer. \
         The following files contain forbidden patterns:\n{}",
        violations.join("\n")
    );
}

/// Guard that the number of checker source files exceeding ~2000 LOC does not increase.
///
/// Per CLAUDE.md section 12: "Checker files should stay under ~2000 LOC."
/// This ratchet captures the current state (4 files over 2000 lines) and prevents
/// regression. As files are split, this ceiling must be lowered.
///
/// Current ceiling: 4 files over 2000 lines. This number must only decrease over time.
#[test]
fn test_checker_file_size_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut oversized = Vec::new();
    let mut max_lines = 0usize;

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip test files — they are not subject to the LOC guideline
        if rel.starts_with("tests/") || rel.contains("/test") {
            continue;
        }

        let line_count = match fs::read_to_string(path) {
            Ok(s) => s.lines().count(),
            Err(_) => continue,
        };

        if line_count > max_lines {
            max_lines = line_count;
        }

        if line_count > 2000 {
            oversized.push(format!("  {rel} ({line_count} lines)"));
        }
    }

    // Ceiling: number of checker source files exceeding 2000 LOC.
    // This number must only shrink as files are split into smaller modules.
    // Current oversized files (as of 2026-04-03):
    //   checkers/call_checker.rs, checkers/generic_checker.rs,
    //   checkers/jsx/props/mod.rs, checkers/jsx/props/resolution.rs, checkers/jsx/props/validation.rs, checkers/jsx/orchestration.rs,
    //   types/type_checking/duplicate_identifiers.rs, types/function_type.rs,
    //   types/queries/lib.rs, types/utilities/core.rs, types/computation/binary.rs,
    //   types/computation/identifier.rs, types/computation/call/inner.rs,
    //   types/computation/object_literal.rs, types/property_access_helpers.rs,
    //   types/property_access_type.rs, types/class_type/core.rs,
    //   types/class_type/constructor.rs,
    //   classes/class_checker.rs, classes/class_implements_checker.rs,
    //   declarations/import/core.rs, declarations/import/declaration.rs,
    //   state/variable_checking/core.rs,
    //   state/variable_checking/variable_helpers.rs, state/variable_checking/destructuring.rs,
    //   state/type_analysis/computed_commonjs.rs, state/type_analysis/computed.rs,
    //   state/type_resolution/module.rs,
    //   jsdoc/params.rs, jsdoc/resolution.rs, symbols/scope_finder.rs,
    //   assignability/assignment_checker.rs, error_reporter/core.rs,
    //   error_reporter/call_errors.rs, flow/control_flow/core.rs
    const FILE_COUNT_CEILING: usize = 33;
    assert!(
        oversized.len() <= FILE_COUNT_CEILING,
        "Number of checker source files over 2000 LOC has grown to {} (ceiling: {FILE_COUNT_CEILING}). \
         Split oversized files into smaller modules before adding new code. \
         Current oversized files:\n{}",
        oversized.len(),
        oversized.join("\n")
    );

    // Ceiling: maximum line count of any single checker source file.
    // This prevents existing large files from growing further.
    const MAX_LOC_CEILING: usize = 3090;
    assert!(
        max_lines <= MAX_LOC_CEILING,
        "Largest checker source file has grown to {max_lines} lines (ceiling: {MAX_LOC_CEILING}). \
         Split the file into smaller modules. Current oversized files:\n{}",
        oversized.join("\n")
    );
}

/// CLAUDE.md §4: Lowering must not import Checker or Emitter.
/// tsz-lowering is a bridge from AST to solver types; it should only depend on
/// parser, binder, solver, and common. Importing the checker or emitter would
/// create a backwards dependency in the pipeline.
#[test]
fn test_lowering_must_not_import_checker_or_emitter() {
    let lowering_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tsz-lowering/src");
    if !lowering_src.exists() {
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&lowering_src, &mut files);

    let forbidden_crates = ["tsz_checker", "tsz_emitter"];

    let mut violations = Vec::new();
    for path in files {
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for crate_name in &forbidden_crates {
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
        "Lowering must not import Checker or Emitter (CLAUDE.md §4). \
         Lowering bridges AST to solver types; it should not depend on \
         downstream pipeline stages. Violations:\n  {}",
        violations.join("\n  ")
    );
}

/// Guard that CLI and ancillary crates consume checker only through public API paths.
///
/// Per CLAUDE.md section 4: "CLI and ancillary crates must consume checker diagnostics
/// via `tsz_checker::diagnostics`."
///
/// This prevents the CLI from reaching into checker internals (types, state, flow,
/// checkers, symbols, etc.) which would create tight coupling.
#[test]
fn test_cli_must_not_import_checker_internals() {
    let cli_src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tsz-cli/src");
    if !cli_src.exists() {
        // Skip if CLI crate doesn't exist in this workspace layout
        return;
    }

    let mut files = Vec::new();
    walk_rs_files_recursive(&cli_src, &mut files);

    // These are checker-internal module paths that CLI must not import.
    // `tsz_checker::diagnostics` and `tsz_checker::context` are the allowed public API.
    const FORBIDDEN_IMPORTS: &[&str] = &[
        "tsz_checker::types::",
        "tsz_checker::state::",
        "tsz_checker::flow::",
        "tsz_checker::checkers::",
        "tsz_checker::symbols::",
        "tsz_checker::error_reporter::",
        "tsz_checker::declarations::",
    ];

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&cli_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for &forbidden in FORBIDDEN_IMPORTS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "  {}:{} — imports {}",
                        rel,
                        line_num + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "CLI crate must not import checker internals. \
         Use `tsz_checker::diagnostics` for diagnostic codes and types. \
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Guard that cleaned-up checker modules do not regress by re-introducing
/// direct `tsz_solver::type_queries::` calls (both `use` imports AND inline
/// fully-qualified calls).
///
/// ALL checker code outside `query_boundaries/` and `tests/` must use the
/// boundary wrappers in `query_boundaries/common.rs` instead of calling
/// `tsz_solver::type_queries::` directly. This is a blanket zero-tolerance guard.
#[test]
fn test_no_inline_type_queries_in_cleaned_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if trimmed.contains("tsz_solver::type_queries::") {
                violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         tsz_solver::type_queries:: calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Zero-tolerance guard: no direct `tsz_solver::visitor::` calls are allowed outside
/// `query_boundaries/`. All visitor access must go through `query_boundaries::common`.
#[test]
fn test_no_inline_visitor_calls_in_checker_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            if trimmed.contains("tsz_solver::visitor::") {
                violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         tsz_solver::visitor:: calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Zero-tolerance guard: no direct inline calls to `tsz_solver::somefunc(` are allowed
/// outside `query_boundaries/`. All solver function calls must go through boundary wrappers.
///
/// This guard catches top-level solver function calls like `tsz_solver::is_conditional_type(...)`
/// that bypass the query_boundaries layer. Struct/enum paths like `tsz_solver::TypeId` and
/// sub-namespace paths like `tsz_solver::operations::property::` are excluded from this check
/// since they're either data types (handled by `test_solver_imports_go_through_query_boundaries`)
/// or internal solver modules with their own boundary guards.
#[test]
fn test_no_inline_solver_function_calls_in_checker_modules() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // Detect `tsz_solver::lowercase_name(` — a direct solver function call.
            // This pattern matches `tsz_solver::` followed by a lowercase identifier (function)
            // and an opening paren, distinguishing it from type/struct paths.
            let mut rest = trimmed;
            while let Some(pos) = rest.find("tsz_solver::") {
                let after = &rest[pos + "tsz_solver::".len()..];
                // Check if this starts with a lowercase letter (function call, not type)
                if after
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                {
                    // Check that there's no second `::` before a `(` — that would be a submodule
                    // path like `tsz_solver::operations::property::`, not a direct function call.
                    let name_end = after
                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                        .unwrap_or(after.len());
                    let _name = &after[..name_end];
                    let suffix = &after[name_end..];
                    // It's a direct function call if followed immediately by `(`
                    if suffix.starts_with('(') {
                        violations.push(format!("  {}:{} — {}", rel, line_num + 1, trimmed));
                        break;
                    }
                }
                // Advance past this occurrence
                rest = &rest[pos + 1..];
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ALL checker code must use query_boundaries wrappers — no direct \
         inline tsz_solver::funcname( calls allowed outside query_boundaries/.\n\
         Violations found:\n{}",
        violations.join("\n")
    );
}

/// Ratchet guard: direct `tsz_solver::widening::widen_type` (or `operations::widening::`)
/// calls outside `query_boundaries/`, `tests/`, and `types/utilities/core.rs` must not grow.
///
/// Callers should use `query_boundaries::common::widen_type` (free function) or
/// `self.widen_literal_type()` (method on `CheckerState`) instead.
///
/// Current ceiling: 0 occurrences — all calls migrated to query_boundaries.
#[test]
fn test_direct_widening_calls_ceiling() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut count = 0usize;
    let mut locations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Skip allowed locations
        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("tsz_solver::widening::widen_type")
                || line.contains("tsz_solver::operations::widening::widen_type")
            {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "Direct tsz_solver::widening::widen_type calls have grown to {count} (ceiling: {CEILING}). \
         Use query_boundaries::common::widen_type or self.widen_literal_type() instead.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `expression_ops::` calls outside `query_boundaries/` and `tests/`.
///
/// Expression operation calls should go through `query_boundaries::type_computation::core`
/// wrappers to maintain the boundary layer.
#[test]
fn test_no_direct_expression_ops_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("expression_ops::") && line.contains("tsz_solver") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::expression_ops:: calls found outside query_boundaries/. \
         Use query_boundaries::type_computation::core wrappers instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: no direct `ApplicationEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Application evaluation should go through boundary wrappers like
/// `query_boundaries::flow_analysis::evaluate_application_type`.
#[test]
fn test_no_direct_application_evaluator_outside_query_boundaries() {
    let checker_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files_recursive(&checker_src, &mut files);

    let mut violations = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&checker_src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if rel.starts_with("query_boundaries/") || rel.starts_with("tests/") {
            continue;
        }
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for (line_num, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("ApplicationEvaluator::new(") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct ApplicationEvaluator::new() calls found outside query_boundaries/. \
         Use query_boundaries::flow_analysis::evaluate_application_type instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: `context/def_mapping.rs` and context/speculation.rs must not cross-reference
/// each other. `def_mapping` owns SymbolId<->DefId identity mapping, speculation owns
/// checker state transaction boundaries. Mixing these concerns would violate the
/// clean context module separation (BOUNDARIES.md §4 Identity Boundary).
#[test]
fn test_def_mapping_and_speculation_do_not_cross_reference() {
    let def_mapping_src = fs::read_to_string("src/context/def_mapping.rs")
        .expect("failed to read src/context/def_mapping.rs");
    let speculation_src = fs::read_to_string("src/context/speculation.rs")
        .expect("failed to read src/context/speculation.rs");

    // def_mapping must not reference speculation types or functions
    assert!(
        !def_mapping_src.contains("DiagnosticSnapshot")
            && !def_mapping_src.contains("FullSnapshot")
            && !def_mapping_src.contains("ReturnTypeSnapshot")
            && !def_mapping_src.contains("rollback_")
            && !def_mapping_src.contains("snapshot_"),
        "def_mapping.rs must not reference speculation types or functions — \
         keep identity mapping separate from transaction boundaries"
    );

    // speculation must not reference def_mapping types or functions
    assert!(
        !speculation_src.contains("get_or_create_def_id")
            && !speculation_src.contains("def_mapping")
            && !speculation_src.contains("DefinitionStore")
            && !speculation_src.contains("DefinitionInfo"),
        "speculation.rs must not reference def_mapping types or functions — \
         keep transaction boundaries separate from identity mapping"
    );

    // Neither should perform type computation
    assert!(
        !def_mapping_src.contains("is_subtype_of") && !def_mapping_src.contains("is_assignable"),
        "def_mapping.rs must not perform type computation — it is pure identity mapping"
    );
    assert!(
        !speculation_src.contains("is_subtype_of") && !speculation_src.contains("is_assignable"),
        "speculation.rs must not perform type computation — it is pure state management"
    );
}

// =============================================================================
