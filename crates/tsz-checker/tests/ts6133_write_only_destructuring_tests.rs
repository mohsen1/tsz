//! Regression tests for write-only detection in destructuring assignment targets.
//!
//! Structural rule: when a parameter or local variable appears exclusively as
//! the target of an assignment (written but never read), TS6133 must fire.
//! This includes explicit property-assignment destructuring (`{ x: x } = src`),
//! not just shorthand (`{ x } = src`) and array destructuring (`[x] = src`).
//!
//! Root cause: `is_window_and_global_this_declared_expression` called
//! `resolve_identifier_symbol` (tracking) on the write-target identifier,
//! causing it to be added to `referenced_symbols` and suppressing TS6133.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_line_column};

fn check_write_only(source: &str) -> Vec<(u32, u32, u32)> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_unused_parameters: true,
            no_unused_locals: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| {
        let (line, column) = diagnostic_line_column(source, &d);
        (d.code, line, column)
    })
    .collect()
}

fn has_ts6133_at(diags: &[(u32, u32, u32)], line: u32, col: u32) -> bool {
    diags
        .iter()
        .any(|&(code, l, c)| code == 6133 && l == line && c == col)
}

fn has_no_ts6133(diags: &[(u32, u32, u32)]) -> bool {
    diags.iter().all(|&(code, _, _)| code != 6133)
}

#[test]
fn write_only_param_explicit_property_destructuring() {
    // `{ x: x }` — explicit property assignment where `x` (parameter) is only written.
    // The second `x` is a write target; it must still be reported as write-only.
    let diags = check_write_only(
        r"
function f(x = 0) {
    let obj = { x: 1 };
    ({ x: x } = obj);
}
",
    );
    assert!(
        has_ts6133_at(&diags, 2, 12),
        "Expected TS6133 for write-only parameter `x` at (2,12) via explicit destructuring. Got: {diags:?}"
    );
}

#[test]
fn write_only_param_shorthand_destructuring() {
    // `{ x }` — shorthand assignment. Should also be write-only.
    let diags = check_write_only(
        r"
function f(x = 0) {
    let obj = { x: 1 };
    ({ x } = obj);
}
",
    );
    assert!(
        has_ts6133_at(&diags, 2, 12),
        "Expected TS6133 for write-only parameter `x` at (2,12) via shorthand destructuring. Got: {diags:?}"
    );
}

#[test]
fn write_only_param_array_destructuring() {
    // `[x]` — array destructuring assignment. Should be write-only.
    let diags = check_write_only(
        r"
function f(x = 0) {
    ([x] = [1]);
}
",
    );
    assert!(
        has_ts6133_at(&diags, 2, 12),
        "Expected TS6133 for write-only parameter `x` at (2,12) via array destructuring. Got: {diags:?}"
    );
}

#[test]
fn write_only_param_simple_assignment() {
    // `x = 1` — simple assignment. The baseline case.
    let diags = check_write_only(
        r"
function f(x = 0) {
    x = 1;
}
",
    );
    assert!(
        has_ts6133_at(&diags, 2, 12),
        "Expected TS6133 for write-only parameter `x` at (2,12) via simple assignment. Got: {diags:?}"
    );
}

#[test]
fn read_param_is_not_reported() {
    // `x` is read inside the function — must NOT be reported.
    let diags = check_write_only(
        r"
function f(x = 0) {
    return x;
}
",
    );
    assert!(
        has_no_ts6133(&diags),
        "Expected no TS6133 for read parameter `x`. Got: {diags:?}"
    );
}

#[test]
fn explicit_destructuring_does_not_suppress_ts6133_on_adjacent_write_only_local() {
    // Non-regression: a `{ k: v }` destructuring pattern for a LOCAL variable that is
    // only written should still emit TS6133 for `v`.
    let diags = check_write_only(
        r"
function f() {
    let v = 0;
    let obj = { k: 1 };
    ({ k: v } = obj);
}
",
    );
    assert!(
        has_ts6133_at(&diags, 3, 9),
        "Expected TS6133 for write-only local `v` at (3,9). Got: {diags:?}"
    );
}

#[test]
fn window_like_identifier_in_object_literal_still_tracked() {
    // Non-regression: a genuine property assignment where the VALUE is actually
    // read (a normal object literal, not destructuring) must still be tracked
    // as referenced. The fix must not break normal object literal tracking.
    let diags = check_write_only(
        r"
function f(x = 0) {
    let result = { key: x };
    return result;
}
",
    );
    assert!(
        has_no_ts6133(&diags),
        "Expected no TS6133 when `x` is used as value in a non-destructuring object literal. Got: {diags:?}"
    );
}

#[test]
fn write_then_read_explicit_destructuring_no_ts6133() {
    // Symmetric counterpart: parameter `x` is first written via explicit
    // destructuring assignment, then read.  TS6133 must NOT fire because the
    // symbol IS eventually read.
    let diags = check_write_only(
        r"
function f(x = 0) {
    let obj = { x: 1 };
    ({ x: x } = obj);
    return x;
}
",
    );
    assert!(
        has_no_ts6133(&diags),
        "Expected no TS6133 when `x` is written via explicit destructuring then read. Got: {diags:?}"
    );
}
