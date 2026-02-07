//! Object Literal Type Checking
//!
//! This module provides type checking for object literal expressions including:
//! - Property type inference
//! - Excess property checking
//! - Shorthand properties
//! - Computed property names
//! - Spread properties
//! - Getter/setter inference

use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use rustc_hash::{FxHashMap, FxHashSet};

/// Object literal type checker
pub struct ObjectLiteralChecker<'a> {
    arena: &'a NodeArena,
    types: &'a TypeInterner,
}

impl<'a> ObjectLiteralChecker<'a> {
    pub fn new(arena: &'a NodeArena, types: &'a TypeInterner) -> Self {
        Self { arena, types }
    }

    /// Collect properties from an object literal expression
    pub fn collect_properties(&self, obj_literal_idx: NodeIndex) -> Vec<ObjectLiteralProperty> {
        let mut properties = Vec::new();

        let Some(literal) = self.arena.get_literal_expr_at(obj_literal_idx) else {
            return properties;
        };

        for &elem_idx in &literal.elements.nodes {
            if let Some(prop) = self.extract_property(elem_idx) {
                properties.push(prop);
            }
        }

        properties
    }

    /// Extract a property from an object literal element
    fn extract_property(&self, elem_idx: NodeIndex) -> Option<ObjectLiteralProperty> {
        let elem_node = self.arena.get(elem_idx)?;

        match elem_node.kind {
            // Regular property assignment: { x: value }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_property_assignment(elem_node)?;
                let name = self.get_property_name(prop.name)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Named(name),
                    value_idx: prop.initializer,
                    kind: PropertyKind::Regular,
                    is_optional: false,
                })
            }

            // Shorthand property: { x } (equivalent to { x: x })
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.arena.get_shorthand_property(elem_node)?;
                let name = self.get_identifier_text(prop.name)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Named(name),
                    value_idx: prop.name, // The identifier itself is the value
                    kind: PropertyKind::Shorthand,
                    is_optional: false,
                })
            }

            // Spread property: { ...other }
            k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                let spread = self.arena.get_spread(elem_node)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Spread,
                    value_idx: spread.expression,
                    kind: PropertyKind::Spread,
                    is_optional: false,
                })
            }

            // Getter: { get x() { ... } }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let accessor = self.arena.get_accessor(elem_node)?;
                let name = self.get_property_name(accessor.name)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Named(name),
                    value_idx: elem_idx, // The whole accessor node
                    kind: PropertyKind::Getter,
                    is_optional: false,
                })
            }

            // Setter: { set x(value) { ... } }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                let accessor = self.arena.get_accessor(elem_node)?;
                let name = self.get_property_name(accessor.name)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Named(name),
                    value_idx: elem_idx, // The whole accessor node
                    kind: PropertyKind::Setter,
                    is_optional: false,
                })
            }

            // Method definition: { method() { ... } }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let method = self.arena.get_method_decl(elem_node)?;
                let name = self.get_property_name(method.name)?;
                Some(ObjectLiteralProperty {
                    name: PropertyName::Named(name),
                    value_idx: elem_idx, // The whole method node
                    kind: PropertyKind::Method,
                    is_optional: false,
                })
            }

            _ => None,
        }
    }

    /// Get the property name from a property name node
    fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;

        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.get_identifier_text(name_idx)
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let lit = self.arena.get_literal(name_node)?;
                Some(lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.arena.get_literal(name_node)?;
                Some(lit.text.clone())
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                // Computed property names can't be statically determined
                None
            }
            _ => None,
        }
    }

    /// Get the text of an identifier
    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let ident = self.arena.get_identifier_at(idx)?;
        Some(ident.escaped_text.clone())
    }

    /// Check for excess properties in an object literal assigned to a type
    pub fn check_excess_properties(
        &self,
        literal_props: &[ObjectLiteralProperty],
        target_props: &FxHashSet<String>,
        has_index_signature: bool,
    ) -> Vec<ExcessPropertyError> {
        let mut errors = Vec::new();

        if has_index_signature {
            // Index signatures allow any property
            return errors;
        }

        for prop in literal_props {
            if let PropertyName::Named(name) = &prop.name {
                if !target_props.contains(name) && prop.kind != PropertyKind::Spread {
                    errors.push(ExcessPropertyError {
                        property_name: name.clone(),
                        pos: prop.value_idx,
                    });
                }
            }
        }

        errors
    }

    /// Check for duplicate properties in an object literal
    pub fn check_duplicate_properties(
        &self,
        properties: &[ObjectLiteralProperty],
    ) -> Vec<DuplicatePropertyError> {
        let mut errors = Vec::new();
        let mut seen: FxHashMap<String, NodeIndex> = FxHashMap::default();

        for prop in properties {
            if let PropertyName::Named(name) = &prop.name {
                // Getters and setters with the same name are allowed
                if prop.kind == PropertyKind::Getter || prop.kind == PropertyKind::Setter {
                    continue;
                }

                if let Some(&first_pos) = seen.get(name) {
                    errors.push(DuplicatePropertyError {
                        property_name: name.clone(),
                        first_pos,
                        duplicate_pos: prop.value_idx,
                    });
                } else {
                    seen.insert(name.clone(), prop.value_idx);
                }
            }
        }

        errors
    }
}

/// A property extracted from an object literal
#[derive(Debug, Clone)]
pub struct ObjectLiteralProperty {
    pub name: PropertyName,
    pub value_idx: NodeIndex,
    pub kind: PropertyKind,
    pub is_optional: bool,
}

/// Property name type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyName {
    /// Named property (identifier, string literal, or numeric literal)
    Named(String),
    /// Computed property name - dynamically determined
    Computed(NodeIndex),
    /// Spread property
    Spread,
}

/// Property kind in object literal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    /// Regular property: { x: value }
    Regular,
    /// Shorthand property: { x }
    Shorthand,
    /// Spread property: { ...other }
    Spread,
    /// Getter: { get x() { } }
    Getter,
    /// Setter: { set x(v) { } }
    Setter,
    /// Method: { method() { } }
    Method,
}

/// Excess property error
#[derive(Debug, Clone)]
pub struct ExcessPropertyError {
    pub property_name: String,
    pub pos: NodeIndex,
}

/// Duplicate property error
#[derive(Debug, Clone)]
pub struct DuplicatePropertyError {
    pub property_name: String,
    pub first_pos: NodeIndex,
    pub duplicate_pos: NodeIndex,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_kind_eq() {
        assert_eq!(PropertyKind::Regular, PropertyKind::Regular);
        assert_ne!(PropertyKind::Regular, PropertyKind::Spread);
        assert_eq!(PropertyKind::Getter, PropertyKind::Getter);
    }

    #[test]
    fn test_property_name_eq() {
        assert_eq!(
            PropertyName::Named("foo".to_string()),
            PropertyName::Named("foo".to_string())
        );
        assert_ne!(
            PropertyName::Named("foo".to_string()),
            PropertyName::Named("bar".to_string())
        );
        assert_eq!(PropertyName::Spread, PropertyName::Spread);
    }
}
