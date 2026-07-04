//! Type-node fallback construction boundary scans.
//!
//! `types/type_node.rs` owns AST traversal and member-order facts. Synthetic
//! solver surfaces for type-node intersections and type-literal function,
//! callable, and object fallbacks belong behind construction query boundaries.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_NODE: &str = "src/types/type_node.rs";
const SIGNATURE_BOUNDARY: &str = "src/query_boundaries/construct_signatures.rs";
const TYPE_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/type_construction.rs";

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
fn type_node_fallback_construction_routes_through_boundaries() {
    let source = fs::read_to_string(checker_path(TYPE_NODE)).expect("failed to read type_node.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);
    let forbidden = [
        "tsz_solver::utils::intersection_or_single(",
        "factory.function(",
        "factory.callable(",
        "factory.object_with_index(",
        "factory.object(",
        ".factory().function(",
        ".factory().callable(",
        ".factory().object_with_index(",
        ".factory().object(",
        "mark_literal_object_annotation(",
        "FunctionShape {",
        "CallableShape {",
        "ObjectShape {",
    ];

    let mut violations = Vec::new();
    for pattern in forbidden {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(pattern);
        }
    }

    assert!(
        violations.is_empty(),
        "type_node.rs must route synthetic intersection, callable/function, \
         and object fallback construction through query boundaries, found: {}",
        violations.join(", ")
    );
}

#[test]
fn type_node_fallback_boundaries_own_construction_helpers() {
    let signatures = fs::read_to_string(checker_path(SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/construct_signatures.rs");
    for helper in [
        "method_function_type_from_call_signature",
        "call_only_callable_type",
        "type_literal_callable_type",
    ] {
        assert!(
            defines_fn(&signatures, helper),
            "query_boundaries::construct_signatures must own `{helper}`"
        );
    }

    let type_construction = fs::read_to_string(checker_path(TYPE_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/type_construction.rs");
    for helper in [
        "type_node_intersection_or_single",
        "type_literal_object",
        "type_literal_object_with_indexes",
    ] {
        assert!(
            defines_fn(&type_construction, helper),
            "query_boundaries::type_construction must own `{helper}`"
        );
    }
}
