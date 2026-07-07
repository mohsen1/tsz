//! Call-result construction boundary scans.
//!
//! Call checking owns callee/signature selection, argument diagnostics,
//! optional-chain detection, and recursive fallback detection. Solver
//! construction for the resulting call-result surfaces belongs in
//! `query_boundaries::checkers::call`.

use std::fs;
use std::path::{Path, PathBuf};

const CALL_RESULT: &str = "src/types/computation/call_result.rs";
const CALL_INNER: &str = "src/types/computation/call/inner.rs";
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
fn call_result_construction_routes_through_call_boundary() {
    let mut violations = Vec::new();

    scan_slice_for_patterns(
        CALL_RESULT,
        "fn correlated_union_call_recovery_return(",
        "fn finalize_call_return_like_success(",
        &[
            ".factory().union(",
            ".factory().union2(",
            ".types.union(",
            ".types.union2(",
        ],
        &mut violations,
    );
    scan_slice_for_patterns(
        CALL_RESULT,
        "fn finalize_call_return_like_success(",
        "fn return_application_uses_opaque_object_base(",
        &[
            ".factory().union(",
            ".factory().union2(",
            ".types.union(",
            ".types.union2(",
        ],
        &mut violations,
    );
    scan_slice_for_patterns(
        CALL_INNER,
        "if callee_type == TypeId::ANY {",
        "if callee_type == TypeId::ERROR\n            && let Some(recovered_type)",
        &["factory.lazy(", "factory.application("],
        &mut violations,
    );
    scan_slice_for_patterns(
        CALL_INNER,
        "if callee_type == TypeId::ERROR {",
        "let check_excess_properties = false;",
        &["factory.lazy(", "factory.application("],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "call result construction must route solver construction through \
         query_boundaries::checkers::call:\n{}",
        violations.join("\n")
    );
}

#[test]
fn call_boundary_owns_call_result_construction_helpers() {
    let source = fs::read_to_string(checker_path(CALL_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/call.rs");

    for helper in [
        "call_result_correlated_union",
        "call_result_optional_chain_return",
        "recursive_call_result_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::call must own `{helper}`"
        );
    }

    for construction_pattern in ["db.union(", "db.union2(", "db.lazy(", "db.application("] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::call should own `{construction_pattern}`"
        );
    }
}
