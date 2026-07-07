//! Await/promise construction boundary scans.
//!
//! Await checking owns AST context, diagnostics, lib/global lookup, and
//! recursion policy. Solver construction for contextual promise operands and
//! distributed `Awaited<T>` joins belongs in
//! `query_boundaries::checkers::promise`.

use std::fs;
use std::path::{Path, PathBuf};

const AWAIT_CHECKER: &str = "src/types/computation/access_await.rs";
const PROMISE_CHECKER: &str = "src/checkers/promise_checker.rs";
const PROMISE_BOUNDARY: &str = "src/query_boundaries/checkers/promise.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    scan_source_for_patterns(relative, &source, patterns, violations, 0);
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
        violations,
        line_offset,
    );
}

fn scan_source_for_patterns(
    relative: &str,
    source: &str,
    patterns: &[&str],
    violations: &mut Vec<String>,
    line_offset: usize,
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
fn await_checker_routes_promise_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[
        ".factory().union(",
        ".factory().intersection(",
        ".types.union(",
        ".types.intersection(",
        ".types.application(",
        ".application(",
    ];

    let mut violations = Vec::new();
    scan_for_patterns(AWAIT_CHECKER, FORBIDDEN_PATTERNS, &mut violations);
    assert!(
        violations.is_empty(),
        "await checking must route promise/await solver construction through \
         query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_checker_routes_await_union_construction_through_boundary() {
    const FORBIDDEN_PATTERNS: &[&str] = &[".factory().union("];

    let mut violations = Vec::new();
    scan_slice_for_patterns(
        PROMISE_CHECKER,
        "fn extract_awaited_type_from_valid_thenable(",
        "/// Extract the first parameter type from a callable/function type,",
        FORBIDDEN_PATTERNS,
        &mut violations,
    );
    scan_slice_for_patterns(
        PROMISE_CHECKER,
        "pub fn unwrap_async_return_type_for_body(",
        "/// If `type_id` is an `Awaited<X>` application,",
        FORBIDDEN_PATTERNS,
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "promise checking must route await/async union construction through \
         query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_await_construction_helpers() {
    let source = fs::read_to_string(checker_path(PROMISE_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/promise.rs");

    for helper in [
        "promise_application_type",
        "await_contextual_operand_type",
        "awaited_union_type",
        "awaited_intersection_type",
        "thenable_callback_value_union",
        "async_return_body_union",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for construction_pattern in ["db.application(", "db.union(", "db.intersection("] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::promise should own `{construction_pattern}`"
        );
    }
}
