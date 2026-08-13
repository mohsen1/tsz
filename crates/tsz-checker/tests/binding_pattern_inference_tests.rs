use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn compile_and_get_diagnostics(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn compile_js_and_get_diagnostics(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    check_source(source, "test.js", options)
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

#[test]
fn test_jsdoc_catch_variable_invalid_type_falls_back_to_catch_policy() {
    let source = r#"
function fn() {
    // @ts-ignore
    try {} catch (/** @type {number} */ err) {
        err.toLowerCase();
    }
    try {} catch (/** @type {unknown} */ { x }) {
        console.log(x);
    }
}
"#;

    let diagnostics = compile_js_and_get_diagnostics(
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            use_unknown_in_catch_variables: false,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.iter().any(|(code, message)| *code == 2339
            && message == "Property 'x' does not exist on type 'unknown'."),
        "Expected TS2339 for destructuring from unknown catch JSDoc. Actual diagnostics: {relevant:#?}"
    );
    assert!(
        !relevant
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("toLowerCase")),
        "Invalid catch JSDoc must not make err a number. Actual diagnostics: {relevant:#?}"
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

#[test]
fn test_destructuring_default_literal_mismatch_reports_initializer_type() {
    let source = r#"
function f({ s = 5 }: { s: string }) {}
let y: string;
({ y = 5 } = {});
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message.as_str())
        .collect();

    assert!(
        ts2322_messages
            .iter()
            .all(|message| message == &"Type 'number' is not assignable to type 'string'."),
        "Destructuring default diagnostics should report the initializer literal's type. \
         Actual diagnostics: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322_messages.len(),
        2,
        "Expected one TS2322 for binding and one for assignment destructuring. \
         Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn nullish_empty_object_fallback_preserves_destructuring_property_source() {
    let source = r#"
declare const yadda: { a?: number, b?: number } | undefined;
const { a, b = a } = yadda ?? {};
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let relevant: Vec<(u32, String)> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();

    assert!(
        relevant.is_empty(),
        "Nullish empty-object fallback should not collapse the destructuring source to `{{}}`. \
         Actual diagnostics: {relevant:#?}"
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
         Got {ts2345_count} TS2345 errors. All diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_parameter_destructuring_with_default_tuple_skips_ts2493() {
    // Regression test for destructuringWithLiteralInitializers2.ts:
    // Parameter `function f01([x, y] = []) {}` should NOT emit TS2493 for the
    // out-of-bounds element access into `[]`. tsc treats the binding elements
    // as implicitly any (TS7031) instead.
    let source = r#"
function f01([x, y] = []) {}
function f10([x = 0, y] = []) {}
function f11([x, y] = [1]) {}
"#;
    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let ts2493_errors: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2493)
        .collect();
    assert!(
        ts2493_errors.is_empty(),
        "Parameter destructuring with default tuple must not emit TS2493. Got: {diagnostics:#?}"
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
    let ts2493_errors: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2493)
        .collect();

    assert!(
        ts2493_errors.is_empty(),
        "Defaulted destructuring assignment elements should not produce TS2493. \
         Got diagnostics: {diagnostics:#?}"
    );
}

// Regression: arrow function used as a default value in a variable destructuring binding
// element must get its own scope, so its parameter resolves correctly and the contextual
// type (from the property type) is used to type-check the body.
#[test]
fn test_arrow_default_in_variable_destructuring_gets_scope() {
    let source = r#"
interface StringIdentity {
    stringIdentity(s: string): string;
}
// arg.length is number, not assignable to string — must produce TS2322
let { stringIdentity: id = arg => arg.length }: StringIdentity = { stringIdentity: x => x };
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for `arg.length` (number not assignable to string) in destructuring default. \
         Got diagnostics: {diagnostics:#?}"
    );
    let non_2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c != 2322).collect();
    assert!(
        non_2322.is_empty(),
        "Expected only TS2322, no other diagnostics. Got: {non_2322:#?}"
    );
}

// The same pattern in a function parameter destructuring must also work (existing behavior).
#[test]
fn test_arrow_default_in_parameter_destructuring_gets_scope() {
    let source = r#"
interface Show {
    show: (x: number) => string;
}
// v => v: number -> number, not assignable to (x: number) => string — must produce TS2322
function f({ show: showRename = v => v }: Show) {}
"#;

    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for `v => v` (number not assignable to string) in parameter destructuring default. \
         Got diagnostics: {diagnostics:#?}"
    );
}

/// In a TS file a destructuring-parameter leaf with its own empty-array
/// initializer (`{ entries = [] }`) is NOT implicit-any: its parameter type is
/// inferred from the binding pattern and tsc's `reportErrorsFromWidening` does
/// not fire for an empty-array initializer, so no TS7031 must be reported.
/// Regression for `destructuringParameterDeclaration10.ts` (the JS counterpart
/// `destructuringParameterDeclaration9.ts` DOES report — see the JS test below).
#[test]
fn no_ts7031_for_binding_leaf_with_empty_array_initializer() {
    // Binder names are varied (`entries`, `rows`, `items`, `queue`) so the fix
    // is a structural rule, not a name-scoped suppression.
    let source = r#"
export function nested({ group: { entries = [] } = {} } = {}) { entries }
export function singleLevel({ rows = [] } = {}) { rows }
export function noOuterDefault({ items = [] }) { items }
export function arrayPattern([queue = []] = []) { queue }
export function beyondDefault([head, tail = []] = [1]) { head; tail }
"#;
    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts7031: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 7031).collect();
    assert!(
        ts7031.is_empty(),
        "A binding leaf with its own `= []` initializer must not report TS7031. Got: {ts7031:#?}"
    );
}

/// The fix must not silence genuine implicit-any: a leaf with NO initializer of
/// its own (whether or not the outer pattern has an empty default) is still
/// implicit-any and must report TS7031, and a `= []` sibling on the same
/// pattern does not suppress it.
#[test]
fn ts7031_still_fires_for_uninitialized_binding_leaf_alongside_empty_array_default() {
    let source = r#"
export function mixed({ seeded = [], bare }) { seeded; bare }
export function arrayMixed([primed = [], plain] = []) { primed; plain }
"#;
    let diagnostics = compile_and_get_diagnostics(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts7031_names: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 7031)
        .map(|(_, m)| m.as_str())
        .collect();
    assert_eq!(
        ts7031_names.len(),
        2,
        "expected TS7031 only for the two uninitialized leaves (`bare`, `plain`), got {ts7031_names:#?}"
    );
    assert!(
        ts7031_names.iter().any(|m| m.contains("'bare'"))
            && ts7031_names.iter().any(|m| m.contains("'plain'")),
        "TS7031 must fire for the uninitialized leaves, not the `= []` ones. Got {ts7031_names:#?}"
    );
}

/// In a JS file with `checkJs`, tsc DOES report TS7031 for a leaf whose default
/// is an empty array literal (`{ json = [] }` → `any[]`), unlike the TS case.
/// This is the `destructuringParameterDeclaration9.ts` behavior; the fix that
/// suppresses the TS false positive must remain JS-scoped and not silence this.
#[test]
fn ts7031_fires_for_empty_array_default_leaf_in_js_checkjs() {
    let source = r#"
export function withoutAnnotation({ additionalFiles: { entries = [] } = {} } = {}) { entries }
"#;
    let diagnostics = compile_js_and_get_diagnostics(
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts7031: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 7031)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        ts7031
            .iter()
            .any(|m| m.contains("'entries'") && m.contains("any[]")),
        "JS (`checkJs`) must report TS7031 `any[]` for an empty-array default leaf. Got {ts7031:#?}"
    );
}
