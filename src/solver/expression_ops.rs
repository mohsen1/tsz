//! Expression type computation operations.
//!
//! This module implements AST-agnostic type computation for expressions,
//! migrated from the Checker as part of the Solver-First architecture refactor.
//!
//! These functions operate purely on TypeIds and maintain no AST dependencies.

use crate::solver::TypeDatabase;
use crate::solver::is_subtype_of;
use crate::solver::types::{IntrinsicKind, LiteralValue, TypeId, TypeKey};

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
pub fn compute_template_expression_type(_interner: &dyn TypeDatabase, parts: &[TypeId]) -> TypeId {
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

/// Computes the best common type (BCT) of a set of types.
///
/// This is used for array literal type inference and other contexts
/// where a single type must be inferred from multiple candidates.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `types` - Slice of type IDs to find the best common type of
/// * `resolver` - Optional TypeResolver for nominal hierarchy lookups (class inheritance)
///
/// # Returns
/// * Empty slice: Returns `TypeId::NEVER`
/// * Single type: Returns that type
/// * All same type: Returns that type
/// * Otherwise: Returns union of all types (or common base class if available)
///
/// # Note
/// When `resolver` is provided, this implements the full TypeScript BCT algorithm:
/// - Find the first candidate that is a supertype of all others
/// - Handle literal widening (via TypeChecker's pre-widening)
/// - Handle base class relationships (Dog + Cat -> Animal)
pub fn compute_best_common_type(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&dyn crate::solver::TypeResolver>,
) -> TypeId {
    // Handle empty cases
    if types.is_empty() {
        return TypeId::NEVER;
    }

    // Propagate errors
    for &ty in types {
        if ty == TypeId::ERROR {
            return TypeId::ERROR;
        }
    }

    // Single type: return it directly
    if types.len() == 1 {
        return types[0];
    }

    // If all types are the same, no need for union
    let first = types[0];
    if types.iter().all(|&ty| ty == first) {
        return first;
    }

    // Try to find common base class for nominal types (e.g., Dog + Cat -> Animal)
    if let Some(r) = resolver {
        // Collect candidate base types from the first type
        let mut base_candidates = get_type_hierarchy(interner, r, types[0]);

        // Filter candidates by checking if all other types are subtypes
        for &ty in types.iter().skip(1) {
            if base_candidates.is_empty() {
                break; // No candidates left
            }
            base_candidates.retain(|&base| is_subtype_of(interner, ty, base));
        }

        // Return the most specific common base (first candidate after filtering)
        if let Some(common_base) = base_candidates.first() {
            return *common_base;
        }
    }

    // Phase 1: Default to union of all types
    interner.union(types.to_vec())
}

/// Get the type hierarchy for a type, from most derived to most base.
/// Returns empty vec if the type is not a class/interface type.
fn get_type_hierarchy(
    interner: &dyn TypeDatabase,
    resolver: &dyn crate::solver::TypeResolver,
    ty: TypeId,
) -> Vec<TypeId> {
    let mut hierarchy = Vec::new();
    collect_type_hierarchy(interner, resolver, ty, &mut hierarchy);
    hierarchy
}

/// Recursively collect the type hierarchy for a class/interface.
fn collect_type_hierarchy(
    interner: &dyn TypeDatabase,
    resolver: &dyn crate::solver::TypeResolver,
    ty: TypeId,
    hierarchy: &mut Vec<TypeId>,
) {
    // Prevent infinite recursion
    if hierarchy.contains(&ty) {
        return;
    }

    // Add current type to hierarchy
    hierarchy.push(ty);

    // Get base type from resolver (for class/interface types)
    let base = resolver.get_base_type(ty, interner);

    if let Some(base_type) = base {
        collect_type_hierarchy(interner, resolver, base_type, hierarchy);
    }
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
    use super::*;
    use crate::solver::intern::TypeInterner;

    // =========================================================================
    // Conditional Expression Tests
    // =========================================================================

    #[test]
    fn test_conditional_both_same() {
        let interner = TypeInterner::new();
        // string ? string : string -> string
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::BOOLEAN,
            TypeId::STRING,
            TypeId::STRING,
        );
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_conditional_different_branches() {
        let interner = TypeInterner::new();
        // boolean ? string : number -> string | number
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::BOOLEAN,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        // Result should be a union type (not equal to either branch)
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_conditional_error_propagation() {
        let interner = TypeInterner::new();
        // ERROR ? string : number -> ERROR
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::ERROR,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::ERROR);

        // boolean ? ERROR : number -> ERROR
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::BOOLEAN,
            TypeId::ERROR,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::ERROR);
    }

    #[test]
    fn test_conditional_any_condition() {
        let interner = TypeInterner::new();
        // any ? string : number -> string | number
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::ANY,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        // Result should be a union type
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_conditional_never_condition() {
        let interner = TypeInterner::new();
        // never ? string : number -> never
        let result = compute_conditional_expression_type(
            &interner,
            TypeId::NEVER,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::NEVER);
    }

    #[test]
    fn test_conditional_truthy_condition() {
        let interner = TypeInterner::new();
        // true ? string : number -> string
        let true_type = interner.literal_boolean(true);
        let result = compute_conditional_expression_type(
            &interner,
            true_type,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_conditional_falsy_condition() {
        let interner = TypeInterner::new();
        // false ? string : number -> number
        let false_type = interner.literal_boolean(false);
        let result = compute_conditional_expression_type(
            &interner,
            false_type,
            TypeId::STRING,
            TypeId::NUMBER,
        );
        assert_eq!(result, TypeId::NUMBER);
    }

    // =========================================================================
    // Template Expression Tests
    // =========================================================================

    #[test]
    fn test_template_always_string() {
        let interner = TypeInterner::new();
        // `foo${bar}` -> string
        let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::NUMBER]);
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_template_empty() {
        let interner = TypeInterner::new();
        // `` -> string
        let result = compute_template_expression_type(&interner, &[]);
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_template_error_propagation() {
        let interner = TypeInterner::new();
        // `foo${ERROR}` -> ERROR
        let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::ERROR]);
        assert_eq!(result, TypeId::ERROR);
    }

    #[test]
    fn test_template_never_propagation() {
        let interner = TypeInterner::new();
        // `foo${never}` -> never
        let result = compute_template_expression_type(&interner, &[TypeId::STRING, TypeId::NEVER]);
        assert_eq!(result, TypeId::NEVER);
    }

    // =========================================================================
    // Best Common Type Tests
    // =========================================================================

    #[test]
    fn test_bct_empty() {
        let interner = TypeInterner::new();
        // BCT of empty set -> never
        let result = compute_best_common_type(&interner, &[], None);
        assert_eq!(result, TypeId::NEVER);
    }

    #[test]
    fn test_bct_single() {
        let interner = TypeInterner::new();
        // BCT of [string] -> string
        let result = compute_best_common_type(&interner, &[TypeId::STRING], None);
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_bct_all_same() {
        let interner = TypeInterner::new();
        // BCT of [string, string, string] -> string
        let result = compute_best_common_type(
            &interner,
            &[TypeId::STRING, TypeId::STRING, TypeId::STRING],
            None,
        );
        assert_eq!(result, TypeId::STRING);
    }

    #[test]
    fn test_bct_different() {
        let interner = TypeInterner::new();
        // BCT of [string, number] -> string | number
        let result = compute_best_common_type(&interner, &[TypeId::STRING, TypeId::NUMBER], None);
        // Result should be a union type (not equal to either input)
        assert_ne!(result, TypeId::STRING);
        assert_ne!(result, TypeId::NUMBER);
    }

    #[test]
    fn test_bct_error_propagation() {
        let interner = TypeInterner::new();
        // BCT of [string, ERROR, number] -> ERROR
        let result = compute_best_common_type(
            &interner,
            &[TypeId::STRING, TypeId::ERROR, TypeId::NUMBER],
            None,
        );
        assert_eq!(result, TypeId::ERROR);
    }
}
