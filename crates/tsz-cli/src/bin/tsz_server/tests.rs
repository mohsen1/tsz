use super::*;

#[path = "tests_completions.rs"]
mod completions;
#[path = "tests_navigation.rs"]
mod navigation;
#[path = "tests_response_taxonomy.rs"]
mod response_taxonomy;
#[path = "tests_support.rs"]
mod support;

use support::*;

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of tests tests.
include!("tests_parts/part_00.rs");
include!("tests_parts/part_01.rs");
include!("tests_parts/part_02.rs");
include!("tests_parts/part_03.rs");
