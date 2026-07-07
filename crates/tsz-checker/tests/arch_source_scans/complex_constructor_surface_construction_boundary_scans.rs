//! Complex constructor surface construction boundary scans.
//!
//! `complex.rs` owns new-expression orchestration, argument collection,
//! diagnostics, and contextual inference policy. Solver construction for
//! evaluated constructor signatures, contextual Promise unions, and evaluated
//! intersection members belongs in
//! `query_boundaries::type_computation::complex`.

use std::fs;
use std::path::{Path, PathBuf};

const COMPLEX_COMPUTATION: &str = "src/types/computation/complex.rs";
const COMPLEX_BOUNDARY: &str = "src/query_boundaries/type_computation/complex.rs";

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
fn complex_constructor_surfaces_route_solver_construction_through_boundary() {
    let source = fs::read_to_string(checker_path(COMPLEX_COMPUTATION))
        .expect("failed to read types/computation/complex.rs");
    let source = production_source_without_comments(&source);
    let compact_source = compact(&source);

    let mut violations = Vec::new();
    for pattern in [
        "tsz_solver::ParamInfo {",
        "tsz_solver::FunctionShape {",
        "ParamInfo {",
        "FunctionShape {",
        ".factory().union(",
        ".factory().union2(",
        ".factory().application(",
        "self.ctx.types.intersection(",
        "store_display_alias(",
    ] {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!("{COMPLEX_COMPUTATION} contains `{pattern}`"));
        }
    }

    assert!(
        violations.is_empty(),
        "complex constructor surfaces must route solver construction through \
         query_boundaries::type_computation::complex:\n{}",
        violations.join("\n")
    );
}

#[test]
fn complex_boundary_owns_constructor_surface_helpers() {
    let source = fs::read_to_string(checker_path(COMPLEX_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/complex.rs");

    for helper in [
        "constructor_shape_with_mapped_parameter_types",
        "constructor_contextual_promise_union",
        "constructor_promise_resolve_value_union",
        "typed_array_length_constructor_return_application",
        "record_explicit_new_display_alias",
        "record_synthetic_explicit_new_display_alias",
        "evaluated_intersection_members",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_computation::complex must own `{helper}`"
        );
    }

    for construction_pattern in [
        "ParamInfo {",
        "FunctionShape {",
        "db.union(",
        "db.union2(",
        "db.application(",
        "db.store_display_alias(",
        "db.intersection(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_computation::complex should own `{construction_pattern}`"
        );
    }
}
