//! Class-constructor return construction boundary scans.
//!
//! `constructor_checker.rs` owns mixin detection, AST/control-flow facts,
//! lazy resolution, and diagnostics. Solver construction for constructor
//! return intersections, abstract-flag clearing, construct return rewrites,
//! and static property merges belongs in `query_boundaries::checkers::constructor`.

use std::fs;
use std::path::{Path, PathBuf};

const CONSTRUCTOR_CHECKER: &str = "src/classes/constructor_checker.rs";
const CONSTRUCTOR_BOUNDARY: &str = "src/query_boundaries/checkers/constructor.rs";

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

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn class_constructor_return_construction_routes_through_boundary() {
    let source = fs::read_to_string(checker_path(CONSTRUCTOR_CHECKER))
        .expect("failed to read classes/constructor_checker.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    let forbidden = [
        "ConstructorReturnMergeKind",
        "classify_for_constructor_return_merge(",
        "tsz_solver::utils::intersection_or_single(",
        "intersect_constructor_returns(",
        ".factory().callable(",
        ".factory().function(",
        ".factory().intersection(",
        ".factory().intersection2(",
        "factory.callable(",
        "factory.function(",
        "factory.intersection(",
        "factory.intersection2(",
    ];

    let mut violations = Vec::new();
    for pattern in forbidden {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(pattern);
        }
    }

    assert!(
        violations.is_empty(),
        "constructor_checker.rs must route constructor-return solver \
         construction through query_boundaries::checkers::constructor, found: {}",
        violations.join(", ")
    );
}

#[test]
fn constructor_boundary_owns_return_construction_helpers() {
    let source = fs::read_to_string(checker_path(CONSTRUCTOR_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/constructor.rs");

    for helper in [
        "constructor_return_intersection_or_single",
        "constructor_instance_intersection_or_single",
        "mixin_returned_class_instance_type",
        "mixin_return_type_with_base_constructor",
        "constructor_type_without_abstract_flag",
        "constructor_type_with_construct_return",
        "constructor_type_with_base_instance_return",
        "constructor_type_with_base_properties",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::constructor must own `{helper}`"
        );
    }

    for construction_pattern in [
        "tsz_solver::utils::intersection_or_single(",
        "tsz_solver::type_queries::data::intersect_constructor_returns(",
        "ConstructorReturnMergeKind",
        "db.callable(",
        "db.function(",
        "db.intersection(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::constructor should own `{construction_pattern}`"
        );
    }
}
