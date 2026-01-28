//! Spread Operator Type Checking
//!
//! This module provides type checking for spread operations in:
//! - Array literals: [...arr]
//! - Object literals: { ...obj }
//! - Function calls: fn(...args)
//! - Array destructuring: [a, ...rest] = arr

use crate::parser::NodeIndex;
use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::solver::type_queries::{
    SpreadTypeKind, classify_spread_type, get_iterable_element_type_from_db, is_iterable_type_kind,
};
use crate::solver::{TypeId, TypeInterner};

/// Spread operator type checker
pub struct SpreadChecker<'a> {
    arena: &'a NodeArena,
    types: &'a TypeInterner,
}

impl<'a> SpreadChecker<'a> {
    pub fn new(arena: &'a NodeArena, types: &'a TypeInterner) -> Self {
        Self { arena, types }
    }

    /// Get the spread element type from a type being spread
    ///
    /// For arrays: T[] spread gives T
    /// For tuples: [A, B, C] spread gives A | B | C
    /// For iterables: Iterable<T> spread gives T
    pub fn get_spread_element_type(&self, spread_type: TypeId) -> SpreadResult {
        // Handle intrinsic types first
        if spread_type.is_any() {
            return SpreadResult::AnySpread;
        }
        if spread_type.is_unknown() {
            return SpreadResult::Error(SpreadError::NotSpreadable);
        }
        if spread_type == TypeId::STRING {
            return SpreadResult::ArraySpread {
                element_type: TypeId::STRING,
            };
        }

        // Use the classification helper to determine spread type kind
        match classify_spread_type(self.types, spread_type) {
            SpreadTypeKind::Array(element_type) => SpreadResult::ArraySpread { element_type },
            SpreadTypeKind::Tuple(tuple_list_id) => {
                let elements = self.types.tuple_list(tuple_list_id);
                if elements.is_empty() {
                    SpreadResult::EmptyTuple
                } else {
                    let element_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                    SpreadResult::TupleSpread { element_types }
                }
            }
            SpreadTypeKind::Object(_) | SpreadTypeKind::ObjectWithIndex(_) => {
                // Object spread - all properties are spread
                SpreadResult::ObjectSpread {
                    source_type: spread_type,
                }
            }
            SpreadTypeKind::StringLiteral(_) => {
                // String literals are iterable
                SpreadResult::ArraySpread {
                    element_type: TypeId::STRING,
                }
            }
            SpreadTypeKind::Other => {
                // Try to check for iterable using the solver helper
                if is_iterable_type_kind(self.types, spread_type) {
                    if let Some(iter_type) =
                        get_iterable_element_type_from_db(self.types, spread_type)
                    {
                        SpreadResult::IterableSpread {
                            element_type: iter_type,
                        }
                    } else {
                        SpreadResult::Error(SpreadError::NotSpreadable)
                    }
                } else {
                    SpreadResult::Error(SpreadError::NotSpreadable)
                }
            }
            SpreadTypeKind::NotSpreadable => SpreadResult::Error(SpreadError::NotSpreadable),
        }
    }

    /// Get the result type when spreading an object into another object
    ///
    /// { ...a, ...b } combines properties from both
    pub fn get_object_spread_type(&self, base_type: TypeId, spread_type: TypeId) -> TypeId {
        // For now, return an intersection-like combination
        // In a full implementation, we'd merge properties
        self.types.intersection(vec![base_type, spread_type])
    }

    /// Check if a type is iterable
    fn is_iterable(&self, type_id: TypeId) -> bool {
        is_iterable_type_kind(self.types, type_id)
    }

    /// Get the element type of an iterable
    fn get_iterable_element_type(&self, type_id: TypeId) -> Option<TypeId> {
        get_iterable_element_type_from_db(self.types, type_id)
    }

    /// Validate that a spread operation is valid in the current context
    pub fn validate_spread(
        &self,
        spread_idx: NodeIndex,
        context: SpreadContext,
    ) -> Vec<SpreadError> {
        let mut errors = Vec::new();

        let Some(spread_node) = self.arena.get(spread_idx) else {
            return errors;
        };

        match context {
            SpreadContext::ArrayLiteral => {
                // In array literals, spread requires an iterable
                if spread_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    // Valid position for spread
                } else {
                    errors.push(SpreadError::InvalidSpreadContext);
                }
            }
            SpreadContext::ObjectLiteral => {
                // In object literals, spread requires an object
                if spread_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT {
                    // Valid position for spread
                } else {
                    errors.push(SpreadError::InvalidSpreadContext);
                }
            }
            SpreadContext::FunctionCall => {
                // In function calls, spread requires an iterable
                if spread_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                    // Valid position for spread
                } else {
                    errors.push(SpreadError::InvalidSpreadContext);
                }
            }
            SpreadContext::Destructuring => {
                // In destructuring, spread must be the last element
                // This is validated elsewhere
            }
        }

        errors
    }
}

/// Result of analyzing a spread operation
#[derive(Debug, Clone)]
pub enum SpreadResult {
    /// Spreading an array type: [...arr] where arr: T[]
    ArraySpread { element_type: TypeId },
    /// Spreading a tuple: [...tuple] where tuple: [A, B, C]
    TupleSpread { element_types: Vec<TypeId> },
    /// Spreading an empty tuple
    EmptyTuple,
    /// Spreading an iterable
    IterableSpread { element_type: TypeId },
    /// Spreading an object
    ObjectSpread { source_type: TypeId },
    /// Spreading `any`
    AnySpread,
    /// Error in spread
    Error(SpreadError),
}

/// Context where spread is used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpreadContext {
    /// Spread in array literal: [1, ...arr, 2]
    ArrayLiteral,
    /// Spread in object literal: { ...obj }
    ObjectLiteral,
    /// Spread in function call: fn(...args)
    FunctionCall,
    /// Spread in destructuring: [a, ...rest] = arr
    Destructuring,
}

/// Errors related to spread operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpreadError {
    /// The type is not spreadable (not iterable for arrays, not object for objects)
    NotSpreadable,
    /// Spread used in invalid context
    InvalidSpreadContext,
    /// Rest element must be last in array pattern
    RestNotLast,
    /// Multiple rest elements in pattern
    MultipleRest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spread_context_eq() {
        assert_eq!(SpreadContext::ArrayLiteral, SpreadContext::ArrayLiteral);
        assert_ne!(SpreadContext::ArrayLiteral, SpreadContext::ObjectLiteral);
    }

    #[test]
    fn test_spread_error_eq() {
        assert_eq!(SpreadError::NotSpreadable, SpreadError::NotSpreadable);
        assert_ne!(SpreadError::NotSpreadable, SpreadError::RestNotLast);
    }

    #[test]
    fn test_spread_checker_creation() {
        let arena = NodeArena::new();
        let types = TypeInterner::new();
        let _checker = SpreadChecker::new(&arena, &types);
    }
}
