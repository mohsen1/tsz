//! Regressions for mapped-type template inference diagnostics.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check_with_no_implicit_any(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn mapped_type_without_template_type_emits_single_7039() {
    let diagnostics = check_with_no_implicit_any(
        r#"
        type MissingTemplate<T> = { [K in keyof T] };
        "#, 
    );

    assert_eq!(
        count_code(&diagnostics, 7039),
        1,
        "mapped type body with implicit template type should emit exactly one TS7039; got: {diagnostics:#?}",
    );
}

#[test]
fn mapped_type_without_template_type_emits_single_7039_for_renamed_parameter() {
    let diagnostics = check_with_no_implicit_any(
        r#"
        type MissingTemplate<T> = { [Q in keyof T] };
        "#, 
    );

    assert_eq!(
        count_code(&diagnostics, 7039),
        1,
        "mapped type rename should preserve the same TS7039 behavior; got: {diagnostics:#?}",
    );
}
