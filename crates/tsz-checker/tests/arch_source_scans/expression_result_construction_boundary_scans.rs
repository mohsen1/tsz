//! Expression result construction boundary scans.
//!
//! Expression computation owns AST classification, diagnostics, relation probes,
//! and result-collapse policy. Solver construction for selected expression
//! result surfaces belongs in `query_boundaries::type_computation::expression_results`.

use std::fs;
use std::path::{Path, PathBuf};

const NULLISH_COALESCING: &str = "src/types/computation/nullish_coalescing.rs";
const EXPRESSION_GUARDS: &str = "src/types/computation/expression_guards.rs";
const BINARY_SUPPORT: &str = "src/types/computation/binary_support.rs";
const COMPUTATION_HELPERS: &str = "src/types/computation/helpers.rs";
const RESULT_BOUNDARY: &str = "src/query_boundaries/type_computation/expression_results.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_file_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    scan_source_for_patterns(relative, &source, patterns, 0, violations);
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
fn expression_result_callers_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();

    scan_file_for_patterns(
        NULLISH_COALESCING,
        &[".factory().object(", ".factory().union2("],
        &mut violations,
    );
    scan_file_for_patterns(EXPRESSION_GUARDS, &[".factory().union("], &mut violations);
    scan_slice_for_patterns(
        BINARY_SUPPORT,
        "pub(crate) fn reduce_literal_index_access_property_types(",
        "fn global_function_interface_type_for_instanceof(",
        &[".factory().union_preserve_members("],
        &mut violations,
    );
    scan_slice_for_patterns(
        BINARY_SUPPORT,
        "pub(super) fn typeof_result_type_if_typeof(",
        "/// Check if an identifier node's declared type overlaps",
        &[".literal_string(", ".factory().union("],
        &mut violations,
    );
    scan_slice_for_patterns(
        COMPUTATION_HELPERS,
        "k if k == SyntaxKind::TypeOfKeyword as u16 =>",
        "// Unary + and - return number",
        &[".literal_string(", ".factory().union("],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "expression result callers must route solver construction through \
         query_boundaries::type_computation::expression_results:\n{}",
        violations.join("\n")
    );
}

#[test]
fn expression_result_boundary_owns_result_construction_helpers() {
    let source = fs::read_to_string(checker_path(RESULT_BOUNDARY))
        .expect("failed to read query_boundaries/type_computation/expression_results.rs");

    for helper in [
        "empty_object_type",
        "nullish_coalescing_union",
        "conditional_branch_union",
        "literal_index_access_union",
        "typeof_result_union",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::type_computation::expression_results must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.object(",
        "db.union2(",
        "db.union(",
        "db.union_preserve_members(",
        "db.literal_string(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::type_computation::expression_results should own `{construction_pattern}`"
        );
    }
}
