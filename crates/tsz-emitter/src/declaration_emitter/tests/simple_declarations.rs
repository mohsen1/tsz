use super::*;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// =============================================================================
// 1. Simple Declarations
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of simple_declarations tests.
include!("simple_declarations_parts/part_00.rs");
include!("simple_declarations_parts/part_01.rs");
include!("simple_declarations_parts/part_02.rs");
include!("simple_declarations_parts/part_03.rs");
include!("simple_declarations_parts/part_04.rs");
include!("simple_declarations_parts/part_05.rs");
include!("simple_declarations_parts/part_06.rs");
