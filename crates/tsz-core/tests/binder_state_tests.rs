//! Tests for Binder
//!
//! This module contains tests for the binder implementation, organized into sections:
//! - Basic declarations (variables, functions, classes, interfaces, etc.)
//! - Import/export binding
//! - Scope resolution and parameter binding
//! - Namespace and enum exports
//! - Symbol merging (namespace/class/function/enum merging)
//! - Scope chain traversal
//! - Module import resolution

use crate::binder::{BinderState, symbol_flags};
use crate::parser::ParserState;
fn parse_test_source(source: &str) -> (crate::parser::ParserState, crate::parser::NodeIndex) {
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// =============================================================================
// Basic Declaration Binding Tests
// =============================================================================

include!("binder_state_tests_parts/part_00.rs");
include!("binder_state_tests_parts/part_01.rs");
