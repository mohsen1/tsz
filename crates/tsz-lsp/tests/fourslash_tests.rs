//! Comprehensive fourslash-style tests for LSP features.
//!
//! These tests use the `FourslashTest` framework to declare test scenarios
//! with marker positions (`/*name*/`) and fluent assertions.

use super::fourslash::FourslashTest;

// =============================================================================
// Go-to-Definition Tests
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of fourslash_tests tests.
include!("fourslash_tests_parts/part_00.rs");
include!("fourslash_tests_parts/part_01.rs");
include!("fourslash_tests_parts/part_02.rs");
