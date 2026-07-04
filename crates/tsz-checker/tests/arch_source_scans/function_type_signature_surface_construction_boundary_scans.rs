//! Function-type signature/return surface construction boundary scans.
//!
//! `function_type.rs` and `function_type_helpers.rs` own AST traversal, scope
//! updates, diagnostics, JSDoc parsing, contextual typing policy, and lib-name
//! lookup. Raw solver records and function signature/return type surfaces
//! belong behind query boundaries once those facts are known.

use std::fs;
use std::path::{Path, PathBuf};

const FUNCTION_TYPE: &str = "src/types/function_type.rs";
const FUNCTION_TYPE_HELPERS: &str = "src/types/function_type_helpers.rs";
const SIGNATURE_BUILDING_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";
const CONSTRUCT_SIGNATURES_BOUNDARY: &str = "src/query_boundaries/construct_signatures.rs";
const FUNCTION_RETURNS_BOUNDARY: &str = "src/query_boundaries/function_returns.rs";

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

fn scan(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
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

#[test]
fn function_type_surfaces_route_solver_construction_through_boundaries() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "TypeParamInfo {",
        "ParamInfo {",
        "TypePredicate {",
        "FunctionShape {",
        ".factory().type_param(",
        ".factory().function(",
        ".factory().union(",
        ".factory().union2(",
        ".factory().array(",
        ".factory().lazy(",
        ".factory().application(",
    ];

    let mut violations = Vec::new();
    scan(FUNCTION_TYPE, FORBIDDEN_PATTERNS, &mut violations);
    scan(FUNCTION_TYPE_HELPERS, FORBIDDEN_PATTERNS, &mut violations);

    assert!(
        violations.is_empty(),
        "function type signature/return surfaces must route raw solver \
         construction through query boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn function_type_surface_boundaries_own_helpers() {
    let signature_building = fs::read_to_string(checker_path(SIGNATURE_BUILDING_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    for helper in [
        "user_type_param_info",
        "user_type_param",
        "type_param",
        "param_array_type",
        "optional_param_type_with_undefined",
        "param_info",
        "call_signature",
        "type_predicate",
    ] {
        assert!(
            defines_fn(&signature_building, helper),
            "query_boundaries::signature_building must own `{helper}`"
        );
    }

    let construct_signatures = fs::read_to_string(checker_path(CONSTRUCT_SIGNATURES_BOUNDARY))
        .expect("failed to read query_boundaries/construct_signatures.rs");
    assert!(
        defines_fn(&construct_signatures, "function_type_from_call_signature"),
        "query_boundaries::construct_signatures must own `function_type_from_call_signature`"
    );

    let function_returns = fs::read_to_string(checker_path(FUNCTION_RETURNS_BOUNDARY))
        .expect("failed to read query_boundaries/function_returns.rs");
    for helper in [
        "function_return_union",
        "function_return_lazy_type",
        "function_return_application",
    ] {
        assert!(
            defines_fn(&function_returns, helper),
            "query_boundaries::function_returns must own `{helper}`"
        );
    }
}
