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
