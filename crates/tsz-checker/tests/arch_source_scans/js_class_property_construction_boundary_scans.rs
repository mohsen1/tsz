//! JS class-property construction boundary scans.
//!
//! JS class-property scanning gathers AST, JSDoc, modifier, and source-order
//! facts. Solver type-parameter, array/union, property, callable, and object
//! construction belongs in `query_boundaries::checkers::class_properties`.

use std::fs;
use std::path::{Path, PathBuf};

const JS_CLASS_PROPERTIES: &str = "src/types/class_type/js_class_properties.rs";
const JS_CLASS_PROPERTY_BOUNDARY: &str = "src/query_boundaries/checkers/class_properties.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }
}

#[test]
fn js_class_property_scanner_routes_solver_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".array(",
        ".type_param(",
        ".union(",
        ".union2(",
        ".callable(",
        ".object(",
        ".object_with_index(",
        "TypeParamInfo {",
        "TypeParamInfo::simple(",
        "TypeParamOrigin::User",
        "PropertyInfo {",
        "PropertyInfo::new(",
        "ParamInfo {",
        "ParamInfo::required(",
        "CallSignature {",
        "CallableShape {",
        "CallableShape::default()",
        "ObjectShape {",
        "ObjectShape::default()",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(JS_CLASS_PROPERTIES, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "JS class-property scanning must route solver shape construction \
         through query_boundaries::checkers::class_properties:\n{}",
        violations.join("\n")
    );
}

#[test]
fn js_class_property_boundary_owns_construction_helpers_and_literals() {
    let source = fs::read_to_string(checker_path(JS_CLASS_PROPERTY_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/class_properties.rs");

    for helper in [
        "js_class_type_param_info",
        "js_class_type_param_type",
        "js_class_array_type",
        "js_class_union_type",
        "js_class_union_pair_type",
        "js_class_property_info",
        "js_class_method_callable_type",
        "js_class_instance_object_type",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::checkers::class_properties must own `{helper}`"
        );
    }

    for shape_pattern in [
        "TypeParamInfo {",
        "TypeParamOrigin::User",
        "PropertyInfo {",
        "ParamInfo {",
        "CallSignature {",
        "CallableShape {",
        "ObjectShape {",
    ] {
        assert!(
            source.contains(shape_pattern),
            "query_boundaries::checkers::class_properties should own `{shape_pattern}`"
        );
    }
}
