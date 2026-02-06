//! Property Checking Module
//!
//! This module contains methods for checking property access and validation.
//! It handles:
//! - Property accessibility (private/protected)
//! - Computed property names
//! - Const modifier checking
//!
//! This module extends CheckerState with property-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::state::CheckerState;
use crate::state::MemberAccessLevel;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Property Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Accessibility
    // =========================================================================

    /// Check if accessing a property is allowed based on its access modifier.
    ///
    /// ## Access Modifiers:
    /// - **Private**: Accessible only within the declaring class
    /// - **Protected**: Accessible within the declaring class and subclasses
    /// - **Public**: Accessible from anywhere (default)
    ///
    /// ## Returns:
    /// - `true` if access is allowed
    /// - `false` if access is denied (error emitted)
    ///
    /// ## Error Codes:
    /// - TS2341: "Property '{}' is private and only accessible within class '{}'."
    /// - TS2445: "Property '{}' is protected and only accessible within class '{}' and its subclasses."
    pub(crate) fn check_property_accessibility(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        object_type: tsz_solver::TypeId,
    ) -> bool {
        use crate::types::diagnostics::diagnostic_codes;

        let Some((class_idx, is_static)) = self.resolve_class_for_access(object_expr, object_type)
        else {
            return true;
        };
        let Some(access_info) = self.find_member_access_info(class_idx, property_name, is_static)
        else {
            return true;
        };

        let current_class_idx = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        let allowed = match access_info.level {
            MemberAccessLevel::Private => {
                current_class_idx == Some(access_info.declaring_class_idx)
            }
            MemberAccessLevel::Protected => match current_class_idx {
                None => false,
                Some(current_class_idx) => {
                    if current_class_idx == access_info.declaring_class_idx {
                        true
                    } else if !self
                        .is_class_derived_from(current_class_idx, access_info.declaring_class_idx)
                    {
                        false
                    } else {
                        let receiver_class_idx =
                            self.resolve_receiver_class_for_access(object_expr, object_type);
                        receiver_class_idx
                            .map(|receiver| {
                                receiver == current_class_idx
                                    || self.is_class_derived_from(receiver, current_class_idx)
                            })
                            .unwrap_or(false)
                    }
                }
            },
        };

        if allowed {
            return true;
        }

        match access_info.level {
            MemberAccessLevel::Private => {
                let message = format!(
                    "Property '{}' is private and only accessible within class '{}'.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(error_node, &message, diagnostic_codes::PROPERTY_IS_PRIVATE);
            }
            MemberAccessLevel::Protected => {
                let message = format!(
                    "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PROTECTED,
                );
            }
        }

        false
    }

    // =========================================================================
    // Computed Property Name Validation
    // =========================================================================

    /// Check a computed property name for type errors.
    ///
    /// This function validates that the expression used for a computed
    /// property name is well-formed. It computes the type of the expression
    /// to ensure any type errors are reported.
    pub(crate) fn check_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        // Simply compute the type - this will report any type errors in the expression
        self.get_type_of_node(computed.expression);
    }

    // =========================================================================
    // Const Modifier Checking
    // =========================================================================

    /// Get the const modifier node from a list of modifiers, if present.
    ///
    /// Returns the NodeIndex of the const modifier for error reporting.
    /// Used to validate that readonly properties cannot have initializers.
    pub(crate) fn get_const_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }
}
