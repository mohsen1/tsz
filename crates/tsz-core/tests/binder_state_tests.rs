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

// =============================================================================
// Basic Declaration Binding Tests
// =============================================================================
include!("binder_state_tests_parts/part_00.rs");
include!("binder_state_tests_parts/part_01.rs");
include!("binder_state_tests_parts/part_02.rs");
include!("binder_state_tests_parts/part_03.rs");
include!("binder_state_tests_parts/part_04.rs");
include!("binder_state_tests_parts/part_05.rs");
include!("binder_state_tests_parts/part_06.rs");
