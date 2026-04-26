//! Tests for `@template T` declared on a JS function being in scope for
//! `@type {T}` annotations inside the function body.
//!
//! Conformance test
//! `TypeScript/tests/cases/conformance/jsdoc/jsdocTemplateConstructorFunction.ts`
//! exercises this exact pattern. Before this fix, the signature builder
//! pushed JSDoc-derived `@template T` into `type_parameter_scope` only for
//! signature construction and popped it before the body walk — so an
//! `@type {T}` annotation on a property assignment inside the function body
//! resolved against an empty scope and produced a false-positive TS2304
//! ("Cannot find name 'T'"). The fix re-pushes the same names around the
//! body check in `check_function_body`.

use tsz_checker::context::CheckerOptions;

fn check_js_with_jsdoc(source: &str) -> Vec<(u32, String)> {
    let mut parser = tsz_parser::parser::ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = tsz_solver::TypeInterner::new();
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "a.js".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn jsdoc_template_param_visible_in_body_jsdoc_type_annotation() {
    // `@template T` declared on `Zet`, then `@type {T}` referenced on
    // `this.u` inside the body. Before the fix, T was popped from
    // `type_parameter_scope` after signature construction, so resolving
    // `T` for the body annotation failed and tsz emitted TS2304.
    let source = r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    /** @type {T} */
    this.u
    this.t = t
}
"#;
    let diagnostics = check_js_with_jsdoc(source);
    let ts2304_for_t: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2304 && msg.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for @template T inside its function body, got: {diagnostics:?}",
    );
}

#[test]
fn jsdoc_template_param_visible_via_type_cast_in_body() {
    // Inline JSDoc cast `/** @type {T} */(expr)` inside the function body
    // must also resolve T against the enclosing function's @template scope.
    let source = r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    var x = /** @type {T} */ (t)
}
"#;
    let diagnostics = check_js_with_jsdoc(source);
    let ts2304_for_t: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2304 && msg.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for @template T inline cast, got: {diagnostics:?}",
    );
}

#[test]
fn jsdoc_template_param_does_not_emit_unrelated_diagnostics() {
    // Sanity: the fix does not regress a basic JS function with `@template T`
    // — the body checker should produce no TS2304 for the declared T and no
    // unrelated diagnostics for the well-typed body.
    let source = r#"
/**
 * @template T
 * @param {T} t
 * @returns {T}
 */
function id(t) {
    /** @type {T} */
    var x = t
    return x
}
"#;
    let diagnostics = check_js_with_jsdoc(source);
    let ts2304_for_t: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2304 && msg.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for @template T in body, got: {diagnostics:?}",
    );
}

/// Regression for `jsdocTemplateClass.ts`: a JS class with class-level
/// `@template T` plus a method whose JSDoc `@param` references a separately
/// declared `@typedef Id<T>` (a generic typedef applied with `T`) used to
/// emit a false-positive TS2304 for the `T` argument inside `Id<T>`.
///
/// Root cause: JSDoc generic-instantiation validation
/// (`report_jsdoc_simple_generic_instantiation_errors`) only consulted
/// inline `<T, ...>` template params parsed from the JSDoc type expression
/// itself; it did not see the enclosing class's `@template T`.
///
/// Fix: (1) push the enclosing class's `@template` JSDoc-derived type
/// parameters into `type_parameter_scope` for the duration of class-member
/// checking AND `get_type_of_function_impl` (covering both the
/// `build_type_environment` pre-walk and the per-member check passes),
/// and (2) make the JSDoc validator skip arg names that exist in
/// `type_parameter_scope`.
#[test]
fn jsdoc_class_template_param_visible_in_method_param_generic_application() {
    let source = r#"
/**
 * @template T
 * @typedef {(t: T) => T} Id
 */
/** @template T */
class Foo {
    /** @param {T} x */
    constructor (x) { this.a = x; }
    /**
     * @param {T} x
     * @param {Id<T>} y
     * @return {T}
     */
    foo(x, y) { return y(x); }
}
"#;
    let diagnostics = check_js_with_jsdoc(source);
    let ts2304_for_t: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2304 && msg.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "expected no TS2304 for @template T inside class-method @param {{Id<T>}}, got: {diagnostics:?}",
    );
}

/// Sanity: the fix's scope is narrow. A JS class `@template T` resolves for
/// arg positions inside method-param JSDoc generics, but a NON-template
/// argument that is NOT a known typedef and IS validated by the JSDoc
/// generic-instantiation pass still errors.
#[test]
fn jsdoc_class_template_only_suppresses_known_template_names() {
    // ZetX is a non-generic typedef; using it as `ZetX<T>` (instantiating a
    // non-generic typedef) should still produce TS_TYPE_IS_NOT_GENERIC at
    // the base name. The class-level T is in scope so the `T` argument is
    // not flagged, but the malformed instantiation is still caught.
    let source = r#"
/** @typedef {string} ZetX */
/** @template T */
class Foo {
    /**
     * @param {T} x
     * @param {ZetX<T>} y
     */
    foo(x, y) { return x; }
}
"#;
    let diagnostics = check_js_with_jsdoc(source);
    let ts2304_for_t: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2304 && msg.contains("'T'"))
        .collect();
    assert!(
        ts2304_for_t.is_empty(),
        "@template T must remain in scope; got TS2304: {diagnostics:?}",
    );
    let ts_not_generic: Vec<&(u32, String)> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2315)
        .collect();
    assert!(
        !ts_not_generic.is_empty(),
        "expected TS2315 for `ZetX<T>` since ZetX is non-generic; got: {diagnostics:?}",
    );
}
