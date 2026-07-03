//! Property-access result construction boundary scans.
//!
//! State checking owns receiver resolution, property lookup policy, and
//! diagnostics. Solver construction for optional, union, and intersection
//! property-access result surfaces belongs in `query_boundaries::property_access`.

use std::fs;
use std::path::{Path, PathBuf};

const PROPERTY_ACCESS_STATE: &str = "src/state/state_checking/property_access.rs";
const PROPERTY_ACCESS_BOUNDARY: &str = "src/query_boundaries/property_access.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_slice_for_patterns(
    relative: &str,
    start_marker: &str,
    end_marker: &str,
    patterns: &[&str],
    violations: &mut Vec<String>,
) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("failed to find start marker `{start_marker}` in {relative}"));
    let after_start = &source[start..];
    let end = after_start
        .find(end_marker)
        .unwrap_or_else(|| panic!("failed to find end marker `{end_marker}` in {relative}"));
    let line_offset = source[..start].lines().count();

    for (line_index, line) in after_start[..end].lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_offset + line_index + 1
                ));
            }
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn property_access_results_route_construction_through_boundary() {
    let mut violations = Vec::new();
    let forbidden = [
        ".factory().union(",
        ".factory().union2(",
        ".factory().intersection(",
        ".types.union(",
        ".types.union2(",
        ".types.intersection(",
        "tsz_solver::utils::union_or_single(",
        "tsz_solver::utils::intersection_or_single(",
    ];

    scan_slice_for_patterns(
        PROPERTY_ACCESS_STATE,
        "fn resolve_remapped_mapped_property_from_source_union(",
        "pub(crate) fn computed_property_display_name(",
        &forbidden,
        &mut violations,
    );
    scan_slice_for_patterns(
        PROPERTY_ACCESS_STATE,
        "pub(crate) fn resolve_property_access_with_env_post_query(",
        "fn resolve_mapped_constraint_for_property_access(",
        &forbidden,
        &mut violations,
    );
    scan_slice_for_patterns(
        PROPERTY_ACCESS_STATE,
        "fn resolve_mapped_property_with_env(",
        "#[cfg(test)]",
        &forbidden,
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "property-access result construction must route through \
         query_boundaries::property_access:\n{}",
        violations.join("\n")
    );
}

#[test]
fn property_access_boundary_owns_result_construction_helpers() {
    let source = fs::read_to_string(checker_path(PROPERTY_ACCESS_BOUNDARY))
        .expect("failed to read query_boundaries/property_access.rs");

    for helper in [
        "mapped_property_read_type",
        "union_property_access_success",
        "intersection_property_access_success",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::property_access must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.union2(",
        "tsz_solver::utils::union_or_single(",
        "tsz_solver::utils::intersection_or_single(",
        "PropertyAccessResult::simple(",
        "PropertyAccessResult::from_index(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::property_access should own `{construction_pattern}`"
        );
    }
}
