//! Type-node annotation surface construction boundary scans.
//!
//! Type-node and type-literal-context resolution own syntax traversal, name
//! lookup, lib fallback, diagnostics, and type-argument validation. Solver
//! construction for annotation unions, tuples, arrays, readonly arrays,
//! applications, lazy bases, `NoInfer`, and unresolved-name display surfaces belongs in
//! `query_boundaries::type_construction`.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_NODE_ANNOTATION_CALLERS: &[&str] = &[
    "src/types/type_literal_checker.rs",
    "src/types/type_node.rs",
    "src/types/type_node_helpers.rs",
];
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
fn type_node_annotation_surfaces_route_solver_construction_through_boundary() {
    let patterns = [
        "union_or_single_literal_reduce(",
        "store_union_origin(",
        "TupleElement {",
        "factory.union(",
        "factory.tuple(",
        "factory.array(",
        "factory.application(",
        "factory.readonly_type(",
        "factory.lazy(",
        "factory.no_infer(",
        ".factory().union(",
        ".factory().tuple(",
        ".factory().array(",
        ".factory().application(",
        ".factory().readonly_type(",
        ".factory().lazy(",
        ".factory().no_infer(",
        "self.ctx.types.union(",
        "self.ctx.types.intersection(",
        "self.ctx.types.array(",
        "self.ctx.types.tuple(",
        "self.ctx.types.application(",
        "self.ctx.types.readonly_type(",
        "self.ctx.types.lazy(",
        "self.ctx.types.no_infer(",
        "self.ctx.types.unresolved_type_name(",
        "ctx.types.union(",
        "ctx.types.intersection(",
        "ctx.types.array(",
        "ctx.types.tuple(",
        "ctx.types.application(",
        "ctx.types.readonly_type(",
        "ctx.types.lazy(",
        "ctx.types.no_infer(",
        "ctx.types.unresolved_type_name(",
    ];

    let mut violations = Vec::new();
    for caller in TYPE_NODE_ANNOTATION_CALLERS {
        scan_for_patterns(caller, &patterns, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "type-node annotation surfaces must route solver construction through \
         query_boundaries::type_construction:\n{}",
        violations.join("\n")
    );
}

#[test]
fn type_construction_boundary_owns_type_node_annotation_helpers() {
    let source = fs::read_to_string(checker_path(TYPE_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/type_construction.rs");

    for helper in [
        "type_node_union",
        "type_node_annotation_union_with_origin",
        "type_node_intersection",
        "type_node_array",
        "type_node_tuple_element",
        "type_node_tuple",
        "type_node_readonly_array",
        "type_node_array_reference",
        "type_node_readonly_any_array",
        "type_node_application",
        "type_node_lazy_type",
        "type_node_no_infer",
        "type_node_lazy_application",
        "type_node_unresolved_type_name",
        "type_node_unresolved_application",
        "type_node_nullable_predicate_union",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_construction must own `{helper}`"
        );
    }

    for construction_pattern in [
        "union_or_single_literal_reduce(",
        "db.store_union_origin(",
        "db.union(",
        "db.intersection(",
        "db.array(",
        "TupleElement {",
        "db.tuple(",
        "db.readonly_type(",
        "db.application(",
        "db.lazy(",
        "db.no_infer(",
        "db.unresolved_type_name(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_construction should own `{construction_pattern}`"
        );
    }
}
