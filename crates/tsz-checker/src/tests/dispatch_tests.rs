use crate::context::{CheckerOptions, ScriptTarget};
use crate::diagnostics::Diagnostic;
use crate::test_utils::{
    check_js_source_diagnostics, check_source, check_source_diagnostics, diagnostic_codes,
};
use tsz_common::checker_options::JsxMode;

fn diagnostics_with_code(diagnostics: &[Diagnostic], code: u32) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_refs_with_code<'a>(diagnostics: &[&'a Diagnostic], code: u32) -> Vec<&'a Diagnostic> {
    diagnostics
        .iter()
        .copied()
        .filter(|diagnostic| diagnostic.code == code)
        .collect()
}

fn diagnostic_count_with_code(diagnostics: &[Diagnostic], code: u32) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

fn diagnostic_messages<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

fn diagnostic_summaries(diagnostics: &[Diagnostic]) -> Vec<(u32, &str)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
        .collect()
}

fn diagnostic_code_starts(diagnostics: &[Diagnostic]) -> Vec<(u32, u32)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.start))
        .collect()
}

fn diagnostic_ref_summaries<'a>(diagnostics: &[&'a Diagnostic]) -> Vec<(u32, &'a str)> {
    diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
        .collect()
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of dispatch_tests tests.
include!("dispatch_tests_parts/part_00.rs");
include!("dispatch_tests_parts/part_01.rs");
include!("dispatch_tests_parts/part_02.rs");
