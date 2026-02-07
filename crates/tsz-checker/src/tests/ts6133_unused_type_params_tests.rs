//! Tests for TS6133 unused type parameter checking.
//!
//! Verifies that type parameters are correctly detected as unused/used
//! across interfaces, functions, classes, and type aliases when
//! noUnusedParameters is enabled (type params are checked under
//! noUnusedParameters, NOT noUnusedLocals — see
//! unusedTypeParametersNotCheckedByNoUnusedLocals conformance test).

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_with_no_unused_params(source: &str) -> Vec<crate::types::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_unused_parameters = true;

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn ts6133_count(diags: &[crate::types::Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 6133).count()
}

fn ts6133_names(diags: &[crate::types::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 6133)
        .filter_map(|d| {
            // Extract name from "'X' is declared but its value is never read."
            d.message_text
                .strip_prefix("'")
                .and_then(|s| s.split("'").next())
                .map(|s| s.to_string())
        })
        .collect()
}

#[test]
fn test_interface_unused_type_param() {
    let diags = check_with_no_unused_params("interface I<T> { x: number; }");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {:?}",
        names
    );
}

#[test]
fn test_interface_used_type_param() {
    let diags = check_with_no_unused_params("interface I<T> { x: T; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {:?}",
        names
    );
}

#[test]
fn test_function_unused_type_param() {
    let diags = check_with_no_unused_params("function f<T>(): void {}");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {:?}",
        names
    );
}

#[test]
fn test_function_used_type_param() {
    let diags = check_with_no_unused_params("function f<T>(x: T): T { return x; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {:?}",
        names
    );
}

#[test]
fn test_type_alias_unused_type_param() {
    let diags = check_with_no_unused_params("type A<T> = string;");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {:?}",
        names
    );
}

#[test]
fn test_type_alias_used_type_param() {
    let diags = check_with_no_unused_params("type A<T> = T[];");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {:?}",
        names
    );
}

#[test]
fn test_class_unused_type_param() {
    let diags = check_with_no_unused_params("class C<T> { x: number = 0; }");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {:?}",
        names
    );
}

#[test]
fn test_class_used_type_param() {
    let diags = check_with_no_unused_params("class C<T> { x: T | undefined = undefined; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {:?}",
        names
    );
}

#[test]
fn test_underscore_prefixed_type_param_not_reported() {
    let diags = check_with_no_unused_params("interface I<_T> { x: number; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"_T".to_string()),
        "_T should be skipped (underscore convention), got names: {:?}",
        names
    );
}

#[test]
fn test_multiple_type_params_partial_usage() {
    let diags = check_with_no_unused_params("interface I<T, U> { x: T; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T is used, should not be reported, got names: {:?}",
        names
    );
    assert!(
        names.contains(&"U".to_string()),
        "U is unused, should be reported, got names: {:?}",
        names
    );
}

#[test]
fn test_no_unused_params_disabled_no_errors() {
    // Without noUnusedParameters, no TS6133 for type params should be emitted
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface I<T> { x: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default(); // no_unused_parameters = false

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    assert_eq!(
        ts6133_count(&checker.ctx.diagnostics),
        0,
        "No TS6133 expected when noUnusedParameters is disabled"
    );
}

#[test]
fn test_no_unused_locals_only_no_type_param_errors() {
    // With only noUnusedLocals (not noUnusedParameters), type params should NOT be checked
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function f<T>(): void {} interface I<U> { x: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_unused_locals = true; // only locals, not params

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    assert_eq!(
        ts6133_count(&checker.ctx.diagnostics),
        0,
        "No TS6133 for type params with only noUnusedLocals (not noUnusedParameters)"
    );
}

#[test]
fn test_this_parameter_not_reported_unused() {
    // `this` parameters are TypeScript type annotations, not real params.
    // They should never be flagged as unused.
    let source = r#"
class A {
    public a: number = 0;
    public method(this: A): number {
        return this.a;
    }
}
function f(this: A): number {
    return this.a;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_unused_parameters = true;
    options.no_unused_locals = true;

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    let this_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'this'"))
        .collect();
    assert!(
        this_errors.is_empty(),
        "this parameter should not be flagged as unused, got: {:?}",
        this_errors
    );
}

#[test]
fn test_using_declaration_not_reported_unused() {
    // `using` declarations always have dispose side effects,
    // so TSC never flags them as unused.
    let source = r#"using x = undefined as any;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.no_unused_locals = true;

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    let using_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'x'"))
        .collect();
    assert!(
        using_errors.is_empty(),
        "using declaration should not be flagged as unused, got: {:?}",
        using_errors
    );
}
