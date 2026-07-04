//! Async promise classification boundary scans.
//!
//! Async Promise diagnostics and await computation own alias policy, recursion
//! limits, and diagnostics. Solver-backed Promise application, base, lazy, union,
//! and object classification belongs in `query_boundaries::checkers::promise`.

use std::fs;
use std::path::{Path, PathBuf};

const ACCESS_AWAIT: &str = "src/types/computation/access_await.rs";
const ASYNC_PROMISE: &str = "src/types/function_type_helpers_async_promise.rs";
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
fn async_promise_callers_do_not_match_promise_type_kind_directly() {
    let mut violations = Vec::new();
    for relative in [ACCESS_AWAIT, ASYNC_PROMISE] {
        let source = read_production_source(relative);
        for pattern in ["PromiseTypeKind", "classify_promise_type("] {
            if contains_pattern(&source, pattern) {
                violations.push(format!("{relative} contains `{pattern}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "async Promise diagnostics and await computation must route Promise \
         classification through query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_async_promise_classification_helpers() {
    let boundary = read_production_source(PROMISE_BOUNDARY);
    for helper in [
        "promise_application_base",
        "promise_application_base_lazy_def_id",
        "promise_base_lazy_def_id",
        "promise_lazy_def_id",
        "promise_union_members",
        "promise_type_is_object",
    ] {
        assert!(
            defines_fn(&boundary, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for exported_raw_classifier in [
        "pub(crate) use tsz_solver::type_queries::PromiseTypeKind",
        "pub(crate) fn classify_promise_type(",
    ] {
        assert!(
            !contains_pattern(&boundary, exported_raw_classifier),
            "raw Promise classification must stay private to the promise boundary"
        );
    }
}

#[test]
fn async_promise_callers_use_boundary_helpers() {
    let access_await = read_production_source(ACCESS_AWAIT);
    for helper in [
        "promise_type_is_object",
        "promise_lazy_def_id",
        "promise_union_members",
    ] {
        assert!(
            contains_pattern(&access_await, &format!("query::{helper}(")),
            "access_await.rs must route through `{helper}`"
        );
    }

    let async_promise = read_production_source(ASYNC_PROMISE);
    for helper in [
        "promise_application_base",
        "promise_application_base_lazy_def_id",
    ] {
        assert!(
            contains_pattern(&async_promise, &format!("query::{helper}(")),
            "function_type_helpers_async_promise.rs must route through `{helper}`"
        );
    }
}
