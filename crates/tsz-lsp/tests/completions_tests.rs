use super::*;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::LibContext;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::construction::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of completions_tests tests.
include!("completions_tests_parts/part_00.rs");
include!("completions_tests_parts/part_01.rs");
include!("completions_tests_parts/part_02.rs");
