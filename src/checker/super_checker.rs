//! Super Expression Checking Module
//!
//! This module contains methods for validating super expression usage.
//! It handles:
//! - super() call validation (must be in constructors)
//! - super property access validation (must be in valid contexts)
//! - Derived class requirements
//! - Static property initializer restrictions
//!
//! This module extends CheckerState with super-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;

// =============================================================================
// Super Expression Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Super Context Detection
    // =========================================================================

    /// Check if super is in a nested function inside a constructor.
    ///
    /// Returns true if super is in a nested function inside a constructor.
    pub(crate) fn is_super_in_nested_function(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        let mut function_depth = 0;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Count function/arrow function boundaries
            if parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                function_depth += 1;
            }

            // Check if we've reached the constructor
            if parent_node.kind == syntax_kind_ext::CONSTRUCTOR {
                return function_depth > 0;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class method body.
    ///
    /// Returns true if inside a method body (both static and non-static).
    /// `super` property access is valid in both static and instance methods
    /// of a derived class.
    pub(crate) fn is_in_class_method_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            if parent_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                // `super` property access is valid in both static and non-static methods
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class accessor body (getter/setter).
    ///
    /// Returns true if inside an accessor body.
    /// IMPORTANT: Skips arrow function boundaries since they capture the class context.
    pub(crate) fn is_in_class_accessor_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            if parent_node.kind == syntax_kind_ext::GET_ACCESSOR
                || parent_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    /// Check if a node is inside a class property initializer.
    ///
    /// Returns true if inside a property declaration (class field).
    /// IMPORTANT: Skips arrow function boundaries since they capture the class context.
    pub(crate) fn is_in_class_property_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            // Arrow functions capture the class context, so skip them when checking
            if parent_node.kind == syntax_kind_ext::ARROW_FUNCTION {
                current = parent_idx;
                continue;
            }

            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                return true;
            }

            // Check if we've left the class
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                break;
            }

            current = parent_idx;
        }

        false
    }

    // =========================================================================
    // Super Expression Validation
    // =========================================================================

    /// Check a super expression for proper usage.
    ///
    /// Validates that super expressions are used correctly:
    /// - TS17011: super cannot be in static property initializers
    /// - TS2335: super can only be used in derived classes
    /// - TS2337: super() calls must be in constructors
    /// - TS2336: super property access must be in valid contexts
    pub(crate) fn check_super_expression(&mut self, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Check if we're in a class context at all
        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_ONLY_IN_DERIVED_CLASS,
                diagnostic_codes::SUPER_ONLY_IN_DERIVED_CLASS,
            );
            return;
        };

        // Check if the class has a base class (is a derived class)
        let has_base_class = self.get_base_class_idx(class_info.class_idx).is_some();

        // Detect if this is a super() call or super property access
        let parent_info = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent).map(|n| (ext.parent, n)));

        let is_super_call = parent_info
            .as_ref()
            .map(|(_, parent_node)| {
                if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                    return false;
                }
                let Some(call) = self.ctx.arena.get_call_expr(parent_node) else {
                    return false;
                };
                call.expression == idx
            })
            .unwrap_or(false);

        let is_super_property_access = parent_info
            .as_ref()
            .map(|(_, parent_node)| {
                parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .unwrap_or(false);

        // TS2335: super can only be referenced in a derived class
        if !has_base_class {
            self.error_at_node(
                idx,
                diagnostic_messages::SUPER_ONLY_IN_DERIVED_CLASS,
                diagnostic_codes::SUPER_ONLY_IN_DERIVED_CLASS,
            );
            return;
        }

        // TS2337/TS17011: Super calls are not permitted outside constructors
        // This includes super() in static property initializers
        if is_super_call {
            // TS17011: super() is not allowed in static property initializers
            if class_info.in_static_property_initializer {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_IN_STATIC_PROPERTY_INITIALIZER,
                    diagnostic_codes::SUPER_IN_STATIC_PROPERTY_INITIALIZER,
                );
                return;
            }
            if !class_info.in_constructor {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                    diagnostic_codes::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                );
                return;
            }

            // Check for nested function inside constructor
            if self.is_super_in_nested_function(idx) {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                    diagnostic_codes::SUPER_CALL_NOT_IN_CONSTRUCTOR,
                );
                return;
            }
        }

        // TS2336: super property access must be in constructor, method, accessor, or property initializer
        // Note: Arrow functions capture the class context, so super inside arrow functions is valid
        // if the arrow itself is in a valid context (checked by the helper functions)
        if is_super_property_access {
            let in_valid_context = class_info.in_constructor
                || self.is_in_class_method_body(idx)
                || self.is_in_class_accessor_body(idx)
                || self.is_in_class_property_initializer(idx);

            if !in_valid_context {
                self.error_at_node(
                    idx,
                    diagnostic_messages::SUPER_PROPERTY_ACCESS_INVALID_CONTEXT,
                    diagnostic_codes::SUPER_PROPERTY_ACCESS_INVALID_CONTEXT,
                );
            }
        }
    }
}
