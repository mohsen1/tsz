//! Interface merge surface construction boundary scans.
//!
//! `interface_type.rs` owns heritage traversal, type resolution, merge mode,
//! property override/order policy, signature deduplication, and index fallback
//! selection. Solver construction for the final merged callable, object,
//! indexed-object, and intersection surfaces belongs in
//! `query_boundaries::interface_merge`.

use std::fs;
use std::path::{Path, PathBuf};

const INTERFACE_TYPE: &str = "src/types/interface_type.rs";
const INTERFACE_MERGE_BOUNDARY: &str = "src/query_boundaries/interface_merge.rs";

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

fn slice_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = source
        .find(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"));
    let rest = &source[start_idx..];
    let end_idx = rest
        .find(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}`"));
    &rest[..end_idx]
}

fn scan_source_for_patterns(source: &str, label: &str, patterns: &[&str]) -> Vec<String> {
    let source = production_source_without_comments(source);
    let compact_source = compact(&source);
    patterns
        .iter()
        .filter(|pattern| source.contains(**pattern) || compact_source.contains(&compact(pattern)))
        .map(|pattern| format!("{label} contains `{pattern}`"))
        .collect()
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
#[ignore = "boundary routing reverted: the routed reconstruction changed the \
interned surface shape (`object_with_flags_and_symbol` vs `object_with_index`, \
derived-shape flag copying) and regressed interface-extends-class member \
visibility (TS2341/TS2445 conformance: interfaceExtendingClassWithPrivates \
family). Re-enable when the routing is re-landed value-identically."]
fn interface_merge_reconstruction_routes_solver_construction_through_boundary() {
    let source =
        fs::read_to_string(checker_path(INTERFACE_TYPE)).expect("failed to read interface_type.rs");
    let merge_impl = slice_between(
        &source,
        "fn merge_interface_types_impl",
        "fn resolve_type_for_interface_merge",
    );
    let merge_intersection = slice_between(
        &source,
        "fn merge_with_intersection",
        "fn merge_overriding_property",
    );

    let patterns = [
        "CallableShape {",
        "ObjectShape {",
        "factory.callable(",
        "factory.object_with_symbol(",
        "factory.object_with_index(",
        "factory.intersection(",
        "factory.intersection2(",
        ".object_with_flags_and_symbol(",
        ".object_with_index(",
        ".callable(",
        ".intersection(",
        ".intersection2(",
    ];

    let mut violations =
        scan_source_for_patterns(merge_impl, "merge_interface_types_impl", &patterns);
    violations.extend(scan_source_for_patterns(
        merge_intersection,
        "merge_with_intersection",
        &patterns,
    ));

    assert!(
        violations.is_empty(),
        "interface merge reconstruction must route solver construction through \
         query_boundaries::interface_merge:\n{}",
        violations.join("\n")
    );
}

#[test]
fn interface_merge_boundary_owns_surface_helpers() {
    let source = fs::read_to_string(checker_path(INTERFACE_MERGE_BOUNDARY))
        .expect("failed to read query_boundaries/interface_merge.rs");

    for helper in [
        "merged_callable_type",
        "merged_object_type",
        "merged_object_with_index_type",
        "merged_intersection_type",
        "merged_intersection_pair_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::interface_merge must own `{helper}`"
        );
    }

    for construction_pattern in [
        "CallableShape {",
        "ObjectShape {",
        "db.callable(",
        "db.object_with_flags_and_symbol(",
        "db.object_with_index(",
        "db.intersection(",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::interface_merge should own `{construction_pattern}`"
        );
    }
}
