//! `yield*` contextual construction boundary scans.
//!
//! Yield dispatch owns AST position, generator-name/lib lookup, diagnostics,
//! and contextual-type selection. Solver construction for contextual
//! `Generator<Y, R, N>` and array fallback surfaces belongs in
//! `query_boundaries::dispatch`.

use std::fs;
use std::path::{Path, PathBuf};

const YIELD_DISPATCH: &str = "src/dispatch/yield_.rs";
const DISPATCH_BOUNDARY: &str = "src/query_boundaries/dispatch.rs";

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
fn yield_dispatch_routes_context_construction_through_boundary() {
    let mut violations = Vec::new();
    scan_for_patterns(
        YIELD_DISPATCH,
        &[
            ".types.lazy(",
            ".types.application(",
            ".types.array(",
            "types.lazy(",
            "types.application(",
            "types.array(",
            "db.lazy(",
            "db.application(",
            "db.array(",
            "factory.lazy(",
            "factory.application(",
            "factory.array(",
            ".factory().lazy(",
            ".factory().application(",
            ".factory().array(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "`yield*` dispatch must route contextual solver construction through \
         query_boundaries::dispatch:\n{}",
        violations.join("\n")
    );
}

#[test]
fn dispatch_boundary_keeps_yield_context_decisions_out_of_helpers() {
    let mut violations = Vec::new();
    scan_for_patterns(
        DISPATCH_BOUNDARY,
        &[
            "ExpressionDispatcher",
            "NodeIndex",
            "syntax_kind_ext",
            "get_global_type_with_libs(",
            "find_enclosing_function(",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "query_boundaries::dispatch yield helpers must not absorb checker AST/lib lookup \
         responsibilities:\n{}",
        violations.join("\n")
    );
}

#[test]
fn dispatch_boundary_owns_yield_context_construction_helpers() {
    let source = fs::read_to_string(checker_path(DISPATCH_BOUNDARY))
        .expect("failed to read query_boundaries/dispatch.rs");

    for helper in ["generator_context_application", "yield_star_array_context"] {
        assert!(
            defines_fn(&source, helper),
            "query_boundaries::dispatch must own `{helper}`"
        );
    }

    for construction_pattern in ["db.lazy(", "db.application(", "db.array("] {
        assert!(
            source.contains(construction_pattern),
            "query_boundaries::dispatch should own `{construction_pattern}`"
        );
    }
}
