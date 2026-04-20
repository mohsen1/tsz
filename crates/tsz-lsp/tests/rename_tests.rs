use super::*;
use crate::resolver::ScopeCache;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

// -----------------------------------------------------------------------
// Original tests (preserved)
// -----------------------------------------------------------------------
include!("rename_tests_parts/part_00.rs");
include!("rename_tests_parts/part_01.rs");
include!("rename_tests_parts/part_02.rs");
include!("rename_tests_parts/part_03.rs");
include!("rename_tests_parts/part_04.rs");
include!("rename_tests_parts/part_05.rs");
include!("rename_tests_parts/part_06.rs");
include!("rename_tests_parts/part_07.rs");
include!("rename_tests_parts/part_08.rs");
include!("rename_tests_parts/part_09.rs");
include!("rename_tests_parts/part_10.rs");
