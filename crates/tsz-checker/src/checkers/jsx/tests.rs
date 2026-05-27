//! JSX unit tests.

use crate::test_utils::{check_multi_file, check_source, check_source_diagnostics};

fn check_jsx(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_codes(source: &str) -> Vec<u32> {
    check_jsx(source).iter().map(|d| d.code).collect()
}

fn check_jsx_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_strict_codes(source: &str) -> Vec<u32> {
    check_jsx_strict(source).iter().map(|d| d.code).collect()
}

fn check_jsx_no_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: false,
        strict_null_checks: false,
        strict_function_types: false,
        strict_property_initialization: false,
        no_implicit_any: false,
        no_implicit_this: false,
        use_unknown_in_catch_variables: false,
        strict_builtin_iterator_return: false,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_no_strict_codes(source: &str) -> Vec<u32> {
    check_jsx_no_strict(source).iter().map(|d| d.code).collect()
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of tests tests.
include!("tests_parts/part_00.rs");
include!("tests_parts/part_01.rs");
include!("tests_parts/part_02.rs");
