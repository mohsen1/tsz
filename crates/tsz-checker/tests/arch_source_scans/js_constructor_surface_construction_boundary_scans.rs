//! Checked-JS constructor surface construction boundary scans.
//!
//! `complex_constructors.rs` and `complex_js_constructor.rs` own checked-JS
//! constructor/prototype evidence discovery, ordering, generic substitution,
//! and diagnostics. Solver construction for shallow callable method surfaces,
//! synthesized public instance properties, and final JS instance objects belongs
//! in `query_boundaries::type_computation::complex`.

use std::fs;
use std::path::{Path, PathBuf};

const COMPLEX_CONSTRUCTORS: &str = "src/types/computation/complex_constructors.rs";
const COMPLEX_JS_CONSTRUCTOR: &str = "src/types/computation/complex_js_constructor.rs";
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

fn scan_source_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
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
fn js_constructor_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    let patterns = [
        "PropertyInfo {",
        "Visibility::Public",
        "CallableShape {",
        ".factory().callable(",
        "factory.callable(",
        ".object_with_symbol(",
        ".object_with_flags_and_symbol(",
    ];

    scan_source_for_patterns(COMPLEX_CONSTRUCTORS, &patterns, &mut violations);
    scan_source_for_patterns(COMPLEX_JS_CONSTRUCTOR, &patterns, &mut violations);

    assert!(
        violations.is_empty(),
        "checked-JS constructor surfaces must route solver construction through \
         query_boundaries::type_computation::complex:\n{}",
        violations.join("\n")
    );
}

#[test]
fn js_constructor_boundary_owns_surface_helpers() {
    let source = fs::read_to_string(checker_path(COMPLEX_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/complex.rs");

    for helper in [
        "shallow_js_method_callable_type",
        "js_surface_property",
        "js_instance_object_with_symbol",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_computation::complex must own `{helper}`"
        );
    }

    for construction_pattern in [
        "CallableShape {",
        "PropertyInfo {",
        "Visibility::Public",
        "db.callable(",
        "db.object_with_flags_and_symbol(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_computation::complex should own `{construction_pattern}`"
        );
    }
}
