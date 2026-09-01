//! Grammar-only parse-health test helper.
//!
//! Split out of `test_utils` to keep that module under the file-size cap
//! (§19). See [`check_source_with_grammar_only_parse_health`] for why this
//! needs to be distinct from `test_utils::check_source_with_parse_health`.

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Like `check_source_with_parse_health`, but for sources whose only parser
/// diagnostics are grammar checks (e.g. TS1029 modifier-order), not genuine
/// structural parse failures.
///
/// `tsz-cli`'s real `has_parse_errors` wiring (`program_has_real_syntax_errors`,
/// filtered through `is_real_syntax_error`) only flips true for codes that
/// indicate the parser actually recovered from broken syntax — TS1029 and its
/// modifier-grammar siblings are deliberately excluded there, because the AST
/// they produce is fully valid. `check_source_with_parse_health`'s coarse
/// `!parse_diagnostics.is_empty()` cannot make that distinction and would set
/// `has_parse_errors = true` for a grammar-only diagnostic too, silently
/// over-suppressing anything gated on it and hiding a real production bug
/// behind a false-negative test. This helper leaves `has_parse_errors` /
/// `has_syntax_parse_errors` at their `false` default while still populating
/// `all_parse_error_positions`, matching production's actual split for these
/// sources.
pub fn check_source_with_grammar_only_parse_health(source: &str) -> (Vec<u32>, Vec<u32>) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();
    let parse_diagnostics = parser.get_diagnostics().to_vec();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.all_parse_error_positions =
        parse_diagnostics.iter().map(|diag| diag.start).collect();
    checker.check_source_file(source_file);

    let parse_codes = parse_diagnostics.iter().map(|diag| diag.code).collect();
    let checker_codes = checker
        .ctx
        .diagnostics
        .iter()
        .map(|diag| diag.code)
        .collect();
    (parse_codes, checker_codes)
}

/// [`check_source_with_grammar_only_parse_health`], codes combined (parser
/// codes first) for membership-only assertions.
pub fn check_source_codes_with_grammar_only_parse_health(source: &str) -> Vec<u32> {
    let (parse_codes, checker_codes) = check_source_with_grammar_only_parse_health(source);
    parse_codes.into_iter().chain(checker_codes).collect()
}
