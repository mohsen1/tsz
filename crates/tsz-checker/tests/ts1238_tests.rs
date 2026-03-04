//! Tests for TS1238: Unable to resolve signature of class decorator when called as an expression.

use tsz_checker::context::CheckerOptions;

fn check_with_experimental_decorators(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        experimental_decorators: true,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn ts1238_class_used_as_decorator_emits_error() {
    // A class has construct signatures but no call signatures,
    // so using it as a decorator should emit TS1238.
    let codes = check_with_experimental_decorators(
        r#"
class Decorate { }
@Decorate
class C { }
"#,
    );
    assert!(
        codes.contains(&1238),
        "Expected TS1238 when a class (no call signatures) is used as decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_function_decorator_no_error() {
    // A function declaration has a call signature, so no TS1238 should be emitted.
    let codes = check_with_experimental_decorators(
        r#"
function decorate(target: any) { }
@decorate
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for a function decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_declared_function_decorator_no_error() {
    // Declared function has a call signature — no TS1238.
    let codes = check_with_experimental_decorators(
        r#"
declare function decorate(target: any): any;
@decorate
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for a declared function decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_for_any_type() {
    // If the decorator expression has type `any`, no TS1238 — tsc allows it.
    let codes = check_with_experimental_decorators(
        r#"
declare var dec: any;
@dec
class C { }
"#,
    );
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 for `any`-typed decorator, got: {codes:?}"
    );
}

#[test]
fn ts1238_not_emitted_without_experimental_decorators() {
    // Without experimentalDecorators, TS1238 should not be emitted.
    let options = CheckerOptions::default(); // experimental_decorators: false

    let mut parser = tsz_parser::parser::ParserState::new(
        "test.ts".to_string(),
        "class Decorate { }\n@Decorate\nclass C { }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1238),
        "Should not emit TS1238 without experimentalDecorators, got: {codes:?}"
    );
}
