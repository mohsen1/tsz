use super::*;
use crate::Visibility;
use crate::construction::QueryCache;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::diagnostics::SubtypeFailureReason;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::TypeResolver;
use tsz_binder::SymbolId;

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a semantically contiguous slice of subtype relation tests.
include!("subtype_tests_parts/part_00.rs");
include!("subtype_tests_parts/part_01.rs");
include!("subtype_tests_parts/part_02.rs");
include!("subtype_tests_parts/part_03.rs");
include!("subtype_tests_parts/part_04.rs");
include!("subtype_tests_parts/part_05.rs");
include!("subtype_tests_parts/part_06.rs");
include!("subtype_tests_parts/part_07.rs");
include!("subtype_tests_parts/part_08.rs");
include!("subtype_tests_parts/part_09.rs");
include!("subtype_tests_parts/part_10.rs");
include!("subtype_tests_parts/part_11.rs");
include!("subtype_tests_parts/part_12.rs");
include!("subtype_tests_parts/part_13.rs");
include!("subtype_tests_parts/part_14.rs");
include!("subtype_tests_parts/part_15.rs");
