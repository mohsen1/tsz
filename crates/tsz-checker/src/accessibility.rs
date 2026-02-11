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
        if let Some(ident) = self.ctx.arena.get_identifier_at(name_idx) {
            let name_str = &ident.escaped_text;
            return name_str.starts_with('#');
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
                    diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS,
                    format!(
                        "Property '{}' is private and only accessible within class '{}'.",
                        property_name, declaring_class_name
                    ),
                )
            } else {
                (
                    diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES,
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
