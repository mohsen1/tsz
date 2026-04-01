use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
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
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn test_unknown_binding_patterns_match_tsc_split_diagnostics() {
    let source = r#"
declare function f<T>(): T;
const {} = f();
const { p1 } = f();
const [] = f();
const [e1, e2] = f();
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let codes: Vec<u32> = relevant.iter().map(|(code, _)| *code).collect();

    assert_eq!(
        codes,
        vec![2571, 2339, 2488, 2571, 2488],
        "Expected TypeScript-style unknown destructuring diagnostics. Actual diagnostics: {relevant:#?}"
    );
}

/// Destructuring parameters with array literal defaults must NOT produce false TS2322 errors
/// when there is no explicit type annotation. TSC infers parameter types from the combination
/// of binding element defaults and the parameter initializer — checking binding defaults
/// against a type inferred purely from the initializer (e.g., `[]` → `never[]`) is incorrect.
///
/// Corresponds to conformance test: destructuringWithLiteralInitializers2.ts
#[test]
fn test_destructuring_param_defaults_no_false_ts2322() {
    let source = r#"
function f01([x, y] = []) {}
function f11([x = 0, y] = []) {}
function f21([x = 0, y = 'bar'] = []) {}
function f22([x = 0, y = 'bar'] = [1]) {}
function f23([x = 0, y = 'bar'] = [1, 'foo']) {}
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322_errors: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        0,
        "Destructuring parameter defaults with inferred types should not produce TS2322. \
         Got {} TS2322 errors: {ts2322_errors:#?}",
        ts2322_errors.len()
    );
}

/// When a destructuring parameter HAS an explicit type annotation, binding element defaults
/// that are incompatible with the declared type SHOULD produce TS2322.
#[test]
fn test_destructuring_param_defaults_ts2322_with_annotation() {
    let source = r#"
function f([x = 'hello']: [number]) {}
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();

    assert!(
        ts2322_count > 0,
        "Destructuring parameter with explicit type annotation should produce TS2322 \
         when default is incompatible. Got diagnostics: {diagnostics:#?}"
    );
}

/// When a generic function's type parameter has no inference candidates (no constraint,
/// no default, and the only argument is a callback with a binding-pattern parameter),
/// T falls back to `unknown`. The callback's binding-pattern type (`{a: any}`) is NOT
/// assignable from `unknown` (the instantiated parameter type), so TS2345 must be emitted.
///
/// This is the `fallbackToBindingPatternForTypeInference` conformance test from TypeScript.
#[test]
fn test_binding_pattern_callback_does_not_infer_generic_parameter() {
    let source = r#"
declare function trans<T>(f: (x: T) => string): number;
trans(({a}) => a);
trans(([b,c]) => 'foo');
trans(({d: [e,f]}) => 'foo');
trans(([{g},{h}]) => 'foo');
trans(({a, b = 10}) => a);
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();

    assert_eq!(
        ts2345_count, 5,
        "Expected 5 TS2345 errors for binding-pattern callbacks with uninferred T. \
         Got {} TS2345 errors. All diagnostics: {diagnostics:#?}",
        ts2345_count
    );
}

#[test]
fn test_destructuring_assignment_defaults_skip_ts2493_for_empty_array_literal_rhs() {
    let source = r#"
class A {
    #field = 1;
    otherObject = new A();
    constructor() {
        [this.#field = 2] = [];
        [this.otherObject.#field = 2] = [];
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2493_errors: Vec<&(u32, String)> =
        diagnostics.iter().filter(|(code, _)| *code == 2493).collect();

    assert!(
        ts2493_errors.is_empty(),
        "Defaulted destructuring assignment elements should not produce TS2493. \
         Got diagnostics: {diagnostics:#?}"
    );
}
