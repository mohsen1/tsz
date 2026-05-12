//! Shared parser test fixtures.
//!
//! Six parser integration test files were each defining the same
//! `parse_source(&str) -> (ParserState, NodeIndex)` helper. This module
//! centralizes them so tests can `use crate::parser::test_fixture::parse_source;`
//! and the parsing setup stays single-source.
//!
//! Rationale: workstream 8 item 9 in `docs/plan/ROADMAP.md`
//! ("Create parser/scanner/binder/lowering fixtures").

use crate::parser::{NodeIndex, ParserState};
use tsz_common::ScriptTarget;

/// Parse a source string with the default test file name `"test.ts"`.
/// Returns the parser (so tests can inspect diagnostics, the arena, etc.)
/// and the source-file `NodeIndex`.
pub(crate) fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    parse_source_named("test.ts", source)
}

/// Parse a source string with a caller-supplied file name. Used by tests
/// that exercise file-name-sensitive behavior (e.g. `.tsx` vs `.ts`).
pub(crate) fn parse_source_named(file_name: &str, source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

/// Parse a source string under an explicit `ScriptTarget` language version,
/// using the default `"test.ts"` file name. Used by tests that exercise
/// target-version-sensitive scanner/parser behaviour (e.g. ES5 vs ES2015
/// identifier/escape rules).
pub(crate) fn parse_source_with_language_version(
    source: &str,
    target: ScriptTarget,
) -> (ParserState, NodeIndex) {
    let mut parser =
        ParserState::new_with_language_version("test.ts".to_string(), source.to_string(), target);
    let root = parser.parse_source_file();
    (parser, root)
}
