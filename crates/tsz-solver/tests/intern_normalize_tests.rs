use super::*;
use crate::def::DefId;
use crate::intern::PROPERTY_MAP_THRESHOLD;

// =========================================================================
// 1. TYPE INTERNING CORE — DEDUPLICATION
// =========================================================================

include!("intern_normalize_tests_parts/part_00.rs");
include!("intern_normalize_tests_parts/part_01.rs");
