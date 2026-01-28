//! JSX Type Checking Module
//!
//! This module contains JSX type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - JSX opening element type resolution
//! - JSX namespace type lookup
//! - JSX intrinsic elements type lookup
//! - JSX element type for fragments
//!
//! This implements Rule #36: JSX type checking with case-sensitive tag lookup.

use crate::binder::SymbolId;
use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::TypeId;

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
        let tag_name = if tag_name_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
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
            .map(|name| {
                name.chars()
                    .next()
                    .map(|c| c.is_ascii_lowercase())
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_intrinsic {
            // Intrinsic elements: look up JSX.IntrinsicElements[tagName]
            // Try to resolve JSX.IntrinsicElements and create an indexed access type
            if let Some(tag) = tag_name {
                if let Some(intrinsic_elements_type) = self.get_intrinsic_elements_type() {
                    // Create JSX.IntrinsicElements['tagName'] as an IndexAccess type
                    let tag_literal = self.ctx.types.literal_string(tag);
                    return self.ctx.types.intern(crate::solver::TypeKey::IndexAccess(
                        intrinsic_elements_type,
                        tag_literal,
                    ));
                }
            }
            // Fall back to ANY if JSX namespace is not available
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
    /// Returns None if the JSX namespace or IntrinsicElements interface is not available.
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
    pub(crate) fn get_jsx_element_type(&mut self) -> TypeId {
        // Try to resolve JSX.Element from the JSX namespace
        if let Some(jsx_sym_id) = self.get_jsx_namespace_type() {
            let lib_binders = self.get_lib_binders();
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(jsx_sym_id, &lib_binders)
            {
                if let Some(exports) = symbol.exports.as_ref() {
                    if let Some(element_sym_id) = exports.get("Element") {
                        return self.type_reference_symbol_type(element_sym_id);
                    }
                }
            }
        }
        // Fall back to ANY if JSX namespace or Element type is not available
        TypeId::ANY
    }
}
