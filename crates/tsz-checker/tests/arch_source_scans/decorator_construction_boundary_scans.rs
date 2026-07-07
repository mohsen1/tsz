//! Class-member decorator construction boundary scans.
//!
//! Decorator signature checking owns AST/member-kind decisions, decorator ABI
//! selection, diagnostics, and relation outcomes. Solver construction for the
//! semantic helper types used by member decorators belongs in
//! `query_boundaries::checkers::decorators`.

use std::fs;
use std::path::{Path, PathBuf};

const DECORATOR_SIGNATURE_CHECKS: &str =
    "src/state/state_checking_members/decorator_signature_checks.rs";
const DECORATOR_BOUNDARY: &str = "src/query_boundaries/checkers/decorators.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn scan_for_patterns(relative: &str, patterns: &[&str], violations: &mut Vec<String>) {
    let source = fs::read_to_string(checker_path(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"));
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for pattern in patterns {
            if line.contains(pattern) {
                violations.push(format!(
                    "{relative}:{} contains `{pattern}`",
                    line_index + 1
                ));
            }
        }
    }
}

fn defines_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}("))
}

#[test]
fn decorator_signature_checks_route_solver_construction_through_boundary() {
    let mut violations = Vec::new();
    scan_for_patterns(
        DECORATOR_SIGNATURE_CHECKS,
        &[
            "create_lazy_type_ref(",
            ".factory().application(",
            ".factory().function(",
            ".factory().union2(",
            "tsz_solver::FunctionShape {",
            "FunctionShape {",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "decorator signature checks must route solver construction through \
         query_boundaries::checkers::decorators:\n{}",
        violations.join("\n")
    );
}

#[test]
fn decorator_boundary_owns_construction_helpers() {
    let source = fs::read_to_string(checker_path(DECORATOR_BOUNDARY))
        .expect("failed to read query_boundaries/checkers/decorators.rs");

    for helper in [
        "decorator_global_type_ref",
        "class_accessor_decorator_target_any",
        "decorator_context_application",
        "method_decorator_value_type",
        "accessor_decorator_value_type",
        "decorator_void_or_replacement_type",
    ] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::checkers::decorators must own `{helper}`"
        );
    }

    for construction_pattern in [
        "db.lazy(",
        "db.application(",
        "db.function(",
        "FunctionShape {",
        "db.union2(",
    ] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::checkers::decorators should own `{construction_pattern}`"
        );
    }
}
