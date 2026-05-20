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

fn non_strict_no_implicit_any_options() -> CheckerOptions {
    CheckerOptions {
        no_implicit_any: true,
        strict_null_checks: false,
        ..CheckerOptions::default()
    }
}

fn count_ts7057(source: &str) -> usize {
    get_diagnostics_with_options(source, no_implicit_any_options())
        .iter()
        .filter(|(code, _)| *code == 7057)
        .count()
}

fn count_ts7055(source: &str, options: CheckerOptions) -> usize {
    get_diagnostics_with_options(source, options)
        .iter()
        .filter(|(code, _)| *code == 7055)
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

// =========================================================================
// TS7057 should NOT fire when yield is contextually typed by function args
// =========================================================================

#[test]
fn no_ts7057_yield_in_typed_function_call() {
    // f<string>(yield) — yield is contextually typed by explicit type argument
    let source = r#"
declare function f<T>(value: T): void;
function* g() {
    f<string>(yield);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield in call with explicit type arg should not trigger TS7057"
    );
}

#[test]
fn no_ts7057_yield_in_overloaded_function_call() {
    // f1(yield 1) — yield is contextually typed by overload parameter
    let source = r#"
declare function f1(x: string): void;
declare function f1(x: number): void;
function* g() {
    const x = f1(yield 1);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield in overloaded call should not trigger TS7057"
    );
}

// =========================================================================
// TS7057 should NOT fire when yield is in destructuring initializer
// =========================================================================

#[test]
fn no_ts7057_yield_in_array_destructuring() {
    // const [a, b] = yield — yield contextually typed by binding pattern
    let source = r#"
function* g() {
    const [a = 1, b = 2] = yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield in array destructuring initializer should not trigger TS7057"
    );
}

#[test]
fn no_ts7057_yield_in_object_destructuring() {
    // const {x} = yield — yield contextually typed by binding pattern
    let source = r#"
function* g() {
    const {x, y} = yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield in object destructuring initializer should not trigger TS7057"
    );
}

#[test]
fn ts7057_still_fires_for_untyped_simple_variable() {
    // const value = yield — NO contextual type, TS7057 should still fire
    let source = r#"
function* g() {
    const value = yield;
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "yield in simple untyped variable should still trigger TS7057"
    );
}

#[test]
fn no_ts7057_yield_with_type_param_variable_annotation() {
    // const a: T = yield 0 — type param from variable annotation provides valid context
    let source = r#"
function* g307<T>() {
    const a: T = yield 0;
    return a;
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield assigned to type-param-annotated variable should not trigger TS7057"
    );
}

#[test]
fn ts7057_fires_for_generic_call_argument() {
    // f2(yield 1) where f2<T>(x: T) — T is inferred from yield (any), so TS7057 fires
    let source = r#"
declare function f2<T>(x: T): T;
function* g204() {
    const x = f2(yield 1);
}
"#;
    assert_eq!(
        count_ts7057(source),
        1,
        "yield in generic call argument should trigger TS7057"
    );
}

#[test]
fn no_ts7057_for_yield_in_nested_dynamic_import_argument() {
    // Mirrors asyncImportNestedYield.ts: yield is contextually typed by import().
    let source = r#"
async function* g() {
    import((await import(yield "foo")).default);
}
"#;
    assert_eq!(
        count_ts7057(source),
        0,
        "yield inside nested dynamic import argument should not trigger TS7057"
    );
}

// =========================================================================
// TS7055: Generator function implicitly has 'any' yield type
// =========================================================================

#[test]
fn ts7055_fires_for_yield_star_empty_array_nonstrict() {
    // In non-strict mode (strictNullChecks: false), `[]` produces `undefined[]`.
    // The element type `undefined` is widened to `any` for the generator yield type,
    // which triggers TS7055 ("g003 implicitly has any yield type").
    //
    // This matches tsc's behavior:
    //   function* g003() { yield* []; }  →  TS7055
    //   "In non-strict mode, `[]` produces the type `undefined[]` which is implicitly any."
    let source = r#"
function* g003() {
    yield* [];
}
"#;
    assert_eq!(
        count_ts7055(source, non_strict_no_implicit_any_options()),
        1,
        "yield* [] in non-strict mode should emit TS7055 (implicit any yield type)"
    );
}

#[test]
fn ts7055_fires_for_bare_yield_nonstrict() {
    // `yield;` in non-strict mode: the bare yield produces `undefined`, which is
    // widened to `any` → TS7055 fires.
    let source = r#"
function* g001() {
    yield;
}
"#;
    assert_eq!(
        count_ts7055(source, non_strict_no_implicit_any_options()),
        1,
        "bare yield in non-strict mode should emit TS7055 (implicit any yield type)"
    );
}

#[test]
fn no_ts7055_for_yield_star_empty_array_strict() {
    // In strict mode, `[]` produces `never[]`. Iterating never[] gives never.
    // never is NOT widened to any → no TS7055.
    let source = r#"
function* g003() {
    yield* [];
}
"#;
    assert_eq!(
        count_ts7055(
            source,
            CheckerOptions {
                no_implicit_any: true,
                strict_null_checks: true,
                ..CheckerOptions::default()
            }
        ),
        0,
        "yield* [] in strict mode should NOT emit TS7055 (yield type is never, not any)"
    );
}
