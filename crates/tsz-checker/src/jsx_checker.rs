//! JSX Type Checking Module
//!
//! This module contains JSX type checking methods for `CheckerState`
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - JSX opening element type resolution
//! - JSX namespace type lookup
//! - JSX intrinsic elements type lookup
//! - JSX element type for fragments
//! - JSX attribute type checking (TS2322 for type mismatches)
//!
//! This implements Rule #36: JSX type checking with case-sensitive tag lookup.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

// =============================================================================
// JSX Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // JSX Opening Element Type
    // =========================================================================

    /// Get the type of a JSX opening element.
    ///
    /// Rule #36 (JSX Intrinsic Lookup): This implements the case-sensitive tag lookup:
    /// - Lowercase tags (e.g., `<div>`) look up `JSX.IntrinsicElements['div']`
    /// - Uppercase tags (e.g., `<MyComponent>`) resolve as variable expressions
    pub(crate) fn get_type_of_jsx_opening_element(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ANY;
        };

        // Get JSX opening data (works for both JSX_OPENING_ELEMENT and JSX_SELF_CLOSING_ELEMENT)
        let Some(jsx_opening) = self.ctx.arena.get_jsx_opening(node) else {
            return TypeId::ANY;
        };

        // Get the tag name
        let tag_name_idx = jsx_opening.tag_name;
        let Some(tag_name_node) = self.ctx.arena.get(tag_name_idx) else {
            return TypeId::ANY;
        };

        // Get tag name text
        let tag_name = if tag_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(tag_name_node)
                .map(|id| id.escaped_text.as_str())
        } else {
            // Property access expression (e.g., React.Component)
            None
        };

        // Determine if this is an intrinsic element (lowercase first char)
        let is_intrinsic = tag_name
            .as_ref()
            .is_some_and(|name| name.chars().next().is_some_and(|c| c.is_ascii_lowercase()));

        if is_intrinsic {
            // Intrinsic elements: look up JSX.IntrinsicElements[tagName]
            // Try to resolve JSX.IntrinsicElements and create an indexed access type
            if let Some(tag) = tag_name
                && let Some(intrinsic_elements_type) = self.get_intrinsic_elements_type()
            {
                // Create JSX.IntrinsicElements['tagName'] as an IndexAccess type
                let factory = self.ctx.types.factory();
                let tag_literal = factory.literal_string(tag);
                let props_type = factory.index_access(intrinsic_elements_type, tag_literal);

                // Check JSX attributes against the expected props type
                self.check_jsx_attributes_against_props(jsx_opening.attributes, props_type);

                return props_type;
            }
            // TS7026: JSX element implicitly has type 'any' because no interface 'JSX.IntrinsicElements' exists.
            // Only report when noImplicitAny is enabled (TS7026 is an implicit-any diagnostic)
            if self.ctx.no_implicit_any() {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    idx,
                    diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                    &["IntrinsicElements"],
                );
            }
            TypeId::ANY
        } else {
            // Component: resolve as variable expression
            // The tag name is a reference to a component (function or class)
            self.compute_type_of_node(tag_name_idx)
        }
    }

    // =========================================================================
    // JSX Namespace Type
    // =========================================================================

    /// Get the global JSX namespace type.
    ///
    /// Rule #36: Resolves the global `JSX` namespace which contains type definitions
    /// for intrinsic elements and the Element type.
    pub(crate) fn get_jsx_namespace_type(&mut self) -> Option<SymbolId> {
        // First try file_locals (includes user-defined globals and merged lib symbols)
        if let Some(sym_id) = self.ctx.binder.file_locals.get("JSX") {
            return Some(sym_id);
        }

        // Then try using get_global_type to check lib binders
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs("JSX", &lib_binders)
        {
            return Some(sym_id);
        }

        None
    }

    // =========================================================================
    // JSX Intrinsic Elements Type
    // =========================================================================

    /// Get the JSX.IntrinsicElements interface type.
    ///
    /// Rule #36: Resolves `JSX.IntrinsicElements` which maps tag names to their prop types.
    /// Returns None if the JSX namespace or `IntrinsicElements` interface is not available.
    pub(crate) fn get_intrinsic_elements_type(&mut self) -> Option<TypeId> {
        // Get the JSX namespace symbol
        let jsx_sym_id = self.get_jsx_namespace_type()?;

        // Get lib binders for cross-arena symbol lookup
        let lib_binders = self.get_lib_binders();

        // Get the JSX namespace symbol data
        let symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(jsx_sym_id, &lib_binders)?;

        // Look up IntrinsicElements in the JSX namespace exports
        let exports = symbol.exports.as_ref()?;
        let intrinsic_elements_sym_id = exports.get("IntrinsicElements")?;

        // Return the type reference for IntrinsicElements
        Some(self.type_reference_symbol_type(intrinsic_elements_sym_id))
    }

    // =========================================================================
    // JSX Element Type
    // =========================================================================

    /// Get the JSX.Element type for fragments.
    ///
    /// Rule #36: Fragments resolve to JSX.Element type.
    pub(crate) fn get_jsx_element_type(&mut self, node_idx: NodeIndex) -> TypeId {
        // Try to resolve JSX.Element from the JSX namespace
        if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
            let lib_binders = self.get_lib_binders();
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(jsx_sym_id, &lib_binders)
                && let Some(exports) = symbol.exports.as_ref()
                && let Some(element_sym_id) = exports.get("Element")
            {
                return self.type_reference_symbol_type(element_sym_id);
            }
        }
        // TS7026: JSX element implicitly has type 'any' because no interface 'JSX.Element' exists.
        // Only report when noImplicitAny is enabled (TS7026 is an implicit-any diagnostic)
        if self.ctx.no_implicit_any() {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                node_idx,
                diagnostic_codes::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_EXISTS,
                &["Element"],
            );
        }
        TypeId::ANY
    }

    // =========================================================================
    // JSX Attribute Type Checking
    // =========================================================================

    /// Check if a string is a valid JavaScript identifier.
    /// Used to avoid type-checking invalid JSX attribute names that should be parser errors.
    fn is_valid_js_identifier(s: &str) -> bool {
        let mut chars = s.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        // JavaScript identifiers must start with a letter, $, or _
        if !first.is_ascii_alphabetic() && first != '$' && first != '_' {
            return false;
        }
        // Subsequent characters can also include digits
        chars.all(|c| c.is_ascii_alphanumeric() || c == '$' || c == '_')
    }

    /// Check JSX attributes against the expected props type.
    ///
    /// For each attribute, checks that the assigned value is assignable to the
    /// expected property type from the props interface. Emits:
    /// - TS2322 for type mismatches and excess properties
    /// - TS2741 for missing required properties
    pub(crate) fn check_jsx_attributes_against_props(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
    ) {
        // Skip checking if props_type is any or error
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return;
        }

        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };

        // Evaluate the props type to resolve IndexAccess types like
        // JSX.IntrinsicElements['tagName'] to the actual props object type
        let props_type = self.evaluate_type_with_env(props_type);

        // Skip if evaluation resulted in any or error
        if props_type == TypeId::ANY || props_type == TypeId::ERROR {
            return;
        }

        // Check if the props type has a string index signature (e.g., [s: string]: any).
        // When it does, any attribute name is valid, so skip excess property checking.
        let has_string_index =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
                .is_some_and(|shape| shape.string_index.is_some());

        // Track provided attribute names for missing-required-property check
        let mut provided_attrs: Vec<String> = Vec::new();
        let mut has_spread = false;
        let mut has_excess_property_error = false;

        // Check each attribute
        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                // Regular JSX attribute: name={value}
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };

                // Get attribute name
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let attr_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    ident.escaped_text.as_str().to_string()
                } else {
                    continue;
                };

                // Skip type-checking invalid attribute names (parser should catch these)
                if !Self::is_valid_js_identifier(&attr_name) {
                    continue;
                }

                // Only track valid identifiers for missing-prop checking
                provided_attrs.push(attr_name.clone());

                // Get expected type from props
                use tsz_solver::operations_property::PropertyAccessResult;
                let expected_type = match self
                    .resolve_property_access_with_env(props_type, &attr_name)
                {
                    PropertyAccessResult::Success { type_id, .. } => type_id,
                    PropertyAccessResult::PropertyNotFound { .. } => {
                        // Excess property: attribute doesn't exist in props type
                        // Skip if props has a string index signature or if attr starts with "data-" or "aria-"
                        if !has_string_index
                            && !attr_name.starts_with("data-")
                            && !attr_name.starts_with("aria-")
                        {
                            let props_name = self.format_type(props_type);
                            let message = format!(
                                "Type '{{{attr_name}}}' is not assignable to type '{props_name}'.\n  \
                                     Object literal may only specify known properties, \
                                     and '{attr_name}' does not exist in type '{props_name}'."
                            );
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                attr_idx,
                                &message,
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            );
                            has_excess_property_error = true;
                        }
                        continue;
                    }
                    _ => continue,
                };

                // Get actual type of the attribute value
                if attr_data.initializer.is_none() {
                    // Boolean attribute without value (e.g., <input disabled />)
                    // TypeScript treats this as true, check against boolean
                    continue;
                }

                // The initializer might be a JSX expression wrapper or a string literal
                let value_node_idx =
                    if let Some(init_node) = self.ctx.arena.get(attr_data.initializer) {
                        if init_node.kind == syntax_kind_ext::JSX_EXPRESSION {
                            // Unwrap JSX expression to get the actual expression
                            if let Some(jsx_expr) = self.ctx.arena.get_jsx_expression(init_node) {
                                jsx_expr.expression
                            } else {
                                continue;
                            }
                        } else {
                            // String literal or other expression
                            attr_data.initializer
                        }
                    } else {
                        continue;
                    };

                let actual_type = self.compute_type_of_node(value_node_idx);

                // Check assignability
                if actual_type != TypeId::ANY && actual_type != TypeId::ERROR {
                    self.check_assignable_or_report(actual_type, expected_type, value_node_idx);
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                has_spread = true;
                // Spread attribute: {...obj}
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };

                let spread_type = self.compute_type_of_node(spread_data.expression);

                if spread_type != TypeId::ANY && spread_type != TypeId::ERROR {
                    self.check_assignable_or_report(
                        spread_type,
                        props_type,
                        spread_data.expression,
                    );
                }
            }
        }

        // Check for missing required properties (TS2741)
        // Skip if there's a spread attribute (spread may provide the missing props)
        // Skip if we already reported excess property errors (tsc doesn't pile on with TS2741)
        if !has_spread && !has_excess_property_error {
            self.check_missing_required_jsx_props(props_type, &provided_attrs, attributes_idx);
        }
    }

    /// Check that all required properties in the props type are provided as JSX attributes.
    /// Emits TS2741 for each missing required property.
    fn check_missing_required_jsx_props(
        &mut self,
        props_type: TypeId,
        provided_attrs: &[String],
        attributes_idx: NodeIndex,
    ) {
        let Some(shape) = tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        else {
            return;
        };

        for prop in &shape.properties {
            if prop.optional {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(prop.name);
            if provided_attrs.iter().any(|a| a == &prop_name) {
                continue;
            }

            // Build the "source type" name from provided attributes
            let source_type = if provided_attrs.is_empty() {
                "{}".to_string()
            } else {
                format!("{{ {} }}", provided_attrs.join(", "))
            };
            let target_type = self.format_type(props_type);
            let message = format!(
                "Property '{prop_name}' is missing in type '{source_type}' but required in type '{target_type}'."
            );
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                attributes_idx,
                &message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }
    }
}
