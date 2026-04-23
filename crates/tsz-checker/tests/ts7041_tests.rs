//! Tests for TS7041: The containing arrow function captures the global value of 'this'.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn arrow_captures_global_this() {
    // Top-level arrow function: `this` captures globalThis
    assert!(has_error_with_code("var f = () => { this.window; }", 7041));
}

#[test]
fn arrow_captures_global_this_message() {
    let diags = get_diagnostics("var f = () => { this; }");
    let msg = &diags.iter().find(|d| d.0 == 7041).unwrap().1;
    assert!(msg.contains("global value of 'this'"));
}

#[test]
fn nested_arrow_captures_global_this() {
    // Nested arrows still capture global this
    let diags = get_diagnostics("var f = () => () => { this; }");
    assert!(
        diags.iter().any(|d| d.0 == 7041),
        "Expected TS7041, got diagnostics: {diags:?}"
    );
}

#[test]
fn regular_function_no_ts7041() {
    // Regular function: should get TS2683, not TS7041
    assert!(!has_error_with_code("function f() { this; }", 7041));
    assert!(has_error_with_code("function f() { this; }", 2683));
}

#[test]
fn arrow_inside_regular_function_no_ts7041() {
    // Arrow inside regular function: `this` is captured from function scope, not global
    // Should get TS2683 on the function, not TS7041 on the arrow
    assert!(!has_error_with_code(
        "function f() { var g = () => this; }",
        7041
    ));
}

#[test]
fn arrow_in_class_property_no_ts7041() {
    // Arrow in class property initializer: `this` refers to the class instance, not global
    assert!(!has_error_with_code(
        "class A { prop = () => { this; }; }",
        7041
    ));
}

#[test]
fn arrow_in_class_static_property_no_ts7041() {
    // Static property arrow: `this` refers to the class constructor, not global
    assert!(!has_error_with_code(
        "class A { static prop = () => { this; }; }",
        7041
    ));
}

#[test]
fn arrow_in_class_property_this_is_not_any_for_assignment() {
    // `this` inside class field arrows should be class-typed, not implicit any.
    // This should report TS2322 for assigning instance `A` to `number`.
    assert!(has_error_with_code(
        "class A { x = 1; f = () => { let n: number = this; }; }",
        2322
    ));
}

#[test]
fn arrow_property_initializer_in_contextual_object_still_captures_global_this() {
    let src = r#"
interface Options<Context, Data> {
    context: Context;
    produce(this: Context): Data;
}

declare function defineOptions<Context, Data>(options: Options<Context, Data>): [Context, Data];

defineOptions({
    context: { value: 8 },
    produce: () => {
        return this;
    },
});
"#;

    assert!(has_error_with_code(src, 7041));
}

#[test]
fn typeof_this_property_in_global_arrow_reports_ts7017_and_ts7041() {
    // From typeofThis.ts: `typeof this.foo` in a top-level arrow should still
    // reuse global-this property access diagnostics, not stop after TS7041.
    let diags = get_diagnostics(
        r#"
const x = () => {
    type T = typeof this.foo;
};
"#,
    );

    assert!(
        diags.iter().any(|d| d.0 == 7041),
        "Expected TS7041 for global-capturing arrow, got diagnostics: {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.0 == 7017),
        "Expected TS7017 for missing globalThis index signature in typeof query, got diagnostics: {diags:?}"
    );
}

/// At the top level of an *external module* (a file with `import`/`export`),
/// `this` is `undefined`, not `globalThis`. TS7041 (global capture) must not
/// fire — the downstream property access produces TS2532 instead. Matches
/// tsc on `conformance/jsx/inline/inlineJsxFactoryDeclarationsLocalTypes.tsx`.
#[test]
fn arrow_at_module_top_level_does_not_report_ts7041() {
    let diags = get_diagnostics(
        r#"
export const x = 1;
const f = () => this.foo;
"#,
    );

    assert!(
        !diags.iter().any(|d| d.0 == 7041),
        "Did not expect TS7041 inside an external module — `this` is not \
         globalThis at module top-level. Got diagnostics: {diags:?}"
    );
}

/// Regression guard: a *script* (no `import`/`export`) must still report
/// TS7041 for `this` in a top-level arrow.
#[test]
fn arrow_at_script_top_level_still_reports_ts7041() {
    let diags = get_diagnostics("var f = () => { this.window; }");

    assert!(
        diags.iter().any(|d| d.0 == 7041),
        "Expected TS7041 in script context — `this` captures globalThis. \
         Got diagnostics: {diags:?}"
    );
}

/// In an external module, property access on top-level `this` produces
/// TS2532 ("Object is possibly 'undefined'."), because module top-level
/// `this` is `undefined`, not `globalThis`. Matches tsc on e.g.
/// `conformance/jsx/inline/inlineJsxFactoryDeclarationsLocalTypes.tsx`.
#[test]
fn arrow_at_module_top_level_property_access_reports_ts2532_not_ts7017() {
    let diags = get_diagnostics(
        r#"
export const x = 1;
const f = () => this.foo;
"#,
    );

    assert!(
        diags.iter().any(|d| d.0 == 2532),
        "Expected TS2532 for property access on `this: undefined` at module \
         top-level. Got diagnostics: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 7017),
        "Did not expect TS7017 — `this` is `undefined`, not `typeof globalThis` \
         in an external module. Got diagnostics: {diags:?}"
    );
}

#[test]
fn nullable_this_property_access_reports_named_ts18048() {
    // From typeofThis.ts: nullable `this` should use the named nullish diagnostic
    // rather than the generic "Object is possibly 'undefined'" form.
    let diags = get_diagnostics(
        r#"
function f(this: { foo?: string } | undefined): string | undefined {
    return this.foo;
}
"#,
    );

    assert!(
        diags.iter().any(|d| d.0 == 18048),
        "Expected TS18048 for nullable `this`, got diagnostics: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2532),
        "Did not expect generic TS2532 once `this` is named, got diagnostics: {diags:?}"
    );
}
