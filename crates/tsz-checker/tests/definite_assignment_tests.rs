use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use tsz_common::common::ScriptTarget;
use tsz_common::diagnostics::Diagnostic;

fn full_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    crate::test_utils::check_source(source, "test.ts", options)
}

fn diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    full_diagnostics_with_options(source, options)
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

#[path = "definite_assignment_tests/part_00.rs"]
mod part_00;
#[path = "definite_assignment_tests/part_01.rs"]
mod part_01;
