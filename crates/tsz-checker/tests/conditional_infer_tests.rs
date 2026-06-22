//! Tests for conditional type evaluation with infer patterns

use tsz_checker::diagnostics::Diagnostic;

fn assert_no_ts2322(source: &str, label: &str) {
    let diags = tsz_checker::test_utils::check_source_strict(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

fn check_source_strict_with_default_libs(source: &str) -> Vec<Diagnostic> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
}

#[path = "conditional_infer_tests/part_00.rs"]
mod part_00;
#[path = "conditional_infer_tests/part_01.rs"]
mod part_01;
#[path = "conditional_infer_tests/part_02.rs"]
mod part_02;
