//! Tests for TS6196 unused type parameter checking.
//!
//! Verifies that type parameters are correctly detected as unused/used
//! across interfaces, functions, classes, and type aliases when
//! noUnusedParameters is enabled (type params are checked under
//! noUnusedParameters, NOT noUnusedLocals — see
//! unusedTypeParametersNotCheckedByNoUnusedLocals conformance test).

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn ts6196_count(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 6196).count()
}

fn ts6196_names(diags: &[Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == 6196)
        .filter_map(|d| {
            // Extract name from "'X' is declared but never used."
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
    let names = ts6196_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6196 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_interface_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("interface I<T> { x: T; }");
    let names = ts6196_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_function_unused_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("function f<T>(): void {}");
    let names = ts6196_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6196 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_function_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params(
        "function f<T>(x: T): T { return x; }",
    );
    let names = ts6196_names(&diags);
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
fn test_underscore_named_imports_do_not_emit_unused_import_diagnostics() {
    let diags = tsz_checker::test_utils::check_source_no_unused_locals(
        "import { _foo, bar as _bar } from './b';\nvoid 0;\n",
    );
    let unused_codes = diags
        .iter()
        .filter(|d| d.code == 6133 || d.code == 6192)
        .map(|d| (d.code, d.message_text.clone()))
        .collect::<Vec<_>>();
    assert!(
        unused_codes.is_empty(),
        "Expected no TS6133/TS6192 for underscore-prefixed imports, got: {unused_codes:?}"
    );
}

#[test]
fn test_type_alias_unused_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("type A<T> = string;");
    let names = ts6196_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6196 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_type_alias_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params("type A<T> = T[];");
    let names = ts6196_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_class_unused_type_param() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("class C<T> { x: number = 0; }");
    let names = ts6196_names(&diags);
    assert!(
        names.contains(&"T".to_string()),
        "Expected TS6196 for unused T, got names: {names:?}"
    );
}

#[test]
fn test_class_used_type_param() {
    let diags = tsz_checker::test_utils::check_source_no_unused_params(
        "class C<T> { x: T | undefined = undefined; }",
    );
    let names = ts6196_names(&diags);
    assert!(
        !names.contains(&"T".to_string()),
        "T should not be reported as unused, got names: {names:?}"
    );
}

#[test]
fn test_underscore_prefixed_type_param_not_reported() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("interface I<_T> { x: number; }");
    let names = ts6196_names(&diags);
    assert!(
        !names.contains(&"_T".to_string()),
        "_T should be skipped (underscore convention), got names: {names:?}"
    );
}

#[test]
fn test_multiple_type_params_partial_usage() {
    let diags =
        tsz_checker::test_utils::check_source_no_unused_params("interface I<T, U> { x: T; }");
    let names = ts6196_names(&diags);
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
fn test_ts7_unused_type_parameter_codes_and_declaration_spans() {
    let cases = [
        ("function alpha<First>(): void {}", "First", "First"),
        (
            "interface Pair<Used, Spare extends string> { value: Used; }",
            "Spare",
            "Spare extends string",
        ),
        (
            "type Pick<Input> = Input extends infer Output ? string : number;",
            "Output",
            "Output",
        ),
    ];

    for (source, name, expected_span) in cases {
        let diagnostics = tsz_checker::test_utils::check_source_no_unused_params(source);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == 6196 && diagnostic.message_text.contains(name))
            .unwrap_or_else(|| panic!("expected TS6196 for {name} in {source:?}: {diagnostics:?}"));
        assert_eq!(
            diagnostic.start,
            source.find(expected_span).unwrap() as u32,
            "unexpected start for {source:?}: {diagnostic:?}"
        );
        assert_eq!(
            diagnostic.length,
            expected_span.len() as u32,
            "unexpected length for {source:?}: {diagnostic:?}"
        );
        assert_eq!(
            diagnostic.message_text,
            format!("'{name}' is declared but never used.")
        );
    }
}

#[test]
fn test_ts7_merged_type_and_value_parameter_names_keep_distinct_codes() {
    let source = "function useNone<T>(T: number) {}";
    let diagnostics = tsz_checker::test_utils::check_source_no_unused_params(source);
    let type_parameter_start = source.find("<T>").unwrap() as u32 + 1;
    let value_parameter_start = source.find("(T:").unwrap() as u32 + 1;
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 6196 && diagnostic.start == type_parameter_start),
        "type parameter must use TS6196: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == 6133 && diagnostic.start == value_parameter_start),
        "value parameter must remain TS6133: {diagnostics:?}"
    );
}

#[test]
fn test_ts7_jsdoc_template_uses_ts6196_but_value_parameter_stays_ts6133() {
    let source = "/** @template Shape */\nfunction build(value) { return 1; }";
    let diagnostics = tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            no_unused_parameters: true,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );
    let type_parameter = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 6196)
        .unwrap_or_else(|| panic!("expected TS6196 for JSDoc Shape: {diagnostics:?}"));
    assert_eq!(type_parameter.start, source.find("Shape").unwrap() as u32);
    assert_eq!(type_parameter.length, "Shape".len() as u32);
    let value_diagnostics = tsz_checker::test_utils::check_source_no_unused_params(
        "function consume(value: number) {}",
    );
    assert!(
        value_diagnostics.iter().any(|diagnostic| {
            diagnostic.code == 6133 && diagnostic.message_text.starts_with("'value'")
        }),
        "value parameters must remain TS6133: {value_diagnostics:?}"
    );
}

#[test]
fn test_no_unused_params_disabled_no_errors() {
    // Without noUnusedParameters, no TS6133 for type params should be emitted
    let diags = tsz_checker::test_utils::check_source_diagnostics("interface I<T> { x: number; }");
    assert_eq!(
        ts6196_count(&diags),
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
        ts6196_count(&diags),
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
fn test_written_setter_only_private_member_not_reported_unused() {
    // A setter without a getter is used by actual write accesses.
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
        "written setter-only private member should not be flagged as unused, got: {setter_errors:?}"
    );
}

#[test]
fn test_unwritten_setter_only_private_member_reported_unused() {
    let source = r"
export class C {
    private set value(v: string) {}
}
";
    let diags = tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_unused_locals: true,
            strict: true,
            ..Default::default()
        },
    );
    let setter_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 6133 && d.message_text.contains("'value'"))
        .collect();
    assert!(
        !setter_errors.is_empty(),
        "unwritten setter-only private member should be flagged as unused, got: {diags:?}"
    );
}
