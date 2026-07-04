//! Call spread-constraint classification boundary scans.
//!
//! Call modules decide how spread arguments affect argument slots, overload
//! candidates, and diagnostics. Structural classification of type-parameter
//! spread constraints as array/tuple-like belongs in
//! `query_boundaries::checkers::call` so candidate collection, non-tuple spread
//! validation, and overload selection cannot drift.

use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATE_COLLECTION: &str = "src/checkers/call_checker/candidate_collection.rs";
const NON_TUPLE_SPREAD_SIGNATURE: &str = "src/checkers/call_checker/non_tuple_spread_signature.rs";
const SPREAD_OVERLOAD_SELECTION: &str = "src/checkers/call_checker/spread_overload_selection.rs";
const ITERABLE_CHECKER: &str = "src/checkers/iterable_checker.rs";
const CALL_BOUNDARY: &str = "src/query_boundaries/checkers/call.rs";

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

fn contains_pattern(source: &str, pattern: &str) -> bool {
    source.contains(pattern) || compact(source).contains(&compact(pattern))
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

fn read_production_source(relative: &str) -> String {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    production_source_without_comments(&source)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = read_production_source(relative);
    for pattern in patterns {
        if contains_pattern(&source, pattern) {
            violations.push(format!("{relative} contains `{pattern}`"));
        }
    }
}

#[test]
fn call_spread_constraint_classification_routes_through_call_boundary() {
    let mut violations = Vec::new();

    scan_for_patterns(
        CANDIDATE_COLLECTION,
        &[
            "mod spread_constraints",
            "constraint_is_array_or_tuple_like(",
            "pub(crate) fn spread_constraint_is_array_or_tuple_like(",
            "check_type_substituted_constraint(",
            "conditional_default_constraint(",
        ],
        &mut violations,
    );
    for relative in [NON_TUPLE_SPREAD_SIGNATURE, SPREAD_OVERLOAD_SELECTION] {
        scan_for_patterns(
            relative,
            &[
                "type_parameter_constraint(",
                "array_element_type_for_type(self.ctx.types, constraint)",
                "tuple_elements_for_type(self.ctx.types, constraint)",
            ],
            &mut violations,
        );
    }
    scan_for_patterns(
        ITERABLE_CHECKER,
        &[
            "spread_constraint_is_array_or_tuple_like(",
            "check_type_substituted_constraint(",
            "conditional_default_constraint(",
            "array_element_type_for_type(self.ctx.types, constraint)",
            "tuple_elements_for_type(self.ctx.types, constraint)",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "call spread-constraint classification must route through \
         query_boundaries::checkers::call:\n{}",
        violations.join("\n")
    );
}

#[test]
fn call_boundary_owns_spread_constraint_classification_helpers() {
    let call_boundary = read_production_source(CALL_BOUNDARY);
    for helper in [
        "spread_type_parameter_constraint_is_array_or_tuple_like_for_call",
        "spread_constraint_is_array_or_tuple_like_for_call",
    ] {
        assert!(
            defines_fn(&call_boundary, helper),
            "query_boundaries::checkers::call must own `{helper}`"
        );
    }

    for owned_pattern in [
        "type_parameter_constraint(",
        "direct_spread_constraint_is_array_or_tuple_like_for_call(",
        "evaluate_with_env(",
        "check_type_substituted_constraint(",
        "conditional_default_constraint(",
        "base_constraint_is_array_or_tuple(",
    ] {
        assert!(
            contains_pattern(&call_boundary, owned_pattern),
            "query_boundaries::checkers::call should own `{owned_pattern}`"
        );
    }

    for relative in [
        CANDIDATE_COLLECTION,
        NON_TUPLE_SPREAD_SIGNATURE,
        SPREAD_OVERLOAD_SELECTION,
        ITERABLE_CHECKER,
    ] {
        let source = read_production_source(relative);
        assert!(
            contains_pattern(
                &source,
                "spread_type_parameter_constraint_is_array_or_tuple_like_for_call("
            ),
            "{relative} must route spread type-parameter constraints through the call boundary"
        );
    }
}
