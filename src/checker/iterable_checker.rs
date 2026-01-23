//! Iterable/Iterator Type Checking Module
//!
//! This module contains iterable and iterator type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Checking if a type is iterable (has Symbol.iterator protocol)
//! - Checking if a type is async iterable (has Symbol.asyncIterator protocol)
//! - Computing element types for for-of loops
//! - Emitting appropriate errors for non-iterable types
//!
//! This module extends CheckerState with methods for iterable/iterator protocol
//! checking, providing cleaner APIs for iteration-related type operations.

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{
    diagnostic_codes, diagnostic_messages, format_message,
};
use crate::parser::NodeIndex;
use crate::solver::TypeId;

// =============================================================================
// Iterable Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Iterable Protocol Checking
    // =========================================================================

    /// Check if a type is iterable (has Symbol.iterator protocol).
    ///
    /// A type is iterable if it is:
    /// - String type
    /// - Array type
    /// - Tuple type
    /// - Has a [Symbol.iterator] method
    /// - A union where all members are iterable
    pub fn is_iterable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        // Intrinsic types that are always iterable or not iterable
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true; // Don't report errors on any/unknown/error
        }
        if type_id == TypeId::STRING {
            return true;
        }
        if type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::VOID
            || type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::NEVER
        {
            return false;
        }

        // Unwrap readonly wrappers
        let mut ty = type_id;
        while let Some(TypeKey::ReadonlyType(inner)) = self.ctx.types.lookup(ty) {
            ty = inner;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::Array(_)) => true,
            Some(TypeKey::Tuple(_)) => true,
            Some(TypeKey::Literal(crate::solver::LiteralValue::String(_))) => true,
            Some(TypeKey::Union(members_id)) => {
                let members = self.ctx.types.type_list(members_id);
                members.iter().all(|&m| self.is_iterable_type(m))
            }
            Some(TypeKey::Object(shape_id)) => {
                // Check if object has a [Symbol.iterator] method or 'next' method
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    // Check for [Symbol.iterator] or 'next' method (iterator protocol)
                    if (prop_name.as_ref() == "[Symbol.iterator]" || prop_name.as_ref() == "next")
                        && prop.is_method
                    {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Check if a type is async iterable (has Symbol.asyncIterator protocol).
    pub fn is_async_iterable_type(&self, type_id: TypeId) -> bool {
        use crate::solver::TypeKey;

        // Intrinsic types that are always iterable or not iterable
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true; // Don't report errors on any/unknown/error
        }

        // Unwrap readonly wrappers
        let mut ty = type_id;
        while let Some(TypeKey::ReadonlyType(inner)) = self.ctx.types.lookup(ty) {
            ty = inner;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::Union(members_id)) => {
                let members = self.ctx.types.type_list(members_id);
                members.iter().all(|&m| self.is_async_iterable_type(m))
            }
            Some(TypeKey::Object(shape_id)) => {
                // Check if object has a [Symbol.asyncIterator] method
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "[Symbol.asyncIterator]" && prop.is_method {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    // =========================================================================
    // For-Of Element Type Computation
    // =========================================================================

    /// Compute the element type produced by a `for (... of expr)` loop.
    ///
    /// This is a best-effort implementation for common cases (arrays/tuples/unions).
    pub fn for_of_element_type(&mut self, iterable_type: TypeId) -> TypeId {
        use crate::solver::TypeKey;

        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
        {
            return iterable_type;
        }

        // Unwrap readonly wrappers with depth guard to prevent infinite loops
        let mut ty = iterable_type;
        let mut readonly_depth = 0;
        while let Some(TypeKey::ReadonlyType(inner)) = self.ctx.types.lookup(ty) {
            readonly_depth += 1;
            if readonly_depth > 100 {
                break;
            }
            ty = inner;
        }

        match self.ctx.types.lookup(ty) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(tuple_id)) => {
                let elems = self.ctx.types.tuple_list(tuple_id);
                let mut member_types: Vec<TypeId> = elems.iter().map(|e| e.type_id).collect();
                if member_types.is_empty() {
                    TypeId::NEVER
                } else if member_types.len() == 1 {
                    member_types.pop().unwrap_or(TypeId::ANY)
                } else {
                    self.ctx.types.union(member_types)
                }
            }
            Some(TypeKey::Union(members_id)) => {
                let members = self.ctx.types.type_list(members_id);
                let mut element_types = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    element_types.push(self.for_of_element_type(member));
                }
                self.ctx.types.union(element_types)
            }
            _ => TypeId::ANY,
        }
    }

    // =========================================================================
    // For-Of Iterability Checking with Error Reporting
    // =========================================================================

    /// Check iterability of a for-of expression and emit TS2488/TS2504 if not iterable.
    ///
    /// Returns `true` if the type is iterable (or async iterable for for-await-of).
    pub fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        is_async: bool,
    ) -> bool {
        // For async for-of, first check async iterable, then fall back to sync iterable
        if is_async {
            if self.is_async_iterable_type(expr_type) || self.is_iterable_type(expr_type) {
                return true;
            }
            // Not async iterable - emit TS2504
            if let Some((start, end)) = self.get_node_span(expr_idx) {
                let type_str = self.format_type(expr_type);
                let message = format_message(
                    diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
                );
            }
            return false;
        }

        // Regular for-of - check sync iterability
        if self.is_iterable_type(expr_type) {
            return true;
        }

        // Not iterable - emit TS2488
        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(expr_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
        }
        false
    }

    /// Check iterability of a spread argument and emit TS2488 if not iterable.
    ///
    /// Used for spread in array literals and function call arguments.
    /// Returns `true` if the type is iterable.
    pub fn check_spread_iterability(&mut self, spread_type: TypeId, expr_idx: NodeIndex) -> bool {
        // Skip error types and any/unknown
        if spread_type == TypeId::ANY
            || spread_type == TypeId::UNKNOWN
            || spread_type == TypeId::ERROR
        {
            return true;
        }

        if self.is_iterable_type(spread_type) {
            return true;
        }

        // Not iterable - emit TS2488
        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(spread_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
        }
        false
    }
}
