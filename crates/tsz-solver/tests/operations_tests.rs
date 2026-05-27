//! Tests for type operations.

use super::*;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::relations::compat::CompatChecker;
use crate::types::{CallableShape, MappedType, TypeData, Visibility};

// Split into under-cap shards to satisfy AGENTS section 19 while preserving test order.
include!("operations_tests_parts/part_00.rs");
include!("operations_tests_parts/part_01.rs");
include!("operations_tests_parts/part_02.rs");
include!("operations_tests_parts/part_03.rs");
include!("operations_tests_parts/part_04.rs");
include!("operations_tests_parts/part_05.rs");
include!("operations_tests_parts/part_06.rs");
include!("operations_tests_parts/part_07.rs");
