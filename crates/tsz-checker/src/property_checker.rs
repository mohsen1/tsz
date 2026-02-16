//! Property Checking Module
//!
//! This module contains methods for checking property access and validation.
//! It handles:
//! - Property accessibility (private/protected)
//! - Computed property names
//! - Const modifier checking
//!
//! This module extends `CheckerState` with property-related methods as part of
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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let is_property_identifier = self
            .ctx
            .arena
            .get(error_node)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();

        // TypeScript allows `super["x"]` element-access forms without applying
        // the stricter method-only/private-protected checks used for `super.x`.
        if self.is_super_expression(object_expr) && !is_property_identifier {
            return true;
        }

        let Some((class_idx, is_static)) = self.resolve_class_for_access(object_expr, object_type)
        else {
            return true;
        };

        if self.is_super_expression(object_expr)
            && let Some(false) =
                self.is_method_member_in_class_hierarchy(class_idx, property_name, is_static)
        {
            // TS2340 when useDefineForClassFields is effectively false (target < ES2022):
            //   "Only public and protected methods of the base class are accessible via the 'super' keyword."
            // TS2855 when useDefineForClassFields is effectively true (target >= ES2022):
            //   "Class field '{}' defined by the parent class is not accessible in the child class via super."
            if !self.ctx.compiler_options.target.supports_es2022() {
                self.error_at_node(
                    error_node,
                    diagnostic_messages::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
                    diagnostic_codes::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
                );
            } else {
                use crate::diagnostics::format_message;
                let message = format_message(
                    diagnostic_messages::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
                    &[property_name],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
                );
            }
            return false;
        }

        let Some(access_info) = self.find_member_access_info(class_idx, property_name, is_static)
        else {
            return true;
        };

        let current_class_idx = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        let mut protected_receiver_mismatch: Option<(NodeIndex, NodeIndex)> = None;
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
                        if let Some(receiver) = receiver_class_idx {
                            if receiver == current_class_idx
                                || self.is_class_derived_from(receiver, current_class_idx)
                            {
                                true
                            } else if self.is_class_derived_from(current_class_idx, receiver) {
                                false
                            } else {
                                protected_receiver_mismatch = Some((current_class_idx, receiver));
                                false
                            }
                        } else {
                            false
                        }
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
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS,
                );
            }
            MemberAccessLevel::Protected => {
                if let Some((current_idx, receiver_idx)) = protected_receiver_mismatch {
                    let current_name = self.get_class_name_from_decl(current_idx);
                    let receiver_name = self.get_class_name_from_decl(receiver_idx);
                    let message = format!(
                        "Property '{property_name}' is protected and only accessible through an instance of class '{current_name}'. This is an instance of class '{receiver_name}'."
                    );
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A,
                    );
                } else {
                    let message = format!(
                        "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                        property_name, access_info.declaring_class_name
                    );
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES,
                    );
                }
            }
        }

        false
    }
    // =========================================================================
    // Computed Property Name Validation
    // =========================================================================

    /// Check if an expression node is an "entity name expression".
    ///
    /// Entity name expressions are simple identifiers or property access chains
    /// (e.g., `a`, `a.b`, `a.b.c`). These are always allowed as computed property
    /// names in class property declarations, regardless of their type.
    fn is_entity_name_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return true;
        }
        if expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
        {
            return self.is_entity_name_expression(access.expression);
        }
        false
    }

    /// Check a computed property name requires a simple literal or unique symbol type.
    ///
    /// Used for TS1166 (class properties), TS1169 (interfaces), and TS1170 (type literals).
    /// Entity name expressions (identifiers, property access chains) and literal
    /// expressions are always allowed. Other expressions must have a literal or
    /// unique symbol type.
    fn check_computed_property_requires_literal(
        &mut self,
        name_idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        // Entity name expressions (identifiers, property access chains) are always OK
        if self.is_entity_name_expression(computed.expression) {
            return;
        }

        // Literal expressions (string, number, no-substitution template) are always OK
        // since they inherently have literal types
        if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
            let kind = expr_node.kind;
            if kind == SyntaxKind::StringLiteral as u16
                || kind == SyntaxKind::NumericLiteral as u16
                || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                return;
            }
        }

        let expr_type = self.get_type_of_node(computed.expression);

        if expr_type == tsz_solver::TypeId::ERROR {
            return;
        }

        if !tsz_solver::type_queries::is_type_usable_as_property_name(self.ctx.types, expr_type) {
            self.error_at_node(name_idx, message, code);
        }
    }

    /// Check a computed property name in a class property declaration (TS1166).
    pub(crate) fn check_class_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
        );
    }

    /// Check a computed property name in an interface (TS1169).
    pub(crate) fn check_interface_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
        );
    }

    /// Check a computed property name in a type literal (TS1170).
    pub(crate) fn check_type_literal_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
        );
    }

    /// Check a computed property name for type errors (TS2464).
    ///
    /// Validates that the expression used for a computed property name
    /// has a type that is string, number, symbol, or any (including literals).
    /// This check is independent of strictNullChecks.
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

        let expr_type = self.get_type_of_node(computed.expression);

        // Skip error types to avoid cascading diagnostics
        if expr_type == tsz_solver::TypeId::ERROR {
            return;
        }

        // TS2464: type must be string, number, symbol, or any (including literals).
        // This check ignores strictNullChecks: undefined/null always fail.
        // Suppress this diagnostic in files with parse errors to avoid noise (e.g., [await] without operand).
        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
        if !self.has_parse_errors() && !evaluator.is_valid_computed_property_name_type(expr_type) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                name_idx,
                diagnostic_messages::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
                diagnostic_codes::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
            );
        }
    }

    // =========================================================================
    // Const Modifier Checking
    // =========================================================================

    /// Get the const modifier node from a list of modifiers, if present.
    ///
    /// Returns the `NodeIndex` of the const modifier for error reporting.
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
