//! Common Promise classification boundary scans.
//!
//! Array-literal contextual typing owns AST context and Promise-like identity
//! policy. Solver-backed Promise application classification belongs in
//! `query_boundaries::checkers::promise`, not the generic `common` quarantine.

use std::fs;
use std::path::{Path, PathBuf};

const ARRAY_LITERAL: &str = "src/types/computation/array_literal.rs";
const COMMON_BOUNDARY: &str = "src/query_boundaries/common.rs";
const PROMISE_BOUNDARY: &str = "src/query_boundaries/checkers/promise.rs";

fn checker_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn production_source_without_comments(source: &str) -> String {
    let mut stripped = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut block_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if block_depth > 0 {
            match (ch, chars.peek().copied()) {
                ('/', Some('*')) => {
                    chars.next();
                    block_depth += 1;
                }
                ('*', Some('/')) => {
                    chars.next();
                    block_depth -= 1;
                }
                ('\n', _) => stripped.push('\n'),
                _ => {}
            }
            continue;
        }

        if in_string {
            stripped.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match (ch, chars.peek().copied()) {
            ('"', _) => {
                in_string = true;
                stripped.push(ch);
            }
            ('/', Some('/')) => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        stripped.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                block_depth = 1;
            }
            _ => stripped.push(ch),
        }
    }

    stripped
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

#[test]
fn array_literal_routes_promise_application_shape_through_promise_boundary() {
    let source = read_production_source(ARRAY_LITERAL);

    for forbidden in [
        "common::classify_promise_type(",
        "query_boundaries::common::classify_promise_type(",
        "common::PromiseTypeKind",
        "query_boundaries::common::PromiseTypeKind",
    ] {
        assert!(
            !contains_pattern(&source, forbidden),
            "array literal Promise context handling must not use `{forbidden}`"
        );
    }

    assert!(
        contains_pattern(&source, "promise_query::promise_application_parts("),
        "array literal Promise context handling must route through \
         query_boundaries::checkers::promise"
    );
}

#[test]
fn common_boundary_no_longer_exports_promise_classification() {
    let source = read_production_source(COMMON_BOUNDARY);

    for forbidden in [
        "pub(crate) fn classify_promise_type(",
        "pub(crate) use tsz_solver::type_queries::PromiseTypeKind",
        "PromiseTypeKind,",
    ] {
        assert!(
            !contains_pattern(&source, forbidden),
            "`query_boundaries::common` must not export Promise classification via `{forbidden}`"
        );
    }
}

#[test]
fn promise_boundary_owns_array_literal_promise_application_helper() {
    let source = read_production_source(PROMISE_BOUNDARY);
    assert!(
        defines_fn(&source, "promise_application_parts"),
        "query_boundaries::checkers::promise must own `promise_application_parts`"
    );
}
