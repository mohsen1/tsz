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

/// Assert that no parse diagnostics were emitted for `source`.
pub(crate) fn assert_no_errors(source: &str) {
    let (parser, _) = parse_source(source);
    assert!(
        parser.get_diagnostics().is_empty(),
        "expected no parse errors for {source:?}, got {:?}",
        parser.get_diagnostics()
    );
}

/// Assert that a node of the given `kind` starts at `expected_text` within
/// `source` and its span ends exactly where `expected_text` ends — i.e. the
/// node does not overshoot into a trailing token such as `;` or `,`.
///
/// Shared by all test files that check node span boundaries.
pub(crate) fn assert_span(source: &str, kind: u16, expected_text: &str) {
    let (parser, _) = parse_source(source);
    assert_span_on(&parser, source, kind, expected_text);
}

/// Like `assert_span` but operates on an already-parsed `ParserState`, avoiding
/// a second `parse_source` call when multiple spans are checked on the same input.
pub(crate) fn assert_span_on(
    parser: &crate::parser::ParserState,
    source: &str,
    kind: u16,
    expected_text: &str,
) {
    let arena = parser.get_arena();
    let expected_start = source.find(expected_text).unwrap_or_else(|| {
        panic!("expected_text {expected_text:?} not found in source {source:?}")
    });
    let expected_end = expected_start + expected_text.len();

    let mut found = false;
    for node in &arena.nodes {
        if node.kind == kind && node.pos as usize == expected_start {
            assert_eq!(
                node.end as usize,
                expected_end,
                "span mismatch for kind={kind} in {source:?}: \
                 got [{pos}..{end}] = {got:?}, expected [{expected_start}..{expected_end}] = {expected_text:?}",
                pos = node.pos,
                end = node.end,
                got = &source[node.pos as usize..node.end as usize],
            );
            found = true;
            break;
        }
    }
    assert!(
        found,
        "no node of kind={kind} starting at offset {expected_start} found in {source:?}"
    );
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
