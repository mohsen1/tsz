//! Comprehensive tests for conditional type evaluation.
//!
//! These tests verify TypeScript's conditional type behavior:
//! - T extends U ? X : Y
//! - Distributive conditional types
//! - infer keyword
//! - Nested conditionals
//! - Conditional type constraint for subtype checking

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{ConditionalType, TypeData, TypeParamInfo};

// =============================================================================
// Basic Conditional Type Tests
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of conditional_comprehensive_tests tests.
include!("conditional_comprehensive_tests_parts/part_00.rs");
include!("conditional_comprehensive_tests_parts/part_01.rs");
include!("conditional_comprehensive_tests_parts/part_02.rs");
