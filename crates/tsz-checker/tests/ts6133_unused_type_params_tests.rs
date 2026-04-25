//! Tests for TS6133 unused type parameter checking.
//!
//! Verifies that type parameters are correctly detected as unused/used
//! across interfaces, functions, classes, and type aliases when
//! noUnusedParameters is enabled (type params are checked under
//! noUnusedParameters, NOT noUnusedLocals — see
//! unusedTypeParametersNotCheckedByNoUnusedLocals conformance test).

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn ts6133_count(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 6133).count()
}

fn ts6133_names(diags: &[Diagnostic]) -> Vec<String> {
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
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("interface I<T> { x: number; }");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_interface_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("interface I<T> { x: T; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_function_unused_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("function f<T>(): void {}");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_function_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params(
        "function f<T>(x: T): T { return x; }",
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_all_imports_unused_emits_ts6192() {
    let diags = tsz_checker::test_utils::check_source_no_unused_locals(
        "import d, { Member as M } from './b';\nvoid 0;\n",
    );
    let ts6192_count = diags.iter().filter(|d| d.code == 6192).count();
    assert!(
        ts6192_count >= 1,
        "Expected TS6192 for fully unused import declaration, got diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_alias_unused_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("type A<T> = string;");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_type_alias_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("type A<T> = T[];");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_class_unused_type_param() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("class C<T> { x: number = 0; }");
    let names = ts6133_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6133 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_class_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params(
        "class C<T> { x: T | undefined = undefined; }",
    );
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_underscore_prefixed_type_param_not_reported() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("interface I<_T> { x: number; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"_T".to_string()),
        "_T should be skipped (underscore convention), got names: {names:?}"
    );
}

#[test]
fn test_multiple_type_params_partial_usage() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("interface I<T, U> { x: T; }");
    let names = ts6133_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T is used, should not be reported, got names: {names:?}"
    );
    assert!(
        names.contains(&"U".to_string()),
        "U is unused, should be reported, got names: {names:?}"
    );
}

#[test]
fn test_no_unused_params_disabled_no_errors() {
    // Without noUnusedParameters, no TS6133 for type params should be emitted
    let diags = tsz_checker::test_utils::check_source_diagnostics("interface I<T> { x: number; }");
    assert_eq!(
        ts6133_count(&diags),
        0,
        "No TS6133 expected when noUnusedParameters is disabled"
    );
}

#[test]
fn test_no_unused_locals_only_no_type_param_errors() {
    // With only noUnusedLocals (not noUnusedParameters), type params should NOT be checked
    let diags = tsz_checker::test_utils::check_source(
        "function f<T>(): void {} interface I<U> { x: number; }",
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    );
    assert_eq!(
        ts6133_count(&diags),
        0,
        "No TS6133 for type params with only noUnusedLocals (not noUnusedParameters)"
    );
}

#[test]
fn test_this_parameter_not_reported_unused() {
    // `this` parameters are TypeScript type annotations, not real params.
    // They should never be flagged as unused.
    let source = r"
class A {
    public a: number = 0;
    public method(this: A): number {
        return this.a;
    }
}
function f(this: A): number {
    return this.a;
}
";
    let diags = tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_parameters: true,
            no_unused_locals: true,
            ..Default::default()
        },
    );
    let this_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'this'"))
        .collect();
    assert!(
        this_errors.is_empty(),
        "this parameter should not be flagged as unused, got: {this_errors:?}"
    );
}

#[test]
fn test_using_declaration_not_reported_unused() {
    // `using` declarations always have dispose side effects,
    // so TSC never flags them as unused.
    let diags = tsz_checker::test_utils::check_source(
        "using x = undefined as any;",
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    );
    let using_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'x'"))
        .collect();
    assert!(
        using_errors.is_empty(),
        "using declaration should not be flagged as unused, got: {using_errors:?}"
    );
}

#[test]
fn test_setter_only_private_member_not_reported_unused() {
    // A setter without a getter is "used" by write accesses.
    // TSC never flags setter-only private members as unused.
    let source = r"
class Employee {
    private set p(_: number) {}

    m() {
        this.p = 0;
    }
}
";
    // Private members are checked under noUnusedLocals, not noUnusedParameters
    let diags = tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            ..Default::default()
        },
    );
    let setter_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'p'"))
        .collect();
    assert!(
        setter_errors.is_empty(),
        "setter-only private member should not be flagged as unused, got: {setter_errors:?}"
    );
}
