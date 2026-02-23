//! Tests for TS7057: yield expression implicitly results in an 'any' type
//!
//! Verifies that TS7057 fires when noImplicitAny is enabled, a generator
//! has no return type annotation, and the yield result value is consumed.

use crate::CheckerState;
use crate::context::CheckerOptions;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn no_implicit_any_options() -> CheckerOptions {
    CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    }
}

fn count_ts7057(source: &str) -> usize {
    get_diagnostics_with_options(source, no_implicit_any_options())
        .iter()
        .filter(|(code, _)| *code == 7057)
        .count()
}

// =========================================================================
// TS7057 should fire when yield result is consumed
// =========================================================================

#[test]
fn yield_result_assigned_to_untyped_variable() {
    // const value = yield; — result consumed, no contextual type → TS7057
    let source = r#"
function* g() {
    const value = yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "should emit TS7057 for consumed yield result"
    );
}

#[test]
fn yield_result_in_function_call() {
    // f(yield); — result consumed as call argument → TS7057
    let source = r#"
declare function f(x: any): void;
function* g() {
    f(yield);
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "should emit TS7057 for yield in function call"
    );
}

#[test]
fn yield_result_in_assignment() {
    // result = yield result; — result consumed in assignment → TS7057
    let source = r#"
function* f() {
    let result: any;
    result = yield result;
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "should emit TS7057 for yield in assignment"
    );
}

#[test]
fn yield_yield_inner_consumed() {
    // yield yield; — inner yield's result is consumed by outer yield → TS7057
    let source = r#"
function* g() {
    yield yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "should emit TS7057 for inner yield"
    );
}

// =========================================================================
// TS7057 should NOT fire when yield result is unused
// =========================================================================

#[test]
fn bare_yield_in_expression_statement() {
    let source = r#"
function* g() {
    yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "bare yield should not trigger TS7057"
    );
}

#[test]
fn yield_in_comma_expression_left() {
    let source = r#"
declare function noop(): void;
function* g() {
    yield, noop();
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield on left of comma should not trigger TS7057"
    );
}

#[test]
fn yield_in_void_expression() {
    let source = r#"
function* g() {
    void (yield);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "void yield should not trigger TS7057"
    );
}

#[test]
fn yield_in_parenthesized_expression_statement() {
    let source = r#"
function* g() {
    (yield);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "parenthesized yield in expression statement should not trigger TS7057"
    );
}

// =========================================================================
// TS7057 should NOT fire when noImplicitAny is off
// =========================================================================

#[test]
fn no_ts7057_without_no_implicit_any() {
    let source = r#"
function* g() {
    const value = yield;
}
"#;
    let options = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diags = get_diagnostics_with_options(source, options);
    let ts7057_count = diags.iter().filter(|(code, _)| *code == 7057).count();
    assert_eq!(
        ts7057_count, 0,
        "should not emit TS7057 when noImplicitAny is off"
    );
}

// =========================================================================
// TS7057 should NOT fire when yield is in for-statement positions
// =========================================================================

#[test]
fn no_ts7057_in_for_statement() {
    let source = r#"
function* g() {
    for (yield; false; yield);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield in for-statement init/increment should not trigger TS7057"
    );
}
