/// Ceiling: direct `BinaryOpEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. A wrapper in
/// `query_boundaries/type_computation/core.rs` exists for `evaluate_plus_chain`;
/// more wrappers should be added over time. This ceiling must only decrease.
///
/// Current ceiling: 21 occurrences.
#[test]
fn test_direct_binary_op_evaluator_construction_ceiling() {
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
            if line.contains("BinaryOpEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "BinaryOpEvaluator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Use query_boundaries::common::new_binary_op_evaluator() instead.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `PropertyAccessEvaluator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// These bypass the query boundary layer. Wrappers should be created in
/// `query_boundaries/` over time. This ceiling must only decrease.
///
/// Current ceiling: 0 occurrences (all migrated to `query_boundaries`).
#[test]
fn test_direct_property_access_evaluator_construction_ceiling() {
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
            if line.contains("PropertyAccessEvaluator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        count == 0,
        "PropertyAccessEvaluator::new() must not be used outside query_boundaries/. \
         Use query_boundaries::property_access::resolve_property_access instead. \
         Found {count} violations:\n{}",
        locations.join("\n")
    );
}

/// Ceiling: direct `TypeInstantiator::new()` calls outside `query_boundaries/` and `tests/`.
///
/// Type instantiation should flow through `query_boundaries/common::instantiate_type`
/// or dedicated boundary helpers. This ceiling must only decrease.
///
/// Current ceiling: 1 occurrence (types/queries/lib.rs).
#[test]
fn test_direct_type_instantiator_construction_ceiling() {
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
            if line.contains("TypeInstantiator::new(") {
                count += 1;
                locations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    const CEILING: usize = 0;
    assert!(
        count == CEILING,
        "TypeInstantiator::new() usage ceiling exceeded: found {count} (ceiling: {CEILING}). \
         Use query_boundaries/common::instantiate_type or create a new boundary wrapper.\n\
         Locations:\n{}",
        locations.join("\n")
    );
}

/// Guard: no direct `tsz_solver::relations::freshness::` calls outside
/// `query_boundaries/` and `tests/`.
///
/// Freshness queries (`is_fresh_object_type`, `widen_freshness`) have wrappers
/// in `query_boundaries/common.rs`. All checker code must use those wrappers
/// to maintain the boundary between checker (WHERE) and solver (WHAT).
#[test]
fn test_no_direct_freshness_calls_outside_query_boundaries() {
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
            if line.contains("tsz_solver::relations::freshness") {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct tsz_solver::relations::freshness:: calls found outside query_boundaries/. \
         Use query_boundaries::common::is_fresh_object_type / widen_freshness instead.\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

/// Guard: core.rs must NOT contain inline type-param priming loops.
///
/// The ad hoc block that manually iterated symbol declarations to extract
/// type parameters was replaced by `ensure_def_ready_for_lowering`. This
/// test ensures the inline pattern doesn't regrow.
#[test]
fn test_core_type_resolution_uses_stable_identity_helper_for_type_param_priming() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    // The old ad hoc pattern iterated declarations with get_interface + get_type_alias
    // inline to extract type parameters. This should now go through
    // ensure_def_ready_for_lowering which delegates to
    // extract_declared_type_params_for_reference_symbol.
    let has_inline_iface_param_extraction = src
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .any(|line| {
            line.contains("get_interface(node)")
                && !line.contains("ensure_def")
                && !line.contains("extract_declared")
        });

    assert!(
        !has_inline_iface_param_extraction,
        "core.rs contains inline interface type-param extraction. \
         Use ensure_def_ready_for_lowering (which delegates to \
         extract_declared_type_params_for_reference_symbol) instead."
    );
}

/// Guard: core.rs type reference resolution must delegate to
/// `ensure_def_ready_for_lowering` for generic ref type-param priming.
#[test]
fn test_core_type_resolution_has_ensure_def_ready_call() {
    let src = fs::read_to_string("src/state/type_resolution/core.rs")
        .expect("failed to read src/state/type_resolution/core.rs");

    assert!(
        src.contains("ensure_def_ready_for_lowering"),
        "core.rs must call ensure_def_ready_for_lowering for generic type \
         reference resolution. This is the stable-identity helper that \
        replaces ad hoc type-param priming blocks."
    );
}

/// Guard: `instanceof` narrowing for class and global-constructor symbols must
/// use real `DefId`-backed lazy types rather than raw SymbolId-shaped
/// `reference(SymbolRef)` fallback.
#[test]
fn test_instanceof_constructor_branches_avoid_raw_symbol_reference_fallback() {
    let source = fs::read_to_string("src/flow/control_flow/narrowing.rs")
        .expect("failed to read src/flow/control_flow/narrowing.rs");
    let class_branch = source
        .split("if symbol.has_any_flags(symbol_flags::CLASS)")
        .nth(1)
        .and_then(|rest| rest.split("// Global constructor variables").next())
        .expect("failed to isolate instanceof class-symbol branch");

    assert!(
        class_branch.contains("self.resolve_symbol_to_lazy(symbol_ref)"),
        "instanceof class-symbol branch should resolve through the DefId-backed lazy helper"
    );
    assert!(
        !class_branch.contains(".reference("),
        "instanceof class-symbol branch must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );

    let global_constructor_branch = source
        .split("// Global constructor variables")
        .nth(1)
        .and_then(|rest| rest.split("// For FUNCTION symbols").next())
        .expect("failed to isolate instanceof global-constructor branch");

    assert!(
        global_constructor_branch.contains("self.resolve_symbol_to_lazy(symbol_ref)"),
        "instanceof global-constructor branch should resolve through the DefId-backed lazy helper"
    );
    assert!(
        !global_constructor_branch.contains(".reference("),
        "instanceof global-constructor branch must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );
}

/// Guard: the manual `ArrayBuffer.isView` fallback must use real `DefId`-backed
/// lazy types rather than raw SymbolId-shaped `reference(SymbolRef)` fallback.
#[test]
fn test_array_buffer_is_view_avoids_raw_symbol_reference_fallback() {
    let source = fs::read_to_string("src/flow/control_flow/type_guards.rs")
        .expect("failed to read src/flow/control_flow/type_guards.rs");
    let branch = source
        .split("if type_id.is_none()")
        .nth(1)
        .and_then(|rest| rest.split("let type_id = type_id?;").next())
        .expect("failed to isolate ArrayBuffer.isView manual fallback branch");

    assert!(
        branch.contains("self.resolve_symbol_to_lazy(symbol_ref)?"),
        "ArrayBuffer.isView fallback should resolve ArrayBufferView through the DefId-backed lazy helper"
    );
    assert!(
        branch.contains("self.resolve_symbol_to_lazy(array_buffer_like_ref)?"),
        "ArrayBuffer.isView fallback should resolve ArrayBufferLike through the DefId-backed lazy helper"
    );
    assert!(
        !branch.contains(".reference("),
        "ArrayBuffer.isView fallback must not create Lazy(DefId(symbol_id)) via raw SymbolRef fallback"
    );
}

/// Guard: checker code must not add new raw `reference(SymbolRef)` fallback
/// construction. New checker code should resolve symbols through stable
/// `DefId` helpers before creating `Lazy(DefId)`.
#[test]
fn test_checker_raw_symbol_reference_construction_budget() {
    fn allowed_raw_reference_constructions(_rel_path: &str) -> usize {
        0
    }

    fn is_raw_reference_construction(line: &str) -> bool {
        let trimmed = line.trim_start();
        !trimmed.starts_with("//") && line.contains(".reference(")
    }

    let mut files = Vec::new();
    collect_checker_rs_files_recursive(Path::new("src"), &mut files);

    let mut violations = Vec::new();
    for path in files {
        if path
            .components()
            .any(|component| component.as_os_str() == "tests")
        {
            continue;
        }

        let rel_path = path.display().to_string();
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("failed to read {}", path.display()));
        let count = source
            .lines()
            .filter(|line| is_raw_reference_construction(line))
            .count();
        let allowed = allowed_raw_reference_constructions(&rel_path);

        if count > allowed {
            violations.push(format!(
                "{rel_path}: {count} raw .reference() calls (allowed {allowed})"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "new raw SymbolRef-backed reference construction found in checker code. \
         Resolve symbols through TypeEnvironment/DefId helpers before creating \
         Lazy(DefId), or migrate one of the existing allowlisted fallbacks first:\n{}",
        violations.join("\n")
    );
}

/// Guard: `reference_helpers.rs` must expose `ensure_def_ready_for_lowering`.
///
/// This helper consolidates the DefId + type-param + body priming pattern.
#[test]
fn test_reference_helpers_expose_stable_identity_helper() {
    let src = fs::read_to_string("src/state/type_resolution/reference_helpers.rs")
        .expect("failed to read src/state/type_resolution/reference_helpers.rs");

    assert!(
        src.contains("fn ensure_def_ready_for_lowering"),
        "reference_helpers.rs must expose ensure_def_ready_for_lowering — \
         the stable-identity helper for DefId + type-param + body priming."
    );
}
