//! Shared test utilities for checker unit tests.
//!
//! Provides common parse→bind→check pipeline helpers to eliminate
//! duplicated test setup boilerplate across checker test modules.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

/// Parse, bind, and type-check a TypeScript source string, returning all diagnostics.
///
/// Uses the given `CheckerOptions` and file name. Calls `set_lib_contexts(Vec::new())`
/// so tests run without lib definitions (preventing spurious TS2318 errors).
pub fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

/// Parse, bind, and type-check a TypeScript source string with default options.
///
/// Convenience wrapper around [`check_source`] using `"test.ts"` and default options.
pub fn check_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

/// Parse, bind, and type-check a JavaScript source string.
///
/// Uses `"test.js"` filename and enables `check_js`.
pub fn check_js_source_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    )
}

/// Parse, bind, and type-check source, returning only diagnostic codes.
///
/// Convenience wrapper for tests that only inspect error codes.
pub fn check_source_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Parse, bind, and type-check source, returning `(code, message_text)` pairs.
///
/// Convenience wrapper for tests that inspect both error codes and message text.
pub fn check_source_code_messages(source: &str) -> Vec<(u32, String)> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}
