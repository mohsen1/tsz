//! Type-node signature/helper surface construction boundary scans.
//!
//! `type_node_signature.rs` and `type_node_helpers.rs` own AST traversal,
//! scope setup, and helper-specific semantic decisions. Raw solver records and
//! type surfaces for parameters, type parameters, optional-parameter unions, and
//! indexed-access helper results belong behind query boundaries.

use std::fs;
use std::path::{Path, PathBuf};

const TYPE_NODE_SIGNATURE: &str = "src/types/type_node_signature.rs";
const TYPE_NODE_HELPERS: &str = "src/types/type_node_helpers.rs";
const SIGNATURE_BUILDING_BOUNDARY: &str = "src/query_boundaries/signature_building.rs";
const TYPE_CONSTRUCTION_BOUNDARY: &str = "src/query_boundaries/type_construction.rs";
const INDEXED_ACCESS_BOUNDARY: &str = "src/query_boundaries/indexed_access_key_space.rs";

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
fn type_node_signature_helpers_route_solver_construction_through_boundaries() {
    let mut violations = Vec::new();
    scan(
        TYPE_NODE_SIGNATURE,
        &[
            "ParamInfo {",
            ".factory().union2(",
            ".types.factory().union2(",
        ],
        &mut violations,
    );
    scan(
        TYPE_NODE_HELPERS,
        &[
            "TypeParamInfo {",
            ".type_param(",
            ".types.index_access(",
            ".factory().index_access(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "type-node signature/helper surfaces must route raw solver construction \
         through query boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn type_node_signature_helper_boundaries_own_surface_helpers() {
    let signature_building = fs::read_to_string(checker_path(SIGNATURE_BUILDING_BOUNDARY))
        .expect("failed to read query_boundaries/signature_building.rs");
    for helper in ["param_info", "user_type_param_info", "user_type_param"] {
        assert!(
            defines_fn(&signature_building, helper),
            "query_boundaries::signature_building must own `{helper}`"
        );
    }

    let type_construction = fs::read_to_string(checker_path(TYPE_CONSTRUCTION_BOUNDARY))
        .expect("failed to read query_boundaries/type_construction.rs");
    assert!(
        defines_fn(&type_construction, "type_node_union"),
        "query_boundaries::type_construction must own `type_node_union`"
    );

    let indexed_access = fs::read_to_string(checker_path(INDEXED_ACCESS_BOUNDARY))
        .expect("failed to read query_boundaries/indexed_access_key_space.rs");
    assert!(
        defines_fn(&indexed_access, "indexed_access_type"),
        "query_boundaries::indexed_access_key_space must own `indexed_access_type`"
    );
}
