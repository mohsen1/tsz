//! Interface/type-literal own surface construction boundary scans.
//!
//! `get_type_of_interface` and both type-literal resolvers own AST traversal,
//! diagnostics, ordering, and member classification. Solver construction for
//! declared call signatures, properties, indexes, functions, callables, and
//! object surfaces belongs in `query_boundaries::type_construction`,
//! `query_boundaries::signature_building`, or
//! `query_boundaries::construct_signatures`.
//!
//! This scan intentionally excludes interface heritage/merge reconstruction in
//! `interface_type.rs`; that is a separate, higher-risk surface.

use std::fs;
use std::path::{Path, PathBuf};

const INTERFACE_TYPE: &str = "src/types/interface_type.rs";
const TYPE_NODE: &str = "src/types/type_node.rs";
const TYPE_LITERAL_CHECKER: &str = "src/types/type_literal_checker.rs";
const TYPE_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/type_construction.rs";
const SIGNATURE_BUILDING_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";
const CONSTRUCT_SIGNATURES_BOUNDARY: &str = "src/query_boundaries/construct_signatures.rs";

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

fn slice_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"));
    let rest = &source[start_idx..];
    let end_idx = rest
        .find(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"));
    &rest[..end_idx]
}

fn scan_source_for_patterns(source: &str, label: &str, patterns: &[&str]) -> Vec<String> {
    let source = production_source_without_comments(source);
    let compact_source = compact(&source);
    patterns
        .iter()
        .filter(|pattern| source.contains(**pattern) || compact_source.contains(&compact(pattern)))
        .map(|pattern| format!("{label} contains `{pattern}`"))
        .collect()
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn interface_and_type_literal_own_surfaces_route_solver_construction_through_boundaries() {
    let interface_source =
        fs::read_to_string(checker_path(INTERFACE_TYPE)).expect("failed to read interface_type.rs");
    let type_node_source =
        fs::read_to_string(checker_path(TYPE_NODE)).expect("failed to read type_node.rs");
    let type_literal_source = fs::read_to_string(checker_path(TYPE_LITERAL_CHECKER))
        .expect("failed to read type_literal_checker.rs");

    let interface_surface = slice_between(
        &interface_source,
        "pub(crate) fn get_type_of_interface",
        "pub(crate) fn merge_interface_heritage_types",
    );
    let type_literal_surface = slice_between(
        &type_literal_source,
        "pub(crate) fn get_type_from_type_literal",
        "\n    }\n}",
    );
    let type_node_literal_surface = slice_between(
        &type_node_source,
        "fn get_type_from_type_literal",
        "/// Resolve a type symbol from a node index.",
    );

    let patterns = [
        "CallSignature {",
        "PropertyInfo {",
        "Visibility::Public",
        "IndexSignature {",
        "FunctionShape {",
        "CallableShape {",
        "ObjectShape {",
        "factory.function(",
        "factory.callable(",
        "factory.object_with_index(",
        "factory.object_with_symbol(",
        "factory.object_with_late_bound_members(",
        "intersect_types_raw2(",
    ];

    let mut violations =
        scan_source_for_patterns(interface_surface, "get_type_of_interface", &patterns);
    violations.extend(scan_source_for_patterns(
        type_literal_surface,
        "get_type_from_type_literal",
        &patterns,
    ));
    violations.extend(scan_source_for_patterns(
        type_node_literal_surface,
        "TypeNodeChecker::get_type_from_type_literal",
        &patterns,
    ));

    assert!(
        violations.is_empty(),
        "interface/type-literal own surfaces must route solver construction \
         through query boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn interface_and_type_literal_boundaries_own_surface_helpers() {
    let type_construction = fs::read_to_string(checker_path(TYPE_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/type_construction.rs");
    let signature_building = fs::read_to_string(checker_path(SIGNATURE_BUILDING_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    let construct_signatures = fs::read_to_string(checker_path(CONSTRUCT_SIGNATURES_BOUNDARY))
        .expect("failed to read query_boundaries/construct_signatures.rs");

    for helper in [
        "declared_surface_property",
        "declared_index_signature",
        "declared_object_with_symbol",
        "declared_object_with_indexes",
        "type_literal_object_with_late_bound",
        "type_literal_object_with_indexes_and_late_bound",
        "type_literal_number_index_member",
        "raw_intersection_pair",
    ] {
        assert!(
            defines_fn(&type_construction, helper),
            "query_boundaries::type_construction must own `{helper}`"
        );
    }

    assert!(
        defines_fn(&signature_building, "call_signature"),
        "query_boundaries::signature_building must own `call_signature`"
    );

    for helper in [
        "declared_method_function_type",
        "declared_callable_surface_type",
    ] {
        assert!(
            defines_fn(&construct_signatures, helper),
            "query_boundaries::construct_signatures must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo {",
        "Visibility::Public",
        "IndexSignature {",
        "ObjectShape {",
        "db.object_with_flags_and_symbol(",
        "db.object_with_index(",
        "db.intersect_types_raw2(",
    ] {
        assert!(
            type_construction.contains(construction_pattern),
            "query_boundaries::type_construction should own `{construction_pattern}`"
        );
    }

    assert!(
        signature_building.contains("CallSignature {"),
        "query_boundaries::signature_building should own `CallSignature {{`"
    );

    for construction_pattern in ["FunctionShape {", "CallableShape {", "db.callable("] {
        assert!(
            construct_signatures.contains(construction_pattern),
            "query_boundaries::construct_signatures should own `{construction_pattern}`"
        );
    }
}
