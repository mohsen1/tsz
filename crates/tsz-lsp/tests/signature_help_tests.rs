use super::*;
use crate::jsdoc::jsdoc_for_node;
use crate::utils::find_node_at_offset;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_parser::syntax_kind_ext;
use tsz_solver::construction::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of signature_help_tests tests.
include!("signature_help_tests_parts/part_00.rs");
include!("signature_help_tests_parts/part_01.rs");
