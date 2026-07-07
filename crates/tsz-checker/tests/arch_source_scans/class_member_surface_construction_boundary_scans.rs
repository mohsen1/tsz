//! Class member/in-progress surface construction boundary scans.
//!
//! Class instance and constructor builders own AST traversal, member ordering,
//! cache-window publication, contextual typing, and late-bound detection. Raw
//! solver construction for member properties, method callables, declared index
//! signatures, partial `this`, rough instance snapshots, and temporary
//! constructor/instance surfaces belongs in `query_boundaries::class_type`.

use std::fs;
use std::path::{Path, PathBuf};

const CLASS_INSTANCE: &str = "src/types/class_type/instance.rs";
const CLASS_CONSTRUCTOR: &str = "src/types/class_type/constructor.rs";
const CLASS_TYPE_BOUNDARY: &str = "src/query_boundaries/class_type.rs";

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
fn class_member_surface_callers_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    let patterns = [
        "ParamInfo {",
        "CallSignature {",
        "CallableShape {",
        "ObjectShape {",
        "IndexSignature {",
        "PropertyInfo {",
        "PropertyInfo::new(",
        "factory.callable(",
        "factory.object(",
        "factory.object_with_index(",
        "factory.union2(",
        "factory.intersection(",
        "factory.intersection2(",
    ];

    scan_source_for_patterns(CLASS_INSTANCE, &patterns, &mut violations);
    scan_source_for_patterns(CLASS_CONSTRUCTOR, &patterns, &mut violations);

    assert!(
        violations.is_empty(),
        "class member and in-progress surface construction must route through \
         query_boundaries::class_type:\n{}",
        violations.join("\n")
    );
}

#[test]
fn class_type_boundary_owns_class_member_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(CLASS_TYPE_BOUNDARY))
        .expect("failed to read query_boundaries/class_type.rs");

    for helper in [
        "class_member_property",
        "class_method_callable_type",
        "optional_class_member_type",
        "class_rest_any_param",
        "class_method_call_signature",
        "class_declared_index_signature",
        "class_member_object_type",
        "class_member_object_with_indexes_type",
        "class_member_partial_this_type",
        "rough_class_instance_return_type",
        "partial_static_constructor_callable_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::class_type must own `{helper}`"
        );
    }

    assert!(
        source.contains("struct ClassMemberProperty"),
        "query_boundaries::class_type must own `ClassMemberProperty`"
    );

    for construction_pattern in [
        "ParamInfo {",
        "CallSignature {",
        "CallableShape {",
        "ObjectShape {",
        "IndexSignature {",
        "PropertyInfo {",
        "db.callable(",
        "db.object(",
        "db.object_with_index(",
        "db.union2(",
        "db.intersection(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::class_type should own `{construction_pattern}`"
        );
    }
}
