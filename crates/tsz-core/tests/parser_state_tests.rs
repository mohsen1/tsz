//! Tests for Parser - Cache-optimized parser using `NodeArena`.
//!
//! This module contains tests organized into sections:
//! - Basic parsing (expressions, statements, functions)
//! - Syntax constructs (classes, interfaces, generics, JSX)
//! - Error recovery and diagnostics
//! - Edge cases and performance

use crate::checker::diagnostics::diagnostic_codes;
use crate::parser::ParserState;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use std::mem::size_of;
fn parse_test_source(source: &str) -> (crate::parser::ParserState, crate::parser::NodeIndex) {
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// =============================================================================
// Basic Parsing Tests
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of parser_state_tests tests.
include!("parser_state_tests_parts/part_00.rs");
include!("parser_state_tests_parts/part_01.rs");
include!("parser_state_tests_parts/part_02.rs");
