use super::*;
use crate::parallel::residency::{MemoryPressure, ResidencyBudget};
use crate::parallel::skeleton::diff_skeletons;
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::path::Path;
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::diagnostic_codes;


// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of parallel_tests tests.
include!("parallel_tests_parts/part_00.rs");
include!("parallel_tests_parts/part_01.rs");
include!("parallel_tests_parts/part_02.rs");
include!("parallel_tests_parts/part_03.rs");
include!("parallel_tests_parts/part_04.rs");
include!("parallel_tests_parts/part_05.rs");
