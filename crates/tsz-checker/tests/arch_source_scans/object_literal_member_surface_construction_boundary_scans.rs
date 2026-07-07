//! Object-literal member surface construction boundary scans.
//!
//! Object-literal computation owns AST traversal, contextual policy, duplicate
//! checks, member order, spelling flags, and synthetic-this orchestration. Raw
//! member properties, synthetic method parameters, and descriptor function
//! surfaces belong behind query boundaries once those facts are known.

use std::fs;
use std::path::{Path, PathBuf};

const COMPUTATION: &str = "src/types/computation/object_literal/computation.rs";
const ACCESSOR_ELEMENT: &str = "src/types/computation/object_literal/accessor_element.rs";
const CIRCULARITY: &str = "src/types/computation/object_literal_circularity.rs";
const OBJECT_LITERAL_MOD: &str = "src/types/computation/object_literal/mod.rs";
const OBJECT_LITERAL_BOUNDARY: &str = "src/query_boundaries/type_computation/object_literals.rs";
const OBJECT_CONTEXT_BOUNDARY: &str = "src/query_boundaries/object_literal_context.rs";
const SIGNATURE_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";
const CONSTRUCT_SIGNATURES_BOUNDARY: &str = "src/query_boundaries/construct_signatures.rs";
const INDEX_SIGNATURE_BOUNDARY: &str = "src/query_boundaries/index_signature.rs";

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
fn object_literal_member_surfaces_route_solver_construction_through_boundaries() {
    let mut violations = Vec::new();
    scan(
        COMPUTATION,
        &[
            "PropertyInfo {",
            "Visibility::Public",
            "use tsz_solver::{IndexSignature",
        ],
        &mut violations,
    );
    scan(
        ACCESSOR_ELEMENT,
        &["PropertyInfo {", "Visibility::Public"],
        &mut violations,
    );
    scan(
        CIRCULARITY,
        &[
            "PropertyInfo {",
            "tsz_solver::PropertyInfo {",
            "ParamInfo {",
            "tsz_solver::ParamInfo {",
            "Visibility::Public",
        ],
        &mut violations,
    );
    scan(
        OBJECT_LITERAL_MOD,
        &[
            "FunctionShape {",
            "FunctionShape::new(",
            "ParamInfo::unnamed(",
            "tsz_solver::ParamInfo",
            ".factory().function(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "object-literal member surfaces must route raw solver construction \
         through query boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn object_literal_member_surface_boundaries_own_helpers() {
    let object_literals = fs::read_to_string(checker_path(OBJECT_LITERAL_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/object_literals.rs");
    assert!(
        object_literals.contains("struct ObjectLiteralMemberProperty"),
        "query_boundaries::type_computation::object_literals must own `ObjectLiteralMemberProperty`"
    );
    assert!(
        defines_fn(&object_literals, "object_literal_member_property"),
        "query_boundaries::type_computation::object_literals must own `object_literal_member_property`"
    );

    let object_context = fs::read_to_string(checker_path(OBJECT_CONTEXT_BOUNDARY))
        .expect("failed to read query_boundaries/object_literal_context.rs");
    assert!(
        defines_fn(&object_context, "synthetic_this_property"),
        "query_boundaries::object_literal_context must own `synthetic_this_property`"
    );

    let signature_building = fs::read_to_string(checker_path(SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    assert!(
        defines_fn(&signature_building, "param_info"),
        "query_boundaries::signature_building must own `param_info`"
    );

    let construct_signatures = fs::read_to_string(checker_path(CONSTRUCT_SIGNATURES_BOUNDARY))
        .expect("failed to read query_boundaries/construct_signatures.rs");
    assert!(
        defines_fn(&construct_signatures, "function_type_from_parts"),
        "query_boundaries::construct_signatures must own `function_type_from_parts`"
    );

    let index_signature = fs::read_to_string(checker_path(INDEX_SIGNATURE_BOUNDARY))
        .expect("failed to read query_boundaries/index_signature.rs");
    assert!(
        index_signature.contains("pub(crate) use tsz_solver::IndexSignature"),
        "query_boundaries::index_signature must re-export `IndexSignature`"
    );
}
