//! Type-environment surface construction boundary scans.
//!
//! `state/type_environment` owns AST/global/lib/JS/mapped discovery, cache
//! policy, and ordering decisions. Raw solver record and shape construction for
//! those derived surfaces belongs in `query_boundaries::state::type_environment`.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_ENVIRONMENT_FILES: &[&str] = &[
    "src/state/type_environment/core.rs",
    "src/state/type_environment/lazy.rs",
    "src/state/type_environment/type_node_resolution.rs",
    "src/context/lib_queries.rs",
];
const TYPE_ENVIRONMENT_BOUNDARY: &str = "src/query_boundaries/state/type_environment.rs";

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
fn type_environment_surfaces_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    let patterns = [
        "PropertyInfo {",
        "PropertyInfo::new(",
        "ObjectShape {",
        "CallableShape {",
        "CallSignature {",
        "ParamInfo {",
        "TypeParamInfo::simple(",
        "tsz_solver::TypeParamInfo {",
        ".factory().object_with_index(",
        ".factory().callable(",
        ".factory().global_this_surface_object(",
        ".types.callable(",
        "factory.object(",
        "factory.callable(",
        "factory.object_with_index(",
    ];

    for path in TYPE_ENVIRONMENT_FILES {
        scan_source_for_patterns(path, &patterns, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "type-environment surface construction must route raw solver records \
         and shape interning through query_boundaries::state::type_environment:\n{}",
        violations.join("\n")
    );
}

#[test]
fn state_type_environment_boundary_owns_surface_helpers() {
    let source = fs::read_to_string(checker_path(TYPE_ENVIRONMENT_BOUNDARY))
        .expect("failed to read query_boundaries/state/type_environment.rs");

    for helper in [
        "enum_namespace_member_property",
        "mapped_property",
        "global_this_surface_property",
        "js_expando_property",
        "global_this_surface_object",
        "mapped_result_object",
        "object_with_expando_properties",
        "callable_shape_for_expando_base",
        "callable_with_appended_properties",
        "callable_with_instantiated_signatures",
        "instantiate_type_environment_signatures",
        "unconstrained_type_environment_type_param",
        "provisional_class_expression_type_param",
        "provisional_class_expression_constructor_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::state::type_environment must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo::new(",
        "ObjectShape {",
        "CallableShape {",
        "db.object_with_index(",
        "db.callable(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::state::type_environment should own `{construction_pattern}`"
        );
    }
}
