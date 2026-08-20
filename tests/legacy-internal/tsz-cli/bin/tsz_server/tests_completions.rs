use super::*;

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of tests_completions tests.
include!("tests_completions_parts/part_00.rs");
include!("tests_completions_parts/part_01.rs");
include!("tests_completions_parts/part_02.rs");
