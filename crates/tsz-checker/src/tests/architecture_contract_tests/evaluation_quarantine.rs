use super::*;
// Boundary Quarantine Tests — Evaluator/Checker Construction Ceilings
// =============================================================================

/// Guard: no `CompatChecker::new()` or `CompatChecker::with_resolver()` outside
/// `query_boundaries/` and `tests/`.
///
/// `CompatChecker` is the solver's Lawyer layer. Checker code should never construct
/// it directly — the relation should flow through `query_boundaries/assignability`
/// via `execute_relation()` and related helpers (CLAUDE.md §5, §22).
#[test]
fn test_no_direct_compat_checker_construction_outside_query_boundaries() {
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
            if line.contains("CompatChecker::new(")
                || line.contains("CompatChecker::with_resolver(")
            {
                violations.push(format!("  {}:{}", rel, line_num + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Direct CompatChecker construction found outside query_boundaries/. \
         Route relation checks through query_boundaries/assignability instead (CLAUDE.md §5, §22).\n\
         Violations:\n{}",
        violations.join("\n")
    );
}

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
/// Current ceiling: 0 occurrences (all migrated to query_boundaries).
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

// =============================================================================
