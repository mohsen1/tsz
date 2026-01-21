//! Nullish Coalescing Type Checking
//!
//! This module provides utilities for type checking the nullish coalescing operator (`??`).
//!
//! ## Nullish Coalescing Semantics
//!
//! The `??` operator returns its right operand when the left is nullish (null | undefined):
//!
//! ```typescript
//! const value = a ?? b;
//! // If a is T | null | undefined, result is T | typeof b
//! // where T is the non-nullish part of a's type
//! ```
//!
//! ## Type Narrowing
//!
//! The left operand is narrowed to exclude null | undefined in the true branch:
//! ```typescript
//! const x = a ?? fallback;
//! // In the expression, a is narrowed to NonNullable<typeof a>
//! ```
//!
//! ## Precedence Rules
//!
//! TypeScript requires parentheses when mixing `??` with `&&` or `||`:
//! ```typescript
//! a && b ?? c;     // Error: requires parentheses
//! (a && b) ?? c;   // OK
//! a ?? (b && c);   // OK
//! ```

use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::parser::node::NodeArena;
use crate::scanner::SyntaxKind;
use crate::solver::{TypeDatabase, TypeId as SolverTypeId};

/// Computes the result type of a nullish coalescing expression
///
/// For `left ?? right`:
/// - If left is definitely nullish -> result is right's type
/// - If left is definitely not nullish -> result is left's type
/// - If left may be nullish -> result is NonNullable<left> | right
pub fn get_nullish_coalescing_type(
    types: &mut impl TypeDatabase,
    left_type: SolverTypeId,
    right_type: SolverTypeId,
) -> SolverTypeId {
    // If left is ANY, result is ANY
    if left_type == SolverTypeId::ANY {
        return SolverTypeId::ANY;
    }

    // If left is definitely nullish, result is right's type
    if is_definitely_nullish(types, left_type) {
        return right_type;
    }

    // If left cannot be nullish, result is left's type
    if !can_be_nullish(types, left_type) {
        return left_type;
    }

    // Left may be nullish - result is NonNullable<left> | right
    let non_nullish_left = get_non_nullish_type(types, left_type);

    // If non-nullish left is same as right (or both are same concrete type),
    // just return that type
    if non_nullish_left == right_type {
        return right_type;
    }

    // Create union of non-nullish left and right
    types.union(vec![non_nullish_left, right_type])
}

/// Checks if a type is definitely nullish (only null or undefined)
fn is_definitely_nullish(types: &impl TypeDatabase, type_id: SolverTypeId) -> bool {
    use crate::solver::{IntrinsicKind, TypeKey};

    if type_id == SolverTypeId::NULL || type_id == SolverTypeId::UNDEFINED {
        return true;
    }

    let Some(key) = types.lookup(type_id) else {
        return false;
    };

    match key {
        TypeKey::Intrinsic(
            IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void,
        ) => true,
        TypeKey::Union(members) => {
            // A union is definitely nullish if all members are nullish
            let members = types.type_list(members);
            members.iter().all(|&m| is_definitely_nullish(types, m))
        }
        _ => false,
    }
}

/// Checks if a type can be nullish (contains null or undefined)
fn can_be_nullish(types: &impl TypeDatabase, type_id: SolverTypeId) -> bool {
    use crate::solver::{IntrinsicKind, TypeKey};

    if type_id == SolverTypeId::NULL || type_id == SolverTypeId::UNDEFINED {
        return true;
    }

    let Some(key) = types.lookup(type_id) else {
        return false;
    };

    match key {
        TypeKey::Intrinsic(
            IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void,
        ) => true,
        TypeKey::Union(members) => {
            let members = types.type_list(members);
            members.iter().any(|&m| can_be_nullish(types, m))
        }
        _ => false,
    }
}

/// Removes null and undefined from a type
fn get_non_nullish_type(types: &mut impl TypeDatabase, type_id: SolverTypeId) -> SolverTypeId {
    use crate::solver::{IntrinsicKind, TypeKey};

    let Some(key) = types.lookup(type_id) else {
        return type_id;
    };

    match key {
        TypeKey::Intrinsic(
            IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void,
        ) => SolverTypeId::NEVER,
        TypeKey::Union(members) => {
            let members = types.type_list(members);
            let non_nullish: Vec<SolverTypeId> = members
                .iter()
                .filter(|&&m| {
                    !matches!(
                        types.lookup(m),
                        Some(TypeKey::Intrinsic(
                            IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void
                        ))
                    ) && m != SolverTypeId::NULL
                        && m != SolverTypeId::UNDEFINED
                })
                .copied()
                .collect();

            if non_nullish.is_empty() {
                SolverTypeId::NEVER
            } else if non_nullish.len() == 1 {
                non_nullish[0]
            } else {
                types.union(non_nullish)
            }
        }
        _ => type_id,
    }
}

/// Checks for mixing ?? with && or || without parentheses
///
/// TypeScript error TS5076: "The left-hand side of a '??' cannot be
/// a '||' or '&&' expression. Consider wrapping it in parentheses."
pub fn check_nullish_coalescing_precedence(
    arena: &NodeArena,
    left_idx: NodeIndex,
) -> Option<PrecedenceError> {
    let Some(left_node) = arena.get(left_idx) else {
        return None;
    };

    if left_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
        return None;
    }

    let Some(binary) = arena.get_binary_expr(left_node) else {
        return None;
    };

    let op = binary.operator_token;
    if op == SyntaxKind::AmpersandAmpersandToken as u16 || op == SyntaxKind::BarBarToken as u16 {
        return Some(PrecedenceError {
            operator: if op == SyntaxKind::AmpersandAmpersandToken as u16 {
                "&&"
            } else {
                "||"
            },
        });
    }

    None
}

/// Error for invalid nullish coalescing precedence
#[derive(Debug)]
pub struct PrecedenceError {
    pub operator: &'static str,
}

impl PrecedenceError {
    pub fn message(&self) -> String {
        format!(
            "'{}' and '??' operations cannot be mixed without parentheses.",
            self.operator
        )
    }
}

/// Computes the result type for a nullish coalescing assignment (??=)
///
/// For `target ??= value`:
/// - The target must be a valid assignment target
/// - The result type is NonNullable<target> | value
pub fn get_nullish_assignment_type(
    types: &mut impl TypeDatabase,
    target_type: SolverTypeId,
    value_type: SolverTypeId,
) -> SolverTypeId {
    // Similar to nullish coalescing, but target must remain assignable
    get_nullish_coalescing_type(types, target_type, value_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;

    #[test]
    fn test_nullish_coalescing_with_null_left() {
        let mut types = TypeInterner::new();

        // null ?? string should be string
        let result =
            get_nullish_coalescing_type(&mut types, SolverTypeId::NULL, SolverTypeId::STRING);
        assert_eq!(result, SolverTypeId::STRING);
    }

    #[test]
    fn test_nullish_coalescing_with_undefined_left() {
        let mut types = TypeInterner::new();

        // undefined ?? number should be number
        let result =
            get_nullish_coalescing_type(&mut types, SolverTypeId::UNDEFINED, SolverTypeId::NUMBER);
        assert_eq!(result, SolverTypeId::NUMBER);
    }

    #[test]
    fn test_nullish_coalescing_non_nullish_left() {
        let mut types = TypeInterner::new();

        // string ?? number should be string (string is never nullish)
        let result =
            get_nullish_coalescing_type(&mut types, SolverTypeId::STRING, SolverTypeId::NUMBER);
        assert_eq!(result, SolverTypeId::STRING);
    }

    #[test]
    fn test_nullish_coalescing_any_left() {
        let mut types = TypeInterner::new();

        // any ?? number should be any
        let result =
            get_nullish_coalescing_type(&mut types, SolverTypeId::ANY, SolverTypeId::NUMBER);
        assert_eq!(result, SolverTypeId::ANY);
    }

    #[test]
    fn test_precedence_check() {
        let arena = NodeArena::new();
        // Test with empty node
        let result = check_nullish_coalescing_precedence(&arena, NodeIndex::NONE);
        assert!(result.is_none());
    }
}
