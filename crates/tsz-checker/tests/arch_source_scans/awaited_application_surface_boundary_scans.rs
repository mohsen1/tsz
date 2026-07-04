//! `Awaited<T>` application surface boundary scans.
//!
//! Checker code owns the decision that a base denotes the standard-library
//! `Awaited` alias and owns recursion-depth policy. Solver-backed application,
//! array, union, tuple, and rebuilt application surfaces belong in
//! `query_boundaries::checkers::promise`.

use std::fs;
use std::path::{Path, PathBuf};

const PROMISE_CHECKER: &str = "src/checkers/promise_checker.rs";
const AWAITED_VARIANCE: &str = "src/assignability/awaited_variance_normalization.rs";
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

fn source_between_markers(relative: &str, start_marker: &str, end_marker: &str) -> String {
    let source = read_production_source(relative);
    let start = source
        .find(start_marker)
        .unwrap_or_else(|| panic!("failed to find `{start_marker}` in {relative}"));
    let after_start = &source[start..];
    let end = after_start
        .find(end_marker)
        .unwrap_or_else(|| panic!("failed to find `{end_marker}` in {relative}"));
    after_start[..end].to_owned()
}

#[test]
fn promise_checker_routes_awaited_application_surfaces_through_boundary() {
    let promise_slice = source_between_markers(
        PROMISE_CHECKER,
        "pub(crate) fn awaited_application_arg(",
        "pub(crate) fn builtin_promise_like_application_arg(",
    );
    let variance = read_production_source(AWAITED_VARIANCE);

    let mut violations = Vec::new();
    for (relative, source, patterns) in [
        (
            PROMISE_CHECKER,
            &promise_slice,
            &[
                "query_boundaries::common::array_element_type(",
                "query_boundaries::common::union_members(",
                "query_boundaries::common::tuple_elements(",
                "query_boundaries::common::get_application_base(",
                "query_boundaries::common::application_info(",
                "elem.type_id",
            ][..],
        ),
        (
            AWAITED_VARIANCE,
            &variance,
            &[
                "query_boundaries::common::application_info(",
                ".ctx.types.factory().application(",
                "factory().application(",
            ][..],
        ),
    ] {
        for pattern in patterns {
            if contains_pattern(source, pattern) {
                violations.push(format!("{relative} contains `{pattern}`"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Awaited application discovery and variance normalization must route \
         solver-backed shape reads/rebuilds through \
         query_boundaries::checkers::promise:\n{}",
        violations.join("\n")
    );
}

#[test]
fn promise_boundary_owns_awaited_application_surface_helpers() {
    let boundary = read_production_source(PROMISE_BOUNDARY);
    for helper in [
        "awaited_application_arg_from_type",
        "for_each_awaited_application_container_child",
        "awaited_variance_application_with_mapped_args",
    ] {
        assert!(
            defines_fn(&boundary, helper),
            "query_boundaries::checkers::promise must own `{helper}`"
        );
    }

    for owned_pattern in [
        "get_application_base(db,",
        "application_info(db,",
        "array_element_type(db,",
        "union_members(db,",
        "tuple_elements(db,",
        "db.application(",
    ] {
        assert!(
            contains_pattern(&boundary, owned_pattern),
            "query_boundaries::checkers::promise should own `{owned_pattern}`"
        );
    }
}

#[test]
fn awaited_application_callers_use_promise_boundary_helpers() {
    let promise_slice = source_between_markers(
        PROMISE_CHECKER,
        "pub(crate) fn awaited_application_arg(",
        "pub(crate) fn builtin_promise_like_application_arg(",
    );
    for helper in [
        "awaited_application_arg_from_type",
        "for_each_awaited_application_container_child",
    ] {
        assert!(
            contains_pattern(&promise_slice, &format!("query::{helper}(")),
            "promise_checker.rs must route through `{helper}`"
        );
    }

    let variance = read_production_source(AWAITED_VARIANCE);
    assert!(
        contains_pattern(
            &variance,
            "query::awaited_variance_application_with_mapped_args("
        ),
        "awaited_variance_normalization.rs must route through the promise boundary"
    );
}
