//! Array-literal surface construction boundary scans.
//!
//! Array-literal computation owns syntax traversal, contextual typing, spread
//! policy, tuple forcing, excess-property checks, and diagnostics. Final
//! tuple/array/union solver surfaces belong in
//! `query_boundaries::type_computation::array_literals`.

use std::fs;
use std::path::{Path, PathBuf};

const ARRAY_LITERAL_COMPUTATION: &str = "src/types/computation/array_literal.rs";
const ARRAY_LITERAL_BOUNDARY: &str = "src/query_boundaries/type_computation/array_literals.rs";

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
fn array_literal_callers_route_surface_construction_through_boundary() {
    let source = fs::read_to_string(checker_path(ARRAY_LITERAL_COMPUTATION))
        .expect("failed to read src/types/computation/array_literal.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);

    let mut violations = Vec::new();
    for pattern in [
        "TupleElement {",
        "tsz_solver::TupleElement",
        "factory.tuple(",
        "factory.array(",
        ".factory().tuple(",
        ".factory().array(",
        "self.ctx.types.union(",
        "ctx.types.union(",
    ] {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!("{ARRAY_LITERAL_COMPUTATION} contains `{pattern}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "array-literal callers must route solver construction through \
         query_boundaries::type_computation::array_literals:\n{}",
        violations.join("\n")
    );
}

#[test]
fn array_literal_boundary_owns_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(ARRAY_LITERAL_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/array_literals.rs");

    for helper in [
        "tuple_element",
        "tuple_type",
        "tuple_from_element_types",
        "empty_tuple_type",
        "array_type",
        "any_array_type",
        "never_array_type",
        "error_array_type",
        "element_union",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_computation::array_literals must own `{helper}`"
        );
    }

    for construction_pattern in ["TupleElement {", "db.tuple(", "db.array(", "db.union("] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_computation::array_literals should own \
             `{construction_pattern}`"
        );
    }
}
