//! Class surface construction boundary scans.
//!
//! Class instance/constructor code owns AST collection, inheritance merging,
//! late-bound-member discovery, and cache publication. Solver construction for
//! merged instance/interface surfaces and final constructor surfaces belongs in
//! `query_boundaries::class_type`.

use std::fs;
use std::path::{Path, PathBuf};

const CLASS_HELPERS: &str = "src/types/class_type/helpers.rs";
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
    scan_source_for_patterns(
        relative,
        &after_start[..end],
        patterns,
        line_offset,
        violations,
    );
}

fn scan_source_for_patterns(
    relative: &str,
    source: &str,
    patterns: &[&str],
    line_offset: usize,
    violations: &mut Vec<String>,
) {
    let source = production_source_without_comments(source);
    let compact_source = compact(&source);
    for pattern in patterns {
        if source.contains(pattern) || compact_source.contains(&compact(pattern)) {
            violations.push(format!(
                "{relative}:{} contains `{pattern}`",
                line_offset + 1
            ));
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn class_surface_callers_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();

    scan_slice_for_patterns(
        CLASS_HELPERS,
        "pub(super) fn merge_class_instance_with_interface(",
        "/// For JS classes without syntax-level type parameters",
        &[
            "CallableShape {",
            "ObjectShape {",
            "IndexSignature {",
            "factory.callable(",
            "factory.object(",
            "factory.object_with_index(",
            "factory.union2(",
            ".factory().union2(",
        ],
        &mut violations,
    );

    scan_slice_for_patterns(
        CLASS_CONSTRUCTOR,
        "// Add default constructor if none exists",
        "// Track constructor accessibility",
        &[
            "CallSignature {",
            "IndexSignature {",
            "CallableShape {",
            "factory.callable(",
            "factory.union2(",
        ],
        &mut violations,
    );

    scan_slice_for_patterns(
        CLASS_CONSTRUCTOR,
        "// Mixin pattern:",
        "        constructor_type\n    }\n}",
        &["factory.intersection2("],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "class surface callers must route solver construction through \
         query_boundaries::class_type:\n{}",
        violations.join("\n")
    );
}

#[test]
fn class_type_boundary_owns_class_surface_construction_helpers() {
    let source = fs::read_to_string(checker_path(CLASS_TYPE_BOUNDARY))
        .expect("failed to read query_boundaries/class_type.rs");

    for helper in [
        "MergedClassInstanceInterfaceSurface",
        "merged_class_instance_interface_type",
        "class_constructor_callable_type",
        "class_constructor_mixin_intersection",
    ] {
        if helper == "MergedClassInstanceInterfaceSurface" {
            assert!(
                source.contains("struct MergedClassInstanceInterfaceSurface"),
                "query_boundaries::class_type must own `{helper}`"
            );
        } else {
            assert!(
                defines_fn(&source, helper),
                "query_boundaries::class_type must own `{helper}`"
            );
        }
    }

    for construction_pattern in [
        "db.callable(",
        "CallableShape {",
        "db.object(",
        "db.object_with_index(",
        "ObjectShape {",
        "db.intersection2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::class_type should own `{construction_pattern}`"
        );
    }
}
