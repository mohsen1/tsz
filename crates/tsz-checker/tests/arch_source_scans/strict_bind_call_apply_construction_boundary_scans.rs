//! Strict bind/call/apply construction boundary scans.
//!
//! Property-access helpers own receiver lookup, `this` collapse, and method
//! selection. Solver parameter, type-parameter, tuple, signature, function, and
//! callable construction belongs in `query_boundaries::property_access`.

use std::fs;
use std::path::{Path, PathBuf};

const ACCESS_SEMANTICS: &str = "src/types/property_access_helpers/access_semantics.rs";
const PROPERTY_ACCESS_BOUNDARY: &str = "src/query_boundaries/property_access.rs";

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
fn strict_bind_call_apply_routes_solver_construction_through_property_access_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".type_param(",
        ".tuple(",
        ".function(",
        ".callable(",
        "TypeParamInfo {",
        "TypeParamOrigin::User",
        "TupleElement {",
        "ParamInfo {",
        "CallSignature {",
        "FunctionShape {",
        "CallableShape {",
        "FunctionShape::new(",
        "CallableShape::default()",
        "intern_string(\"thisArg\")",
        "intern_string(\"args\")",
        "intern_string(\"TThis\")",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(ACCESS_SEMANTICS, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "strict bind/call/apply synthesis must route solver shape construction \
         through query_boundaries::property_access:\n{}",
        violations.join("\n")
    );
}

#[test]
fn property_access_boundary_owns_strict_bind_call_apply_construction_helpers() {
    let source = fs::read_to_string(checker_path(PROPERTY_ACCESS_BOUNDARY))
        .expect("failed to read query_boundaries/property_access.rs");

    for helper in [
        "strict_bind_call_apply_param_with_type",
        "strict_bind_call_apply_type_param_with_constraint",
        "strict_bind_call_apply_call_signature",
        "strict_bind_call_apply_signature_from_function_shape",
        "strict_bind_call_apply_params_tuple_type",
        "strict_bind_call_apply_bound_return_type",
        "strict_bind_call_apply_call_only_callable_type",
        "strict_bind_call_apply_this_arg_param",
        "strict_bind_call_apply_args_param",
        "strict_bind_call_apply_generic_this_param",
        "strict_bind_call_apply_method_type",
    ] {
        assert!(
            source.contains(&format!("fn {helper}(")),
            "query_boundaries::property_access must own `{helper}`"
        );
    }

    for shape_pattern in [
        "db.tuple(",
        "db.function(",
        "db.callable(",
        "db.type_param(",
        "TypeParamInfo {",
        "TypeParamOrigin::User",
        "TupleElement {",
        "ParamInfo {",
        "CallSignature {",
        "FunctionShape {",
        "CallableShape {",
    ] {
        assert!(
            source.contains(shape_pattern),
            "query_boundaries::property_access should own `{shape_pattern}`"
        );
    }
}
