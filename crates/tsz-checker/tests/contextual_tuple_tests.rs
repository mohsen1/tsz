use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_with_options;

fn check_default(source: &str) -> Vec<Diagnostic> {
    check_with_options(source, CheckerOptions::default())
}

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}
