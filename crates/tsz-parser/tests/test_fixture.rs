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
