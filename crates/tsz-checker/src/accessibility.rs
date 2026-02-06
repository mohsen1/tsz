//! Accessibility Module
//!
//! This module contains accessibility checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Private/protected modifier checking
//! - Constructor accessibility verification
//! - Property accessibility verification
//! - Private brand handling for nominal type checking
//!
//! Methods use a `verify_` or `check_` prefix for clarity.

use crate::state::CheckerState;
use crate::types::diagnostics::{Diagnostic, DiagnosticCategory, diagnostic_codes};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

// =============================================================================
// Accessibility Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Modifier Checking
    // =========================================================================

    /// Check if a modifier list contains a private modifier.
    ///
    /// This checks for the presence of a PrivateKeyword in the modifier list.
    pub fn verify_has_private_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        self.ctx
            .has_modifier(modifiers, SyntaxKind::PrivateKeyword as u16)
    }

    /// Check if a modifier list contains a protected modifier.
    ///
    /// This checks for the presence of a ProtectedKeyword in the modifier list.
    pub fn verify_has_protected_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        self.ctx
            .has_modifier(modifiers, SyntaxKind::ProtectedKeyword as u16)
    }

    /// Check if a member requires nominal typing (has private/protected modifiers or is a private identifier).
    ///
    /// This checks if the member has private/protected modifiers or if the name is a private identifier.
    pub fn verify_member_requires_nominal(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        name_idx: NodeIndex,
    ) -> bool {
        if self.verify_has_private_modifier(modifiers)
            || self.verify_has_protected_modifier(modifiers)
        {
            return true;
        }

        // Check if this is a private identifier (starts with #)
        if let Some(node) = self.ctx.arena.get(name_idx) {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                let name_str = &ident.escaped_text;
                return name_str.starts_with('#');
            }
        }

        false
    }

    // =========================================================================
    // Constructor Accessibility
    // =========================================================================

    /// Check if two constructor types have compatible accessibility levels.
    ///
    /// Returns true if source can be assigned to target based on their constructor accessibility.
    /// - Public constructors are compatible with everything
    /// - Private constructors are only compatible with the same private constructor
    /// - Protected constructors are compatible with protected or public targets
    ///
    /// Note: This is a simplified version. The full accessibility checking with
    /// inheritance relationships is handled by `constructor_accessibility_override`.
    pub fn verify_constructor_accessibility_compatible(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // Public constructors are compatible with everything
        if !self.ctx.private_constructor_types.contains(&source)
            && !self.ctx.protected_constructor_types.contains(&source)
        {
            return true;
        }

        // Private constructors are only compatible with the same private constructor
        if self.ctx.private_constructor_types.contains(&source) {
            if self.ctx.private_constructor_types.contains(&target) {
                return source == target;
            }
            return false;
        }

        // Protected constructors are compatible with protected or public targets
        if self.ctx.protected_constructor_types.contains(&source) {
            if self.ctx.private_constructor_types.contains(&target) {
                return false;
            }
            return true;
        }

        true
    }

    /// Check if a type is an abstract constructor (typeof AbstractClass).
    ///
    /// Returns true if the type represents an abstract class constructor.
    pub fn verify_is_abstract_constructor(&self, type_id: TypeId) -> bool {
        self.ctx.abstract_constructor_types.contains(&type_id)
    }

    /// Check if a type is a private constructor.
    ///
    /// Returns true if the constructor has private access modifier.
    pub fn verify_is_private_constructor(&self, type_id: TypeId) -> bool {
        self.ctx.private_constructor_types.contains(&type_id)
    }

    /// Check if a type is a protected constructor.
    ///
    /// Returns true if the constructor has protected access modifier.
    pub fn verify_is_protected_constructor(&self, type_id: TypeId) -> bool {
        self.ctx.protected_constructor_types.contains(&type_id)
    }

    /// Check if a type is a public constructor.
    ///
    /// Returns true if the constructor is NOT private and NOT protected.
    pub fn verify_is_public_constructor(&self, type_id: TypeId) -> bool {
        !self.verify_is_private_constructor(type_id)
            && !self.verify_is_protected_constructor(type_id)
    }

    // =========================================================================
    // Property Accessibility
    // =========================================================================

    /// Verify that a property access is valid given the current class context.
    ///
    /// Returns true if the access is allowed, false if it should be an error.
    /// This does NOT emit errors - use `check_property_accessible` for error emission.
    ///
    /// This is a simplified pre-check. For full accessibility checking with error emission,
    /// use the existing `check_property_accessibility` method.
    pub fn verify_property_accessible(&self, _object_type: TypeId, _property_name: &str) -> bool {
        // If we're not in a class context, we can only access public members
        let _current_class = self.ctx.enclosing_class.as_ref();

        // For now, we always return true and let the full check handle it
        // This method is meant for quick pre-checks where we don't need errors
        true
    }

    // =========================================================================
    // Private Brand Checking
    // =========================================================================

    /// Check if two types share the same private brand.
    ///
    /// This is used for nominal typing with private members.
    /// Returns true if both types have the same private brand or neither has a private brand.
    ///
    /// Note: This is a simplified implementation. The full version would check
    /// the private brand properties directly in the type system.
    pub fn verify_same_private_brand(&self, _type1: TypeId, _type2: TypeId) -> bool {
        // Simplified: always return true for now
        // The full implementation requires access to private type internals
        true
    }

    /// Get a description of why private brands don't match, if any.
    ///
    /// Returns None if brands match or if there are no private brands.
    /// Returns Some(message) describing the mismatch otherwise.
    pub fn describe_private_brand_mismatch(
        &self,
        _source: TypeId,
        _target: TypeId,
    ) -> Option<String> {
        // Simplified implementation - return None (no mismatch)
        // The full implementation would compare private brand properties
        None
    }

    // =========================================================================
    // Error Reporting for Accessibility
    // =========================================================================

    /// Report a property accessibility error (TS2341 for private, TS2445 for protected).
    pub fn report_property_not_accessible(
        &mut self,
        property_name: &str,
        is_private: bool,
        declaring_class_name: &str,
        idx: NodeIndex,
    ) {
        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let (code, message) = if is_private {
                (
                    diagnostic_codes::PROPERTY_IS_PRIVATE,
                    format!(
                        "Property '{}' is private and only accessible within class '{}'.",
                        property_name, declaring_class_name
                    ),
                )
            } else {
                (
                    diagnostic_codes::PROPERTY_IS_PROTECTED,
                    format!(
                        "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                        property_name, declaring_class_name
                    ),
                )
            };

            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code,
                related_information: Vec::new(),
            });
        }
    }

    /// Report an abstract constructor instantiation error (TS2511).
    pub fn report_cannot_instantiate_abstract_class(&mut self, class_name: &str, idx: NodeIndex) {
        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let message = format!(
                "Cannot create an instance of an abstract class '{}'.",
                class_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: 2511, // TS2511
                related_information: Vec::new(),
            });
        }
    }

    /// Report a constructor accessibility error (TS2673).
    pub fn report_constructor_not_accessible(
        &mut self,
        source_name: &str,
        _target_name: &str,
        idx: NodeIndex,
    ) {
        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let message = format!(
                "Constructor of class '{}' is private and only accessible within the class declaration.",
                source_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: 2673, // TS2673
                related_information: Vec::new(),
            });
        }
    }
}
