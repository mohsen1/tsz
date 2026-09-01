//! Regression tests for TS1308 (`await` outside an async function) in the
//! statement positions the `await`-grammar walk never reached.
//!
//! `check_await_expression` is rooted per statement kind, on that statement's
//! own expression children (`crates/tsz-checker/src/statements.rs`). Before
//! this suite the rooted set was `ExpressionStatement`, an `if` condition, a
//! `for..in`/`for..of` iterated expression, a `return` operand, variable
//! declarations, property initializers, decorators, and (since #16061) the
//! dispatcher's catch-all arm for a concise arrow body. Five statement kinds
//! owned expression positions with no root at all, so tsz was silent where
//! tsc reports:
//!
//! - `while` / `do..while` condition
//! - `for` initializer (expression form), condition, incrementor
//! - `switch` discriminant and `case` expressions
//! - `throw` operand
//!
//! Every expectation here is pinned against a live
//! `tsc@7.0.2 --noEmit --strict --pretty false --target es2017` run, not
//! recalled. tsc's grammar checks are syntactic, so they fire in unreachable
//! code too — `count_ts1308` cases below pin that as well.

use crate::test_utils::{check_source_codes, check_source_diagnostics};

/// TS1308 occurrences in `source`. Counting rather than testing membership:
/// `error_at_node` deduplicates by `(start, code)`, so a body the checker
/// visits more than once (a `while` body is visited twice, a contextually
/// typed callback body twice) must still yield exactly one diagnostic. A
/// `contains` assertion cannot see a regression that starts double-reporting
/// at distinct positions.
fn count_ts1308(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 1308)
        .count()
}

// --- while / do..while conditions ---

/// `while (await 1) {}` in a non-async function. tsc:
/// `(1,23): error TS1308`.
#[test]
fn while_condition_await_reports_ts1308() {
    let source = r"
function w() { while (await 1) {} }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `while` condition's `await` outside an async function must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// `do {} while (await 1);` — the same arm, the other loop form. tsc:
/// `(1,29): error TS1308`.
#[test]
fn do_while_condition_await_reports_ts1308() {
    let source = r"
function d() { do {} while (await 1); }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `do..while` condition's `await` must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A `while` body is checked twice (once, then again after the loop-body
/// recheck caches are cleared). The body's own `await` must still report
/// once — this is the case that regresses if the walk is ever rooted at the
/// top of the dispatcher instead of per-arm.
#[test]
fn while_body_await_reports_exactly_one_ts1308_despite_double_visit() {
    let source = r"
function w() { while (true) { await 1; } }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a twice-visited `while` body must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// Renamed-binder control (anti-hardcoding): different function name,
/// different awaited literal, same `while` shape.
#[test]
fn while_condition_await_reports_ts1308_renamed_binders() {
    let source = r"
function pollUntilSettled() { while (await 'ready') {} }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "renamed binders must not change the `while` condition result; got {:?}",
        check_source_codes(source)
    );
}

// --- for clause expressions ---

/// `for (await 1; ; ) {}` — an expression-form initializer. tsc:
/// `(1,21): error TS1308`.
#[test]
fn for_expression_initializer_await_reports_ts1308() {
    let source = r"
function a() { for (await 1; ; ) {} }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "an expression-form `for` initializer must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A variable-declaration-list initializer is rooted by the declaration
/// check, not by the `for` arm. Pinned so the two roots cannot both be
/// removed on the assumption the other covers it. tsc reports here too.
#[test]
fn for_declaration_initializer_await_reports_ts1308() {
    let source = r"
function a() { for (let i = await 0; ; ) {} }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a declaration-form `for` initializer must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// Condition and incrementor together. tsc reports both:
/// `(1,29)` and `(1,38)`.
#[test]
fn for_condition_and_incrementor_await_report_two_ts1308() {
    let source = r"
function f() { for (; await 1; await 2) {} }
";
    assert_eq!(
        count_ts1308(source),
        2,
        "a `for` condition and incrementor each report their own TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// tsc's grammar checks are syntactic and run in unreachable code:
/// `function b() { return; for (; await 1; await 2) {} }` reports at both
/// `(2,31)` and `(2,40)`. tsz skips *typing* an unreachable condition and
/// incrementor, so the grammar roots must sit before that analysis.
#[test]
fn unreachable_for_clause_await_still_reports_ts1308() {
    let source = r"
function b() { return; for (; await 1; await 2) {} }
";
    assert_eq!(
        count_ts1308(source),
        2,
        "grammar checks are syntactic: an unreachable `for` clause still reports TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A `for` body's `await` was already reported through the
/// `ExpressionStatement` root; pinning it here guards the interaction with
/// the new clause roots. tsc: `(1,44)` for the body form below.
#[test]
fn for_body_await_reports_exactly_one_ts1308() {
    let source = r"
function d() { for (;;) { await 1; } }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `for` body's `await` must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

// --- switch discriminant and case expressions ---

/// `switch (await 1)` — the discriminant. tsc: `(1,24): error TS1308`.
#[test]
fn switch_discriminant_await_reports_ts1308() {
    let source = r"
function s() { switch (await 1) { default: break; } }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `switch` discriminant's `await` must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// `case await 2:` — a case expression, which the arm visits through
/// `get_type_of_case_expression_with_request` rather than the ordinary
/// expression path. tsc reports TS1308 there as well.
#[test]
fn switch_case_expression_await_reports_ts1308() {
    let source = r"
function e() { switch (1) { default: break; case await 1: break; } }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `case` expression's `await` must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// Discriminant and two case expressions: three distinct positions, three
/// diagnostics. Pins that the walk roots per case clause and not once for
/// the whole case block.
#[test]
fn switch_discriminant_and_two_cases_report_three_ts1308() {
    let source = r"
function e() { switch (await 0) { case await 1: break; case await 2: break; } }
";
    assert_eq!(
        count_ts1308(source),
        3,
        "a discriminant and two `case` expressions report one TS1308 each; got {:?}",
        check_source_codes(source)
    );
}

// --- throw operand ---

/// `throw await 1;` — the `throw` arm carries its operand through
/// `ReturnData`, so it never reached the `return` root. tsc:
/// `(1,29): error TS1308`.
#[test]
fn throw_operand_await_reports_ts1308() {
    let source = r"
function t(): never { throw await 1; }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `throw` operand's `await` must report exactly one TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// The same `throw` shape nested in an object-literal method — a different
/// enclosing function form reaching the same arm. tsc reports it.
#[test]
fn throw_operand_await_in_object_literal_method_reports_ts1308() {
    let source = r"
function h() { const holder = { emit() { throw await 1; } }; }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `throw` operand inside an object-literal method must report TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// And in a function expression, the third enclosing-function form. tsc:
/// `(1,35): error TS1308`.
///
/// A class method, constructor, or accessor body was once wrong here and is
/// now correct and pinned below (`class_member_body_await_reports_ts1308`):
/// #16070 made a class member body a `ctx.function_depth` boundary.
///
/// A class static block was once wrong here in the same way: through this
/// suite's parse-health-blind `check_source_codes` (which uses
/// `CheckerOptions::default()`) `class K { static { await 1; } }` answered
/// `[TS1375, TS1378]`, the top-level-await pair, as if the static block were
/// the top level of the file. That is fixed. #16367 made
/// `checkAwaitExpression`'s class-static-block branch exclusive at the source
/// (`await_container_is_class_static_block` in `core_statement_checks.rs`), so
/// this walk now declines and the parser's TS18037 stands alone. It is pinned
/// in its own suite, `await_static_block_grammar_tests.rs` — in particular
/// `static_block_await_is_silent_in_the_checker_walk_without_suppression`,
/// which asserts that the same blind `check_source_codes` helper now reports
/// none of TS1308/TS1375/TS1378 — rather than restated here.
#[test]
fn while_condition_await_in_function_expression_reports_ts1308() {
    let source = r"
const gate = function () { while (await 1) {} };
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a `while` condition inside a function expression must report TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A `label:` wrapper delegates to the labeled statement through the
/// dispatcher, so the loop's own root still applies. tsc: `(1,30)`.
#[test]
fn labeled_while_condition_await_reports_ts1308() {
    let source = r"
function g() { outer: while (await 1) {} }
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a labeled `while`'s condition must still report TS1308; got {:?}",
        check_source_codes(source)
    );
}

// --- negative controls: the roots must not report on their own ---

/// Every position above, legal inside an `async function`. No TS1308 —
/// confirmed against tsc, which reports nothing for this source.
#[test]
fn async_function_await_in_every_statement_position_reports_no_ts1308() {
    let source = r"
async function all(p: Promise<number>) {
    while (await p) { break; }
    do { break; } while (await p);
    for (await p; await p; await p) { break; }
    switch (await p) { case await p: break; default: break; }
    if (await p) { }
    throw await p;
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "inside an async function every awaited position is legal; got {:?}",
        check_source_codes(source)
    );
}

/// The same statement kinds with no `await` at all: the new roots must not
/// synthesize a diagnostic of their own.
#[test]
fn statements_without_await_report_no_ts1308() {
    let source = r"
function none(): never {
    while (false) { }
    do { break; } while (false);
    for (let i = 0; i < 1; i++) { }
    switch (1) { case 2: break; default: break; }
    throw new Error('x');
}
";
    assert_eq!(
        count_ts1308(source),
        0,
        "statements with no `await` must report no TS1308; got {:?}",
        check_source_codes(source)
    );
}

/// A nested non-async function inside an `async` one still reports: the walk
/// stops at function boundaries, so the inner function's own async state
/// governs. tsc reports TS1308 for the inner `while`.
#[test]
fn non_async_function_nested_in_async_function_reports_ts1308() {
    let source = r"
async function outer(p: Promise<number>) {
    function inner() { while (await 1) {} }
    await p;
}
";
    assert_eq!(
        count_ts1308(source),
        1,
        "a non-async function nested in an async one still reports TS1308 for its own `await`; got {:?}",
        check_source_codes(source)
    );
}

// --- enclosing forms the header once listed as unasserted defects ---

/// A class method, constructor, or accessor body is a `ctx.function_depth`
/// boundary (#16070), so a non-async one answers TS1308 rather than the
/// top-level-await pair. tsc on each of the three, `--target es2017`:
/// `TS1308`, once.
#[test]
fn class_member_body_await_reports_ts1308() {
    for source in [
        "class K { m() { await 1; } }",
        "class K { constructor() { await 1; } }",
        "class K { get g() { await 1; return 1; } }",
    ] {
        assert_eq!(
            count_ts1308(source),
            1,
            "a non-async class member body must report exactly one TS1308: {source}"
        );
        assert!(
            !check_source_codes(source).contains(&1375),
            "a class member body is not the top level of the file: {source}"
        );
    }
}

/// Renamed binders for the same three forms — the boundary is structural, not
/// keyed on any member or class name.
#[test]
fn class_member_body_await_is_name_agnostic() {
    for source in [
        "class Widget { render() { await 1; } }",
        "class Widget { constructor() { await 1; } }",
        "class Widget { get label() { await 1; return 1; } }",
    ] {
        assert_eq!(count_ts1308(source), 1, "renamed binders: {source}");
    }
}
