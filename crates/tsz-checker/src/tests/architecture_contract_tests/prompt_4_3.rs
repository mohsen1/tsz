use super::*;
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
