use super::*;
use crate::resolver::ScopeCache;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// -----------------------------------------------------------------------
// Original tests (preserved)
// -----------------------------------------------------------------------

include!("rename_tests_parts/part_00.rs");
include!("rename_tests_parts/part_01.rs");
