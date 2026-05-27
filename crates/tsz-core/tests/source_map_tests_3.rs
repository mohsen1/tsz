//! Source map tests - Part 3 (ES5 transforms continued)

use crate::context::emit::EmitContext;
use crate::emitter::{Printer, PrinterOptions, ScriptTarget};
use crate::lowering::LoweringPass;
use crate::parser::ParserState;
use crate::source_map_test_utils::decode_mappings;
use serde_json::Value;
fn parse_test_source(source: &str) -> (crate::parser::ParserState, crate::parser::NodeIndex) {
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of source_map_tests_3 tests.
include!("source_map_tests_3_parts/part_00.rs");
include!("source_map_tests_3_parts/part_01.rs");
include!("source_map_tests_3_parts/part_02.rs");
include!("source_map_tests_3_parts/part_03.rs");
include!("source_map_tests_3_parts/part_04.rs");
include!("source_map_tests_3_parts/part_05.rs");
include!("source_map_tests_3_parts/part_06.rs");
include!("source_map_tests_3_parts/part_07.rs");
include!("source_map_tests_3_parts/part_08.rs");
include!("source_map_tests_3_parts/part_09.rs");
