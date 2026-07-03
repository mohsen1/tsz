//! Object-literal contextual construction boundary scans.
//!
//! Object-literal contextual typing owns member selection, fallback policy,
//! callable classification, and property lookup. Solver construction for
//! contextual union/intersection rebuilds belongs in
//! `query_boundaries::object_literal_context`.

use std::fs;
use std::path::{Path, PathBuf};

const OBJECT_LITERAL_CONTEXT: &str = "src/types/computation/object_literal_context.rs";
const OBJECT_LITERAL_CONTEXT_BOUNDARY: &str = "src/query_boundaries/object_literal_context.rs";

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
    for (line_index, line) in source.lines().enumerate() {
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
fn object_literal_context_routes_contextual_rebuilds_through_boundary() {
    let mut violations = Vec::new();
    let forbidden = [
        ".factory().union_preserve_members(",
        ".factory().intersection(",
        ".types.union_preserve_members(",
        ".types.intersection(",
        "factory.union_preserve_members(",
        "factory.intersection(",
    ];

    scan_slice_for_patterns(
        OBJECT_LITERAL_CONTEXT,
        "pub(crate) fn strip_contextual_this_type_markers(",
        "fn should_preserve_absent_contextual_property_type(",
        &forbidden,
        &mut violations,
    );
    scan_slice_for_patterns(
        OBJECT_LITERAL_CONTEXT,
        "pub(crate) fn precise_callable_context_type(",
        "pub(crate) fn function_initializer_context_type(",
        &forbidden,
        &mut violations,
    );
    scan_slice_for_patterns(
        OBJECT_LITERAL_CONTEXT,
        "let union_member_property_type = |this: &mut Self,",
        "let original_contextual_type = contextual_type;",
        &forbidden,
        &mut violations,
    );
    scan_slice_for_patterns(
        OBJECT_LITERAL_CONTEXT,
        "fn mapped_contextual_property_type(",
        "fn contextual_union_reduces_to_member(",
        &[
            ".types.literal_number(",
            "common::create_string_literal_type(",
        ],
        &mut violations,
    );
    scan_slice_for_patterns(
        OBJECT_LITERAL_CONTEXT,
        "pub(crate) fn narrow_contextual_union_via_object_literal_discriminants(",
        "fn shorthand_const_literal_type(",
        &forbidden,
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "object literal contextual rebuilds must route solver construction through \
         query_boundaries::object_literal_context:\n{}",
        violations.join("\n")
    );
}

#[test]
fn object_literal_context_boundary_owns_contextual_rebuild_helpers() {
    let source = fs::read_to_string(checker_path(OBJECT_LITERAL_CONTEXT_BOUNDARY))
        .expect("failed to read query_boundaries/object_literal_context.rs");

    for helper in [
        "contextual_union_preserve_members",
        "contextual_intersection",
        "mapped_contextual_property_number_key_type",
        "mapped_contextual_property_string_key_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::object_literal_context must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.union_preserve_members(",
        "db.intersection(",
        "db.literal_number(",
        "db.literal_string(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::object_literal_context should own `{construction_pattern}`"
        );
    }
}
