//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{
    TypeId, Visibility, construction::TypeInterner, relations::relation_queries::RelationPolicy,
    types::RelationCacheKey, types::TypeData,
};

fn assignability_test_key(source: TypeId, target: TypeId, flags: u16) -> RelationCacheKey {
    RelationCacheKey::for_assignability(
        source,
        target,
        RelationPolicy::from_flags(flags).cache_config(),
    )
}

fn parse_test_source(source: &str) -> (crate::parser::ParserState, crate::parser::NodeIndex) {
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a semantically contiguous slice of checker-state tests.
include!("checker_state_tests_parts/part_00.rs");
include!("checker_state_tests_parts/part_01.rs");
include!("checker_state_tests_parts/part_02.rs");
include!("checker_state_tests_parts/part_03.rs");
include!("checker_state_tests_parts/part_04.rs");
include!("checker_state_tests_parts/part_05.rs");
include!("checker_state_tests_parts/part_06.rs");
include!("checker_state_tests_parts/part_07.rs");
include!("checker_state_tests_parts/part_08.rs");
include!("checker_state_tests_parts/part_09.rs");
include!("checker_state_tests_parts/part_10.rs");
include!("checker_state_tests_parts/part_11.rs");
include!("checker_state_tests_parts/part_12.rs");
include!("checker_state_tests_parts/part_13.rs");
include!("checker_state_tests_parts/part_14.rs");
include!("checker_state_tests_parts/part_15.rs");
include!("checker_state_tests_parts/part_16.rs");
include!("checker_state_tests_parts/part_17.rs");
include!("checker_state_tests_parts/part_18.rs");
include!("checker_state_tests_parts/part_19.rs");
