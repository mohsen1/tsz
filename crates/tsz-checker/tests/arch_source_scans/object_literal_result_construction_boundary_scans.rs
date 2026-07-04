//! Object-literal result construction boundary scans.
//!
//! Object-literal computation owns AST traversal, property collection, spread
//! policy, contextual typing, and display normalization. Final solver result
//! surfaces for object literals and mapped-spread fallbacks belong in
//! `query_boundaries::type_computation::object_literals`.

use std::fs;
use std::path::{Path, PathBuf};

const OBJECT_LITERAL_RESULT_CALLERS: &[&str] = &[
    "src/types/computation/object_literal_support.rs",
    "src/types/computation/object_literal/spread_element.rs",
];
const OBJECT_LITERAL_RESULT_BOUNDARY: &str =
    "src/query_boundaries/type_computation/object_literals.rs";

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
fn object_literal_result_callers_route_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().object(",
        ".factory().object_fresh(",
        ".factory().object_with_index(",
        ".factory().union(",
        ".factory().intersection(",
        ".factory().mapped(",
        "factory.union_preserve_order(",
        "object_fresh_all_properties_context_sensitive(",
        "object_preserve_declaration_order(",
        "ObjectShape {",
        "IndexSignature {",
        "tsz_solver::MappedType {",
        "PropertyInfo::new(",
        "PropertyInfo {",
        "store_display_properties(",
    ];

    let mut violations = Vec::new();
    for caller in OBJECT_LITERAL_RESULT_CALLERS {
        scan_for_patterns(caller, FORBIDDEN_PATTERNS, &mut violations);
    }
    assert!(
        violations.is_empty(),
        "object-literal result callers must route solver construction through \
         query_boundaries::type_computation::object_literals:\n{}",
        violations.join("\n")
    );
}

#[test]
fn object_literal_result_boundary_owns_result_construction_helpers() {
    let source = fs::read_to_string(checker_path(OBJECT_LITERAL_RESULT_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/object_literals.rs");

    assert!(
        source.contains("struct ObjectLiteralIndexedType"),
        "query_boundaries::type_computation::object_literals must own `ObjectLiteralIndexedType`"
    );

    for helper in [
        "spread_object_type",
        "fresh_object_type",
        "indexed_object_type",
        "spread_fallback_index_signature",
        "union_type",
        "intersection_type",
        "mapped_type_with_constraint",
        "mapped_spread_property",
        "mapped_spread_object_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_computation::object_literals must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.object_with_flags_and_symbol(",
        "db.store_display_properties(",
        "ObjectFlags::FRESH_LITERAL",
        "ObjectFlags::PRESERVE_DECLARATION_ORDER",
        "ObjectShape {",
        "IndexSignature {",
        "db.union(",
        "db.union_from_sorted_vec(",
        "db.intersection(",
        "db.mapped(",
        "PropertyInfo::new(",
        "db.object(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_computation::object_literals should own \
             `{construction_pattern}`"
        );
    }
}
