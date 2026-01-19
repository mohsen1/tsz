//! Optional Chaining Type Checking
//!
//! This module provides utilities for type checking optional chaining expressions (`?.`).
//!
//! ## Optional Chaining Semantics
//!
//! Optional chaining short-circuits when the base is nullish (null | undefined):
//!
//! ```typescript
//! // Property access
//! obj?.prop     // T | undefined if obj is T | null | undefined
//!
//! // Element access
//! arr?.[0]      // T | undefined if arr is T[] | null | undefined
//!
//! // Call expression
//! func?.()      // ReturnType<T> | undefined if func is T | null | undefined
//! ```
//!
//! ## Nested Chains
//!
//! Nested chains propagate undefined from any point:
//! ```typescript
//! a?.b?.c?.d    // Returns undefined if any part is nullish
//! ```
//!
//! ## Type Narrowing
//!
//! The result type is always `T | undefined` where T is the non-nullish result type.

use crate::checker::types::TypeId;
use crate::parser::thin_node::ThinNodeArena;
use crate::parser::{syntax_kind_ext, NodeIndex};
use crate::scanner::SyntaxKind;
use crate::solver::TypeStore;

/// Information about an optional chain expression
#[derive(Debug, Clone)]
pub struct OptionalChainInfo {
    /// Whether this is an optional chain (has ?. somewhere in the chain)
    pub is_optional: bool,
    /// The root expression of the chain
    pub root: NodeIndex,
    /// Whether the immediate expression is optional (has ?. directly)
    pub is_immediate_optional: bool,
}

/// Analyze optional chain information for a node
pub fn analyze_optional_chain(arena: &ThinNodeArena, idx: NodeIndex) -> OptionalChainInfo {
    let mut is_optional = false;
    let mut current = idx;
    let mut root = idx;
    let mut is_immediate_optional = false;

    // Walk up the expression chain looking for optional chaining
    loop {
        let Some(node) = arena.get(current) else {
            break;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = arena.get_access_expr(node) {
                    if current == idx && access.question_dot_token {
                        is_immediate_optional = true;
                    }
                    if access.question_dot_token {
                        is_optional = true;
                    }
                    root = access.expression;
                    current = access.expression;
                } else {
                    break;
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = arena.get_call_expr(node) {
                    // Check if this is an optional call (has OptionalChain flag)
                    // For now, we check if the callee has optional chaining
                    root = call.expression;
                    current = call.expression;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    OptionalChainInfo {
        is_optional,
        root,
        is_immediate_optional,
    }
}

/// Checks if a node is an optional chain expression
pub fn is_optional_chain(arena: &ThinNodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                access.question_dot_token
            } else {
                false
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            // Check if this call is part of an optional chain
            // For now, check the callee expression
            if let Some(call) = arena.get_call_expr(node) {
                is_optional_chain(arena, call.expression)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Gets the result type for an optional chain expression
///
/// If the expression is an optional chain and the base can be nullish,
/// the result is T | undefined
pub fn get_optional_chain_type(
    types: &mut TypeStore,
    base_type: TypeId,
    access_type: TypeId,
    is_optional: bool,
) -> TypeId {
    if !is_optional {
        return access_type;
    }

    // For optional chains, result is always T | undefined
    // unless it already includes undefined
    if access_type == TypeId::UNDEFINED {
        return TypeId::UNDEFINED;
    }

    // Check if access_type already contains undefined
    if type_contains_undefined(types, access_type) {
        return access_type;
    }

    // Create union T | undefined
    types.union(vec![access_type, TypeId::UNDEFINED])
}

/// Checks if a type contains undefined
pub fn type_contains_undefined(types: &TypeStore, type_id: TypeId) -> bool {
    use crate::solver::{IntrinsicKind, TypeKey};

    if type_id == TypeId::UNDEFINED {
        return true;
    }

    let Some(key) = types.lookup(type_id) else {
        return false;
    };

    match key {
        TypeKey::Intrinsic(IntrinsicKind::Undefined | IntrinsicKind::Void) => true,
        TypeKey::Union(members) => {
            let members = types.type_list(members);
            members.iter().any(|&m| type_contains_undefined(types, m))
        }
        _ => false,
    }
}

/// Removes null and undefined from a type for optional chain narrowing
pub fn get_non_nullish_type(types: &mut TypeStore, type_id: TypeId) -> TypeId {
    use crate::solver::{IntrinsicKind, TypeKey};

    let Some(key) = types.lookup(type_id) else {
        return type_id;
    };

    match key {
        TypeKey::Intrinsic(IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void) => {
            TypeId::NEVER
        }
        TypeKey::Union(members) => {
            let members = types.type_list(members);
            let non_nullish: Vec<TypeId> = members
                .iter()
                .filter(|&&m| !is_nullish_type(types, m))
                .copied()
                .collect();

            if non_nullish.is_empty() {
                TypeId::NEVER
            } else if non_nullish.len() == 1 {
                non_nullish[0]
            } else {
                types.union(non_nullish)
            }
        }
        _ => type_id,
    }
}

/// Checks if a type is nullish (null or undefined)
pub fn is_nullish_type(types: &TypeStore, type_id: TypeId) -> bool {
    use crate::solver::{IntrinsicKind, TypeKey};

    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return true;
    }

    let Some(key) = types.lookup(type_id) else {
        return false;
    };

    matches!(
        key,
        TypeKey::Intrinsic(IntrinsicKind::Null | IntrinsicKind::Undefined | IntrinsicKind::Void)
    )
}

/// Checks if a type can be nullish (contains null or undefined)
pub fn can_be_nullish(types: &TypeStore, type_id: TypeId) -> bool {
    use crate::solver::{IntrinsicKind, TypeKey};

    if is_nullish_type(types, type_id) {
        return true;
    }

    let Some(key) = types.lookup(type_id) else {
        return false;
    };

    if let TypeKey::Union(members) = key {
        let members = types.type_list(members);
        return members.iter().any(|&m| is_nullish_type(types, m));
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::thin_node::ThinNodeArena;

    #[test]
    fn test_optional_chain_info_creation() {
        let arena = ThinNodeArena::new();
        let info = analyze_optional_chain(&arena, NodeIndex::NONE);
        assert!(!info.is_optional);
        assert!(!info.is_immediate_optional);
    }

    #[test]
    fn test_is_optional_chain_empty() {
        let arena = ThinNodeArena::new();
        assert!(!is_optional_chain(&arena, NodeIndex::NONE));
    }
}
