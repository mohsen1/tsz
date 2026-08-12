//! Combined parser+checker parse-health test helper.
//!
//! Split out of `test_utils` to keep that module under the file-size cap
//! (§19).

use crate::context::CheckerOptions;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check source with real parser-diagnostic wiring:
/// `has_parse_errors`/`has_syntax_parse_errors`/the position vectors are set
/// from the actual parser output, matching `tsz-cli`'s `check_file.rs`.
///
/// Every other helper in `test_utils` (`check_source`, `check_source_codes`,
/// etc.) builds a `CheckerState` with those fields left at their `false`/empty
/// defaults, so parser-only diagnostics (e.g. TS18037, emitted by
/// `parse_await_expression`) never appear in the result, and grammar checks
/// gated on `has_syntax_parse_errors` (e.g. `check_await_expression`'s TS1308,
/// suppressed by tsc's `hasParseDiagnostics`) never see a parse error and so
/// never suppress. A test built on the plain helpers can read as "tsz reports
/// TS1308" when the compiled CLI reports only the parser's TS18037 and TS1308
/// is correctly suppressed — reach for this helper instead whenever the
/// source under test can trigger a parser-emitted diagnostic.
///
/// Returns `(parser diagnostic codes, checker diagnostic codes)` separately
/// so a test can assert on each side, or combine them.
///
/// This mirrors an existing local pattern
/// (`checkers/parameter_checker.rs`'s `checker_codes_with_parse_health`) using
/// the coarse `!parse_diagnostics.is_empty()` signal rather than `tsz-cli`'s
/// `is_non_suppressing_parse_error` allowlist (unreachable from this crate) —
/// good enough for the common case of "did a parser diagnostic fire here",
/// slightly more suppressive than production for the handful of codes on that
/// allowlist (trailing commas, rest-parameter constraints, and similar).
pub fn check_source_with_parse_health(source: &str) -> (Vec<u32>, Vec<u32>) {
    check_source_with_parse_health_and_options(source, CheckerOptions::default())
}

/// [`check_source_with_parse_health`], with the checker's [`CheckerOptions`]
/// supplied by the caller instead of always defaulting — needed for
/// diagnostics gated on `strict`/`no_implicit_any` (e.g. TS7005) that also
/// need the combined parser+checker view (e.g. a diagnostic that moved to the
/// parser, like TS1155).
pub fn check_source_with_parse_health_and_options(
    source: &str,
    options: CheckerOptions,
) -> (Vec<u32>, Vec<u32>) {
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
        options,
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.has_parse_errors = !parse_diagnostics.is_empty();
    checker.ctx.has_syntax_parse_errors = !parse_diagnostics.is_empty();
    checker.ctx.syntax_parse_error_positions =
        parse_diagnostics.iter().map(|diag| diag.start).collect();
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

/// [`check_source_with_parse_health`], with both diagnostic sources combined
/// into one code list (parser codes first) for tests that just need
/// membership/count across either side.
pub fn check_source_codes_with_parse_health(source: &str) -> Vec<u32> {
    let (parse_codes, checker_codes) = check_source_with_parse_health(source);
    parse_codes.into_iter().chain(checker_codes).collect()
}

/// [`check_source_codes_with_parse_health`], with a caller-supplied
/// [`CheckerOptions`] (e.g. `strict_checker_options`).
pub fn check_source_codes_with_parse_health_and_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let (parse_codes, checker_codes) = check_source_with_parse_health_and_options(source, options);
    parse_codes.into_iter().chain(checker_codes).collect()
}
