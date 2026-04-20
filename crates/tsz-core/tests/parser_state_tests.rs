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

// =============================================================================
// Basic Parsing Tests
// =============================================================================
include!("parser_state_tests_parts/part_00.rs");
include!("parser_state_tests_parts/part_01.rs");
include!("parser_state_tests_parts/part_02.rs");
include!("parser_state_tests_parts/part_03.rs");
include!("parser_state_tests_parts/part_04.rs");
include!("parser_state_tests_parts/part_05.rs");
include!("parser_state_tests_parts/part_06.rs");
include!("parser_state_tests_parts/part_07.rs");
include!("parser_state_tests_parts/part_08.rs");
include!("parser_state_tests_parts/part_09.rs");
include!("parser_state_tests_parts/part_10.rs");
include!("parser_state_tests_parts/part_11.rs");
include!("parser_state_tests_parts/part_12.rs");
include!("parser_state_tests_parts/part_13.rs");
include!("parser_state_tests_parts/part_14.rs");
include!("parser_state_tests_parts/part_15.rs");
include!("parser_state_tests_parts/part_16.rs");
include!("parser_state_tests_parts/part_17.rs");
include!("parser_state_tests_parts/part_18.rs");
include!("parser_state_tests_parts/part_19.rs");
include!("parser_state_tests_parts/part_20.rs");
include!("parser_state_tests_parts/part_21.rs");
include!("parser_state_tests_parts/part_22.rs");
include!("parser_state_tests_parts/part_23.rs");
include!("parser_state_tests_parts/part_24.rs");
