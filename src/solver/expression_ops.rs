//! Expression type computation operations.
//!
//! This module implements AST-agnostic type computation for expressions,
//! migrated from the Checker as part of the Solver-First architecture refactor.
//!
//! These functions operate purely on TypeIds and maintain no AST dependencies.

use crate::solver::types::{TypeId, TypeKey, LiteralValue, IntrinsicKind};
use crate::solver::TypeDatabase;

/// Computes the result type of a conditional expression: `condition ? true_branch : false_branch`.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `condition` - Type of the condition expression
/// * `true_type` - Type of the true branch (when_true)
/// * `false_type` - Type of the false branch (when_false)
///
/// # Returns
/// * If condition is definitely truthy: returns `true_type`
/// * If condition is definitely falsy: returns `false_type`
/// * Otherwise: returns union of `true_type` and `false_type`
pub fn compute_conditional_expression_type(
    interner: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    // Handle error propagation
    if condition == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if true_type == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if false_type == TypeId::ERROR {
        return TypeId::ERROR;
    }

    // Handle special type constants
    if condition == TypeId::ANY {
        // any ? A : B -> A | B
        return interner.union2(true_type, false_type);
    }
    if condition == TypeId::NEVER {
        // never ? A : B -> never (unreachable)
        return TypeId::NEVER;
    }

    // Check if condition is definitely truthy or falsy
    if is_definitely_truthy(interner, condition) {
        return true_type;
    }
    if is_definitely_falsy(interner, condition) {
        return false_type;
    }

    // If both branches are the same type, no need for union
    if true_type == false_type {
        return true_type;
    }

    // Default: return union of both branches
    interner.union2(true_type, false_type)
}

/// Computes the type of a template literal expression.
///
/// Template literals always produce string type in TypeScript.
///
/// # Arguments
/// * `_interner` - The type database/interner (unused in Phase 1)
/// * `parts` - Slice of type IDs for each template part
///
/// # Returns
/// * `TypeId::STRING` - Template literals always produce strings
pub fn compute_template_expression_type(
    _interner: &dyn TypeDatabase,
    parts: &[TypeId],
) -> TypeId {
    // Check for error propagation
    for &part in parts {
        if part == TypeId::ERROR {
            return TypeId::ERROR;
        }
        if part == TypeId::NEVER {
            return TypeId::NEVER;
        }
    }

    // For Phase 1, template literals always produce string type
    // The Checker handles type-checking each part's expression
    TypeId::STRING
}

// =============================================================================
// Helpers
// =============================================================================

/// Checks if a type is definitely truthy.
fn is_definitely_truthy(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::Boolean(true))) => true,
        Some(TypeKey::Literal(LiteralValue::String(s))) if !s.is_none() => true,
        Some(TypeKey::Literal(LiteralValue::Number(_ordered_float))) => true, // TODO: Check if zero
        Some(TypeKey::Object(_)) => true,
        Some(TypeKey::Function(_)) => true,
        _ => false,
    }
}

/// Checks if a type is definitely falsy.
fn is_definitely_falsy(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(TypeKey::Literal(LiteralValue::Boolean(false))) => true,
        Some(TypeKey::Literal(LiteralValue::String(s))) if s.is_none() => true,
        Some(TypeKey::Literal(LiteralValue::Number(_ordered_float))) => true, // TODO: Check if zero
        Some(TypeKey::Intrinsic(IntrinsicKind::Null)) => true,
        Some(TypeKey::Intrinsic(IntrinsicKind::Undefined)) => true,
        Some(TypeKey::Intrinsic(IntrinsicKind::Void)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    // Note: These are placeholder tests. Real tests need a TypeDatabase implementation.

    #[test]
    fn test_conditional_both_same() {
        // Placeholder: true ? A : A -> A
    }

    #[test]
    fn test_conditional_different_branches() {
        // Placeholder: boolean ? string : number -> string | number
    }

    #[test]
    fn test_template_always_string() {
        // Placeholder: `foo${bar}` -> string
    }
}
