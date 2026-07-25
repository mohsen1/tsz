//! Tests for JSDoc @callback rest parameter and @typedef nested property handling.

use crate::context::CheckerOptions;
use crate::test_utils::{check_js_source_diagnostics, check_source, diagnostic_codes};

/// Strict JS check (no lib contexts) so `noImplicitAny`-driven TS7006 and
/// assignability diagnostics surface for the named-reference resolution tests.
fn check_strict_js(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

/// @callback with @param {...string} should create a rest parameter accepting
/// variable string arguments. No TS2554 should be emitted for extra arguments.
#[test]
fn test_jsdoc_callback_rest_param_no_false_arity_error() {
    let source = r#"
/**
 * @callback Foo
 * @param {...string} args
 * @returns {number}
 */

/** @type {Foo} */
const x = () => 1
var res = x('a', 'b')
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for rest parameter call, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// @callback with @param {...*} (rest of any) should accept any number of arguments.
#[test]
fn test_jsdoc_callback_rest_any_param() {
    let source = r#"
/**
 * @callback Handler
 * @param {...*} args
 * @returns {void}
 */

/** @type {Handler} */
const h = function() {}
h(1, 'a', true)
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for rest any parameter call, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// @typedef with nested @property using dotted names should create nested object types.
/// @property {Object} icons followed by @property {string} icons.image32
/// should produce { icons: { image32: string } }, not { icons: any, "icons.image32": string }.
#[test]
fn test_jsdoc_typedef_nested_property() {
    let source = r#"
/** @typedef {Object} App
 * @property {string} name
 * @property {Object} icons
 * @property {string} icons.image32
 * @property {string} icons.image64
 */
var ex;

/** @type {App} */
const app = {
    name: 'name',
    icons: {
        image32: 'x.png',
        image64: 'y.png',
    }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2739 = diagnostics.iter().filter(|d| d.code == 2739).count();
    assert_eq!(
        ts2739,
        0,
        "Expected no TS2739: nested @property should create nested object, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Closure-compiler function type syntax with rest params:
/// (b: boolean, s: string, ...rest: *[]) => void should accept variadic arguments.
#[test]
fn test_jsdoc_closure_function_type_rest_param() {
    let source = r#"
/**
 * @type {(b: boolean, s: string, ...rest: *[]) => void}
 */
const foo = function (a, b) { };
foo(false, '', 1, 2, 3);
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2554 = diagnostics.iter().filter(|d| d.code == 2554).count();
    assert_eq!(
        ts2554,
        0,
        "Expected no TS2554 for Closure function type rest param, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

// ---------------------------------------------------------------------------
// Named references in `@typedef {Object}` `@property`, `@callback` params, and
// inline method/call signatures must resolve through the full reference
// resolver, not the structural-only step. A bare name (`@callback`, sibling
// `@typedef`, class/interface) previously collapsed to `any`, silently
// dropping contextual typing and assignment checks. See issue #14850.
// ---------------------------------------------------------------------------

/// `@typedef {Object} Opts` + `@property {Cb} fn` where `Cb` is a named
/// `@callback`: assigning a non-function value must report TS2322, not be
/// silently accepted (the property type must be `Cb`, not `any`).
#[test]
fn jsdoc_object_typedef_property_callback_checks_assignment() {
    let source = r#"
/**
 * @callback Cb
 * @param {number} n
 * @returns {string}
 */
/**
 * @typedef {Object} Opts
 * @property {Cb} fn
 */
/** @type {Opts} */
const o = { fn: 123 };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "Expected TS2322 assigning a number to a @callback-typed @property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Same shape, but the property value is an arrow: its parameter must be
/// contextually typed by `Cb` (no false TS7006), and the wrong return type
/// must surface a TS2322 — matching the inline-object typedef path.
#[test]
fn jsdoc_object_typedef_property_callback_contextually_types_arrow() {
    let source = r#"
/**
 * @callback Cb
 * @param {number} n
 * @returns {string}
 */
/**
 * @typedef {Object} Opts
 * @property {Cb} fn
 */
/** @type {Opts} */
const o = { fn: (n) => n };
"#;
    let diagnostics = check_strict_js(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&7006),
        "Parameter should be contextually typed by the callback (no TS7006), got: {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "Returning the number param where a string is expected should report TS2322, got: {codes:?}"
    );
}

/// The defect generalizes to *any* named alias, not just `@callback`. A
/// `@property` whose type is a sibling `@typedef {string}` must resolve to
/// that alias (here: a number assigned to a string property → TS2322).
#[test]
fn jsdoc_object_typedef_property_named_alias_resolves() {
    let source = r#"
/** @typedef {string} Name */
/**
 * @typedef {Object} Box
 * @property {Name} label
 */
/** @type {Box} */
const b = { label: 42 };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "Expected TS2322: a sibling @typedef alias @property must not collapse to any, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Anti-hardcoding: the fix is structural, not keyed on the names `Cb`/`Opts`.
/// Renaming every binder must produce the same TS2322.
#[test]
fn jsdoc_object_typedef_property_callback_renamed_binders() {
    let source = r#"
/**
 * @callback Zzz
 * @param {number} q
 * @returns {string}
 */
/**
 * @typedef {Object} Qqq
 * @property {Zzz} ww
 */
/** @type {Qqq} */
const vv = { ww: 999 };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "Renamed binders must still report TS2322, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Regression guard: the inline-object typedef form `@typedef {{ fn: Cb }}`
/// — which already worked — must keep reporting TS2322.
#[test]
fn jsdoc_inline_object_typedef_property_callback_still_checks() {
    let source = r#"
/**
 * @callback Cb
 * @param {number} n
 * @returns {string}
 */
/** @typedef {{ fn: Cb }} Opts */
/** @type {Opts} */
const o = { fn: 123 };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "Inline-object typedef form must keep checking the callback property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// A `@callback` *parameter* that references a named `@typedef` must resolve to
/// that type (so a bad property access reports TS2339), instead of the
/// parameter collapsing to `any`.
#[test]
fn jsdoc_callback_param_resolves_named_typedef() {
    let source = r#"
/** @typedef {{ id: number }} User */
/**
 * @callback Handler
 * @param {User} u
 * @returns {number}
 */
/** @type {Handler} */
const h = (u) => u.nope;
"#;
    let diagnostics = check_strict_js(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        1,
        "A @callback param typed by a named @typedef must report TS2339 on a missing property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// A *nested* `@property` (dotted name) typed by a named `@callback` must also
/// resolve through the full resolver — the nested-object builder previously
/// used the structural-only step, collapsing `nested.fn` to `any`.
#[test]
fn jsdoc_nested_property_callback_resolves() {
    let source = r#"
/**
 * @callback Cb
 * @param {number} n
 * @returns {string}
 */
/**
 * @typedef {Object} Opts
 * @property {Object} nested
 * @property {Cb} nested.fn
 */
/** @type {Opts} */
const o = { nested: { fn: 123 } };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "A nested @property typed by a named @callback must report TS2322, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// An inline method/call signature inside a `@property` whose return type is a
/// named `@typedef` must resolve that name (a mismatched returned value reports
/// TS2322), rather than collapsing the return to `void`/`any`.
#[test]
fn jsdoc_inline_method_signature_return_resolves_named_typedef() {
    let source = r#"
/** @typedef {{ id: number }} User */
/**
 * @typedef {Object} Api
 * @property {{ get(): User }} client
 */
/** @type {Api} */
const a = { client: { get: () => ({ id: "x" }) } };
"#;
    let diagnostics = check_strict_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        1,
        "An inline method signature return typed by a named @typedef must report TS2322, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}
