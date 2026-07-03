//! Call-inference construction boundary scans.
//!
//! Call inference gathers AST-derived contribution facts for partial
//! object/function/tuple inference surfaces. Solver construction for those
//! partial shapes belongs in `query_boundaries::checkers::call`.

use std::fs;
use std::path::{Path, PathBuf};

const CALL_HELPERS: &str = "src/types/computation/call_helpers.rs";
const CALL_BOUNDARY: &str = "src/query_boundaries/checkers/call.rs";

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
fn call_inference_partial_construction_routes_through_call_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        "PropertyInfo::new(",
        "PropertyInfo {",
        "TupleElement {",
        "FunctionShape::new(",
        "FunctionShape {",
        ".factory().object_fresh(",
        ".factory().tuple(",
        ".factory().function(",
        ".types.literal_string(",
        ".object_fresh(",
        ".tuple(",
        ".function(",
        ".literal_string(",
    ];

    let mut violations = Vec::new();
    scan_slice_for_patterns(
        CALL_HELPERS,
        "pub(crate) fn extract_non_sensitive_object_type(",
        "/// Check if a type is an intersection containing an Application",
        FORBIDDEN_PATTERNS,
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "call inference must route partial object/function/tuple construction \
         through query_boundaries::checkers::call:\n{}",
        violations.join("\n")
    );
}

#[test]
fn call_boundary_owns_call_inference_partial_construction_helpers() {
    let source = fs::read_to_string(checker_path(CALL_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/call.rs");

    for helper in [
        "call_inference_partial_object_type",
        "call_inference_zero_arg_function_type",
        "call_inference_string_key_type",
        "call_inference_tuple_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::call must own `{helper}`"
        );
    }

    for construction_pattern in [
        "PropertyInfo::new(",
        "FunctionShape::new(",
        "db.function(",
        "db.literal_string(",
        "TupleElement {",
        "db.tuple(",
        "db.object_fresh(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::call should own `{construction_pattern}`"
        );
    }
}
