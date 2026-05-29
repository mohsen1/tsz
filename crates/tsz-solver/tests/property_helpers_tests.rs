//! Comprehensive tests for property access resolution helpers.
//!
//! Covers property lookup on object types, union types, intersection types,
//! index signature access, optional property handling, missing property detection,
//! readonly properties, primitive property access, array property access, and more.

use crate::intern::TypeInterner;
use crate::operations::expression_ops::normalize_object_union_members_for_write_target;
use crate::operations::property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::types::*;

// =============================================================================
// Helpers
// =============================================================================

fn assert_property_success(result: &PropertyAccessResult, expected: TypeId) {
    match result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(
            *type_id, expected,
            "Expected Success with type {expected:?}, got type {type_id:?}"
        ),
        PropertyAccessResult::PropertyNotFound {
            property_name,
            type_id,
        } => {
            panic!(
                "Expected Success({expected:?}), got PropertyNotFound(type={type_id:?}, prop={property_name:?})"
            )
        }
        PropertyAccessResult::PossiblyNullOrUndefined { cause, .. } => {
            panic!("Expected Success({expected:?}), got PossiblyNullOrUndefined(cause={cause:?})")
        }
        PropertyAccessResult::IsUnknown => {
            panic!("Expected Success({expected:?}), got IsUnknown")
        }
    }
}

fn assert_property_not_found(result: &PropertyAccessResult) {
    assert!(
        matches!(result, PropertyAccessResult::PropertyNotFound { .. }),
        "Expected PropertyNotFound, got {result:?}"
    );
}

fn assert_possibly_null_or_undefined(result: &PropertyAccessResult) {
    assert!(
        matches!(result, PropertyAccessResult::PossiblyNullOrUndefined { .. }),
        "Expected PossiblyNullOrUndefined, got {result:?}"
    );
}

// =============================================================================
// Property lookup on simple object types
// =============================================================================

include!("property_helpers_tests_parts/part_00.rs");
include!("property_helpers_tests_parts/part_01.rs");
