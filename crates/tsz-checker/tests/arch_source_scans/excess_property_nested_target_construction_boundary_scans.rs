//! Excess-property nested target construction boundary scans.
//!
//! The property checker owns AST/source-order facts and diagnostic control flow.
//! Construction for nested excess-property intersections, raw annotation
//! intersections, and optional recursive nested-target unions belongs in
//! `query_boundaries::state::checking`.

use std::fs;
use std::path::{Path, PathBuf};

const PROPERTY_CHECKER: &str = "src/state/state_checking/property.rs";
const EXCESS_PROPERTY_TAIL: &str = "src/state/state_checking/property/excess_property_tail.rs";
const STATE_CHECKING_BOUNDARY: &str = "src/query_boundaries/state/checking.rs";

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
fn excess_property_nested_target_construction_routes_through_boundary() {
    let property_source = fs::read_to_string(checker_path(PROPERTY_CHECKER))
        .expect("failed to read state_checking/property.rs");
    let tail_source = fs::read_to_string(checker_path(EXCESS_PROPERTY_TAIL))
        .expect("failed to read state_checking/property/excess_property_tail.rs");
    let source = production_source_without_comments(&format!("{property_source}\n{tail_source}"));
    let compact_source = compact(&source);
    let forbidden = [
        "tsz_solver::utils::intersection_or_single(",
        ".intersect_types_raw2(",
        "self.ctx.types.union(vec![",
        ".ctx.types.union(vec![",
        "raw_intersection_or_single(",
    ];

    let mut violations = Vec::new();
    for pattern in forbidden {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(pattern);
        }
    }

    assert!(
        violations.is_empty(),
        "property excess-checking must route nested target construction through \
         query_boundaries::state::checking, found: {}",
        violations.join(", ")
    );
}

#[test]
fn state_checking_boundary_owns_excess_property_nested_target_helpers() {
    let source = fs::read_to_string(checker_path(STATE_CHECKING_BOUNDARY))
        .expect("failed to read query_boundaries/state/checking.rs");

    for helper in [
        "excess_property_nested_target_intersection_or_single",
        "excess_property_annotation_intersection_or_single",
        "optional_excess_property_nested_target",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::state::checking must own `{helper}`"
        );
    }

    for construction_pattern in [
        "tsz_solver::utils::intersection_or_single(",
        "db.intersect_types_raw2(",
        "db.union(vec![target, TypeId::UNDEFINED])",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::state::checking should own `{construction_pattern}`"
        );
    }
}
