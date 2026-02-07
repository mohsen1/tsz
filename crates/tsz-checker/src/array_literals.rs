//! Array Literal Type Checking
//!
//! This module provides type checking for array literal expressions including:
//! - Element type inference
//! - Tuple type inference
//! - Spread element handling
//! - Contextual typing from expected types

use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{TypeId, TypeInterner};

/// Array literal type checker
pub struct ArrayLiteralChecker<'a> {
    arena: &'a NodeArena,
    types: &'a TypeInterner,
}

impl<'a> ArrayLiteralChecker<'a> {
    pub fn new(arena: &'a NodeArena, types: &'a TypeInterner) -> Self {
        Self { arena, types }
    }

    /// Collect elements from an array literal expression
    pub fn collect_elements(&self, array_literal_idx: NodeIndex) -> Vec<ArrayLiteralElement> {
        let mut elements = Vec::new();

        let Some(literal) = self.arena.get_literal_expr_at(array_literal_idx) else {
            return elements;
        };

        for (index, &elem_idx) in literal.elements.nodes.iter().enumerate() {
            elements.push(self.extract_element(elem_idx, index));
        }

        elements
    }

    /// Extract an element from an array literal
    fn extract_element(&self, elem_idx: NodeIndex, index: usize) -> ArrayLiteralElement {
        if elem_idx.is_none() {
            return ArrayLiteralElement {
                index,
                value_idx: elem_idx,
                kind: ElementKind::Omitted,
            };
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return ArrayLiteralElement {
                index,
                value_idx: elem_idx,
                kind: ElementKind::Regular,
            };
        };

        // Check for spread element: [...items]
        if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
            if let Some(spread) = self.arena.get_spread(elem_node) {
                return ArrayLiteralElement {
                    index,
                    value_idx: spread.expression,
                    kind: ElementKind::Spread,
                };
            }
        }

        ArrayLiteralElement {
            index,
            value_idx: elem_idx,
            kind: ElementKind::Regular,
        }
    }

    /// Determine if an array literal should be inferred as a tuple
    ///
    /// An array should be inferred as a tuple when:
    /// - The contextual type is a tuple
    /// - All elements have different types
    /// - The array is in a const assertion context
    pub fn should_infer_tuple(
        &self,
        elements: &[ArrayLiteralElement],
        has_tuple_context: bool,
        is_const_context: bool,
    ) -> bool {
        if has_tuple_context || is_const_context {
            return true;
        }

        // If any element is a spread, we can't easily infer a tuple
        for elem in elements {
            if elem.kind == ElementKind::Spread {
                return false;
            }
        }

        false
    }

    /// Check if an array literal has any spread elements
    pub fn has_spread_elements(&self, elements: &[ArrayLiteralElement]) -> bool {
        elements.iter().any(|e| e.kind == ElementKind::Spread)
    }

    /// Check if an array literal has any omitted elements (holes)
    pub fn has_omitted_elements(&self, elements: &[ArrayLiteralElement]) -> bool {
        elements.iter().any(|e| e.kind == ElementKind::Omitted)
    }

    /// Get the number of required (non-optional) elements in the array
    pub fn required_element_count(&self, elements: &[ArrayLiteralElement]) -> usize {
        // Count elements before the first spread or omitted element
        let mut count = 0;
        for elem in elements {
            match elem.kind {
                ElementKind::Regular => count += 1,
                ElementKind::Spread | ElementKind::Omitted => break,
            }
        }
        count
    }
}

/// An element extracted from an array literal
#[derive(Debug, Clone)]
pub struct ArrayLiteralElement {
    pub index: usize,
    pub value_idx: NodeIndex,
    pub kind: ElementKind,
}

/// Element kind in array literal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    /// Regular element
    Regular,
    /// Spread element: [...items]
    Spread,
    /// Omitted element (hole): [1, , 3]
    Omitted,
}

/// Information about an array literal's inferred type
#[derive(Debug, Clone)]
pub struct ArrayTypeInfo {
    /// Whether the array is inferred as a tuple
    pub is_tuple: bool,
    /// The element types (for tuples, one per element; for arrays, the unified type)
    pub element_types: Vec<TypeId>,
    /// Whether the array has rest elements (from spread)
    pub has_rest: bool,
    /// Whether any elements are optional (from omitted positions)
    pub has_optional: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_kind_eq() {
        assert_eq!(ElementKind::Regular, ElementKind::Regular);
        assert_ne!(ElementKind::Regular, ElementKind::Spread);
        assert_eq!(ElementKind::Omitted, ElementKind::Omitted);
    }

    #[test]
    fn test_array_literal_checker_creation() {
        let arena = NodeArena::new();
        let types = TypeInterner::new();
        let _checker = ArrayLiteralChecker::new(&arena, &types);
    }
}
