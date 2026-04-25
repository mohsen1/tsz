//! Shared emitter integration test fixtures.
//!
//! Five emitter integration test files (`comment_tests.rs`,
//! `variable_declaration_emit_tests.rs`, `optional_chaining_tests.rs`,
//! `computed_property_es5_tests.rs`, `jsx_spread_tests.rs`) repeated the same
//! parse + print boilerplate. This module centralizes those helpers so each
//! test file can `#[path]`-mount it and call `parse_source` /
//! `parse_and_print` / `parse_and_print_with_opts` /
//! `parse_and_lower_print` instead of writing the boilerplate inline.
//!
//! Cargo `[[test]]`-registered binaries cannot share modules cross-binary, so
//! each integration test file mounts this file via
//! `#[path = "test_support.rs"] mod test_support;`. The compiler will
//! deduplicate these tiny helpers within each binary.
//!
//! Rationale: workstream 8 item 4 in `docs/plan/ROADMAP.md`
//! ("Add `emit_test_support` with parser/print helpers and table-case support").

#![allow(dead_code)]

use tsz_emitter::output::printer::{PrintOptions, Printer, lower_and_print};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;

/// Default file name used by `parse_source` when callers do not need
/// file-name-sensitive behavior (e.g. `.ts` vs `.tsx`).
pub const DEFAULT_TEST_FILE_NAME: &str = "test.ts";

/// Parse a source string with the default test file name (`"test.ts"`).
/// Returns the parser (so tests can inspect the arena, diagnostics, etc.)
/// and the source-file `NodeIndex`.
pub fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    parse_source_named(DEFAULT_TEST_FILE_NAME, source)
}

/// Parse a source string with a caller-supplied file name. Used by tests
/// that exercise file-name-sensitive behavior (e.g. `.tsx` vs `.ts` for
/// JSX parsing).
pub fn parse_source_named(file_name: &str, source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

/// Parse and print with the default `PrintOptions`. The source text is wired
/// into the printer so comment preservation and single-line detection work.
///
/// Equivalent to:
/// ```ignore
/// let (parser, root) = parse_source(source);
/// let mut printer = Printer::new(&parser.arena, PrintOptions::default());
/// printer.set_source_text(source);
/// printer.print(root);
/// printer.finish().code
/// ```
pub fn parse_and_print(source: &str) -> String {
    parse_and_print_with_opts(source, PrintOptions::default())
}

/// Parse and print with caller-supplied `PrintOptions`. Source text is wired
/// into the printer so comment preservation works for the printed output.
pub fn parse_and_print_with_opts(source: &str, opts: PrintOptions) -> String {
    let (parser, root) = parse_source(source);
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

/// Parse and run the lowering pipeline through `lower_and_print`. This does
/// NOT call `set_source_text` — use this for tests that don't care about
/// comment preservation but do exercise lowering transforms (ES5, CommonJS,
/// etc.).
pub fn parse_and_lower_print(source: &str, opts: PrintOptions) -> String {
    let (parser, root) = parse_source(source);
    lower_and_print(&parser.arena, root, opts).code
}

/// Like `parse_and_print_with_opts` but uses a caller-supplied file name.
/// Required for `.tsx` integration tests where the file name drives JSX
/// parsing.
pub fn parse_and_print_named_with_opts(
    file_name: &str,
    source: &str,
    opts: PrintOptions,
) -> String {
    let (parser, root) = parse_source_named(file_name, source);
    let mut printer = Printer::new(&parser.arena, opts);
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}
