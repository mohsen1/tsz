use super::*;
use crate::construction::TypeDatabase;
use crate::construction::TypeInterner;
use crate::def::resolver::TypeResolver;
use crate::types::SymbolRef;

// =============================================================================
// Discriminant Detection Tests
// =============================================================================

include!("narrowing_tests_parts/part_00.rs");
include!("narrowing_tests_parts/part_01.rs");
