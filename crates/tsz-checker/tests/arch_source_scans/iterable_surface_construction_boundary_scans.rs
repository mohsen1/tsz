//! Iterable surface construction boundary scans.
//!
//! Iterable checking owns AST position, diagnostics, protocol orchestration,
//! and ES5 iteration policy. Solver iterator-info queries and iterable element
//! union/intersection surfaces belong in `query_boundaries::checkers::iterable`.

use std::fs;
use std::path::{Path, PathBuf};

const ITERABLE_CHECKER: &str = "src/checkers/iterable_checker.rs";
const ITERABLE_BOUNDARY: &str = "src/query_boundaries/checkers/iterable.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    for pattern in patterns {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn iterable_checker_routes_iterator_and_surface_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "tsz_solver::operations::get_iterator_info",
        "tsz_solver::operations::extract_iterator_result_value_types",
        "tsz_solver::utils::union_or_single",
        "factory.union(",
        "factory.intersection(",
        ".factory().union(",
        ".factory().intersection(",
        "self.ctx.types.factory()",
        "self.ctx.types.union(",
        "self.ctx.types.intersection(",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(ITERABLE_CHECKER, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "iterable checking must route iterator-info and element-surface construction \
         through query_boundaries::checkers::iterable:\n{}",
        violations.join("\n")
    );
}

#[test]
fn iterable_boundary_owns_iterator_and_surface_helpers() {
    let source = fs::read_to_string(checker_path(ITERABLE_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/iterable.rs");

    for helper in [
        "iterator_info_yield_type",
        "iterator_result_value_types",
        "tuple_element_union_type",
        "union_element_type",
        "intersection_element_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::iterable must own `{helper}`"
        );
    }

    for construction_pattern in [
        "tsz_solver::operations::get_iterator_info",
        "tsz_solver::operations::extract_iterator_result_value_types",
        "tsz_solver::utils::union_or_single",
        "db.union(",
        "db.intersection(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::iterable should own `{construction_pattern}`"
        );
    }
}
