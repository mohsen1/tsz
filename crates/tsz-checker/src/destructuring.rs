//! Destructuring Pattern Type Checking
//!
//! This module provides type checking for destructuring patterns including:
//! - Object destructuring: const { a, b } = obj
//! - Array destructuring: const [a, b] = arr
//! - Nested destructuring: const { a: { b } } = obj
//! - Default values: const { a = 1 } = obj
//! - Rest patterns: const [a, ...rest] = arr
//! - Function parameter destructuring

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, TypeInterner};

/// Destructuring pattern type checker
pub struct DestructuringChecker<'a> {
    arena: &'a NodeArena,
    types: &'a TypeInterner,
}

impl<'a> DestructuringChecker<'a> {
    pub fn new(arena: &'a NodeArena, types: &'a TypeInterner) -> Self {
        Self { arena, types }
    }

    /// Check an object binding pattern and infer types for bindings
    pub fn check_object_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) -> DestructuringResult {
        let mut bindings = Vec::new();
        let mut errors = Vec::new();

        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return DestructuringResult { bindings, errors };
        };

        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return DestructuringResult { bindings, errors };
        }

        let Some(binding_pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return DestructuringResult { bindings, errors };
        };

        for &element_idx in &binding_pattern.elements.nodes {
            if let Some(binding) =
                self.check_binding_element(element_idx, source_type, BindingContext::Object)
            {
                bindings.push(binding);
            }
        }

        // Check for missing required properties
        if let Some(obj_shape) =
            tsz_solver::type_queries::get_object_shape(self.types, source_type)
        {
            for prop in obj_shape.properties.iter() {
                if !prop.optional {
                    let prop_name = self.types.resolve_atom(prop.name);
                    let is_bound = bindings.iter().any(|b| b.name == prop_name);
                    if !is_bound {
                        // Not an error - unbound properties are just ignored
                    }
                }
            }
        }

        DestructuringResult { bindings, errors }
    }

    /// Check an array binding pattern and infer types for bindings
    pub fn check_array_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
        source_type: TypeId,
    ) -> DestructuringResult {
        let mut bindings = Vec::new();
        let mut errors = Vec::new();

        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return DestructuringResult { bindings, errors };
        };

        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return DestructuringResult { bindings, errors };
        }

        let Some(binding_pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return DestructuringResult { bindings, errors };
        };

        let mut has_rest = false;
        for (index, &element_idx) in binding_pattern.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                // Omitted element: [, b] = arr - skip this position
                continue;
            }

            let elem_node = match self.arena.get(element_idx) {
                Some(n) => n,
                None => continue,
            };

            // Check for rest element
            if self.is_rest_element(elem_node) {
                if has_rest {
                    errors.push(DestructuringError::MultipleRestElements { pos: element_idx });
                }
                has_rest = true;

                if let Some(binding) = self.check_rest_element(element_idx, source_type, index) {
                    bindings.push(binding);
                }
            } else {
                let element_type = self.get_array_element_type(source_type, index);
                if let Some(binding) =
                    self.check_binding_element(element_idx, element_type, BindingContext::Array)
                {
                    bindings.push(binding);
                }
            }
        }

        DestructuringResult { bindings, errors }
    }

    /// Check a binding element
    fn check_binding_element(
        &self,
        element_idx: NodeIndex,
        source_type: TypeId,
        context: BindingContext,
    ) -> Option<Binding> {
        let elem_node = self.arena.get(element_idx)?;

        if elem_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            let binding_elem = self.arena.get_binding_element(elem_node)?;

            // Get the property name (for object destructuring)
            let property_name = if !binding_elem.property_name.is_none() {
                self.get_property_name(binding_elem.property_name)
            } else if context == BindingContext::Object {
                // Shorthand: { a } = obj means { a: a } = obj
                self.get_binding_name(binding_elem.name)
            } else {
                None
            };

            // Get the binding name
            let binding_name = self.get_binding_name(binding_elem.name)?;

            // Determine the bound type
            let mut bound_type = match context {
                BindingContext::Object => {
                    if let Some(ref prop) = property_name {
                        self.get_property_type(source_type, prop)
                    } else {
                        source_type
                    }
                }
                BindingContext::Array => source_type,
            };

            // Handle default value
            let has_default = !binding_elem.initializer.is_none();
            if has_default {
                // If there's a default, the type is T | undefined -> T
                // (the default is used when the property is undefined)
                bound_type = self.remove_undefined_from_type(bound_type);
            }

            // Check for nested pattern
            let nested_pattern = if self.is_binding_pattern(binding_elem.name) {
                Some(binding_elem.name)
            } else {
                None
            };

            return Some(Binding {
                name: binding_name,
                property_name,
                bound_type,
                has_default,
                default_value_idx: if has_default {
                    Some(binding_elem.initializer)
                } else {
                    None
                },
                nested_pattern,
                is_rest: false,
            });
        }

        // Handle simple identifier binding
        if elem_node.kind == SyntaxKind::Identifier as u16 {
            let name = self.get_identifier_text(element_idx)?;
            return Some(Binding {
                name: name.clone(),
                property_name: Some(name),
                bound_type: source_type,
                has_default: false,
                default_value_idx: None,
                nested_pattern: None,
                is_rest: false,
            });
        }

        None
    }

    /// Check a rest element in array destructuring
    fn check_rest_element(
        &self,
        rest_idx: NodeIndex,
        source_type: TypeId,
        start_index: usize,
    ) -> Option<Binding> {
        let rest_node = self.arena.get(rest_idx)?;

        // Get the binding from the rest element
        if rest_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            let binding_elem = self.arena.get_binding_element(rest_node)?;
            let name = self.get_binding_name(binding_elem.name)?;

            // Rest element type is the remaining elements
            let rest_type = self.get_rest_type(source_type, start_index);

            return Some(Binding {
                name,
                property_name: None,
                bound_type: rest_type,
                has_default: false,
                default_value_idx: None,
                nested_pattern: None,
                is_rest: true,
            });
        }

        None
    }

    /// Check if a node is a binding pattern (object or array)
    fn is_binding_pattern(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
    }

    /// Check if a node is a rest element
    fn is_rest_element(&self, node: &tsz_parser::parser::node::Node) -> bool {
        if node.kind == syntax_kind_ext::BINDING_ELEMENT {
            if let Some(binding) = self.arena.get_binding_element(node) {
                return binding.dot_dot_dot_token;
            }
        }
        false
    }

    /// Get the property name from a node
    fn get_property_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(idx),
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            _ => None,
        }
    }

    /// Get the binding name from a node
    fn get_binding_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(idx);
        }

        // If it's a binding pattern, we don't have a simple name
        None
    }

    /// Get identifier text
    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        let ident = self.arena.get_identifier(node)?;
        Some(ident.escaped_text.clone())
    }

    /// Get the type of a property from an object type
    fn get_property_type(&self, object_type: TypeId, property_name: &str) -> TypeId {
        // Handle intrinsic `any` type
        if object_type.is_any() {
            return TypeId::ANY;
        }

        if let Some(obj_shape) =
            tsz_solver::type_queries::get_object_shape(self.types, object_type)
        {
            let prop_atom = self.types.intern_string(property_name);
            for prop in obj_shape.properties.iter() {
                if prop.name == prop_atom {
                    return prop.type_id;
                }
            }
            // Property not found
            TypeId::UNDEFINED
        } else {
            TypeId::UNDEFINED
        }
    }

    /// Get the element type at a specific index for array/tuple types
    fn get_array_element_type(&self, array_type: TypeId, index: usize) -> TypeId {
        // Handle intrinsic `any` type
        if array_type.is_any() {
            return TypeId::ANY;
        }

        use tsz_solver::type_queries::IterableTypeKind;
        match tsz_solver::type_queries::classify_iterable_type(self.types, array_type) {
            IterableTypeKind::Array(elem_type) => elem_type,
            IterableTypeKind::Tuple(elements) => {
                if index < elements.len() {
                    elements[index].type_id
                } else {
                    TypeId::UNDEFINED
                }
            }
            IterableTypeKind::Other => TypeId::UNDEFINED,
        }
    }

    /// Get the rest type from an array/tuple starting at a given index
    fn get_rest_type(&self, source_type: TypeId, start_index: usize) -> TypeId {
        // Handle intrinsic `any` type
        if source_type.is_any() {
            return self.types.array(TypeId::ANY);
        }

        use tsz_solver::type_queries::IterableTypeKind;
        match tsz_solver::type_queries::classify_iterable_type(self.types, source_type) {
            IterableTypeKind::Array(elem_type) => self.types.array(elem_type),
            IterableTypeKind::Tuple(elements) => {
                if start_index >= elements.len() {
                    return self.types.array(TypeId::NEVER);
                }
                let rest_types: Vec<TypeId> =
                    elements[start_index..].iter().map(|e| e.type_id).collect();
                if rest_types.is_empty() {
                    self.types.array(TypeId::NEVER)
                } else {
                    let union_type = self.types.union(rest_types);
                    self.types.array(union_type)
                }
            }
            IterableTypeKind::Other => self.types.array(TypeId::UNDEFINED),
        }
    }

    /// Remove undefined from a union type (for handling default values)
    fn remove_undefined_from_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::UNDEFINED {
            return TypeId::NEVER;
        }

        use tsz_solver::type_queries::UnionMembersKind;
        if let UnionMembersKind::Union(types) =
            tsz_solver::type_queries::classify_for_union_members(self.types, type_id)
        {
            let filtered: Vec<TypeId> = types
                .iter()
                .copied()
                .filter(|&t| t != TypeId::UNDEFINED)
                .collect();
            if filtered.is_empty() {
                return TypeId::NEVER;
            }
            if filtered.len() == 1 {
                return filtered[0];
            }
            return self.types.union(filtered);
        }

        type_id
    }
}

/// Context for binding element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingContext {
    Object,
    Array,
}

/// Result of checking a destructuring pattern
#[derive(Debug, Clone)]
pub struct DestructuringResult {
    pub bindings: Vec<Binding>,
    pub errors: Vec<DestructuringError>,
}

/// A binding extracted from a destructuring pattern
#[derive(Debug, Clone)]
pub struct Binding {
    /// The local variable name being bound
    pub name: String,
    /// The property name being destructured (for object patterns)
    pub property_name: Option<String>,
    /// The inferred type of the binding
    pub bound_type: TypeId,
    /// Whether there's a default value
    pub has_default: bool,
    /// Index of the default value expression
    pub default_value_idx: Option<NodeIndex>,
    /// For nested patterns, the pattern node
    pub nested_pattern: Option<NodeIndex>,
    /// Whether this is a rest binding
    pub is_rest: bool,
}

/// Errors in destructuring patterns
#[derive(Debug, Clone)]
pub enum DestructuringError {
    /// Property doesn't exist on the source type
    PropertyNotFound {
        property_name: String,
        pos: NodeIndex,
    },
    /// Cannot destructure from non-object type
    NotDestructurable { type_id: TypeId, pos: NodeIndex },
    /// Multiple rest elements in pattern
    MultipleRestElements { pos: NodeIndex },
    /// Rest element is not last
    RestNotLast { pos: NodeIndex },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binding_context_eq() {
        assert_eq!(BindingContext::Object, BindingContext::Object);
        assert_ne!(BindingContext::Object, BindingContext::Array);
    }

    #[test]
    fn test_destructuring_checker_creation() {
        let arena = NodeArena::new();
        let types = TypeInterner::new();
        let _checker = DestructuringChecker::new(&arena, &types);
    }
}
