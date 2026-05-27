//! Integration tests for solver strictness improvements.
//!
//! This module tests the comprehensive solver improvements made in SOLV-15, SOLV-18, and SOLV-19:
//! - Generic type constraints (SOLV-15): Using constraints instead of falling back to Any
//! - Tuple type subtyping (SOLV-18): Covariant tuple subtyping with proper length handling
//! - Function type variance (SOLV-19): Proper contravariance for parameter types
//!
//! These integration tests verify TS2322 and TS7006 error detection improves with strictness.

use super::*;
use crate::computation::CompatChecker;
use crate::relations::subtype::SubtypeChecker;

/// Test suite for SOLV-15: Generic type strict subtyping
#[cfg(test)]

include!("integration_tests_parts/part_00.rs");
include!("integration_tests_parts/part_01.rs");
