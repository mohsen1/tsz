//! Error Reporter Module
//!
//! This module contains additional error reporting methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module use a `report_` prefix naming convention to distinguish
//! them from the legacy `error_` methods in state.rs. Over time, callers should migrate
//! to using these `report_` methods.
//!
//! This module extends CheckerState with additional impl blocks rather than moving
//! existing code, to maintain backward compatibility during the refactoring.

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::parser::NodeIndex;
use crate::solver::TypeId;

// =============================================================================
// Error Reporting Methods (Extended API)
// =============================================================================
//
// These methods provide a cleaner, more consistent API for error reporting.
// They use `report_` prefix instead of `error_` for better discoverability
// and to distinguish from legacy methods.

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Core Error Emission
    // =========================================================================

    /// Report an error at a specific node with message and code.
    ///
    /// This is a convenience wrapper that creates a diagnostic and adds it
    /// to the checker's diagnostics list.
    pub fn report_error(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message.to_string(),
                category: DiagnosticCategory::Error,
                code,
                related_information: Vec::new(),
            });
        }
    }

    /// Report an error at a specific position with message and code.
    ///
    /// This is a convenience wrapper that creates a diagnostic and adds it
    /// to the checker's diagnostics list.
    pub fn report_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic {
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: message.to_string(),
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        });
    }

    // =========================================================================
    // Type Error Reporting
    // =========================================================================

    /// Report a type not assignable error (TS2322).
    ///
    /// This is the basic error that just says "Type X is not assignable to Y".
    /// For detailed errors with elaboration (e.g., "property 'x' is missing"),
    /// use the existing `error_type_not_assignable_with_reason_at` instead.
    pub fn report_type_not_assignable(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        // ERROR TYPE SUPPRESSION
        //
        // When source or target type IS the ERROR sentinel type, suppress TS2322 emission.
        // This prevents unhelpful cascading errors like "Type 'error' is not assignable to type 'string'".
        //
        // Rationale:
        // 1. ERROR type means symbol resolution failed earlier (TS2304 already emitted)
        // 2. Emitting TS2322 for ERROR provides no diagnostic value to users
        // 3. TypeScript behavior: only report the root resolution failure, not cascades
        //
        // Note: We only suppress when type IS ERROR, not when type CONTAINS ERROR.
        // A union like `string | error` should still be checked against other types.
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return;
        }

        // ANY TYPE SUPPRESSION
        //
        // ANY is assignable to and from any type - this matches TypeScript semantics.
        // The `any` type is an escape hatch that bypasses type checking entirely.
        if source == TypeId::ANY || target == TypeId::ANY {
            return;
        }

        self.error_type_not_assignable_at(source, target, idx);
    }

    // =========================================================================

    /// Report a type constraint not satisfied error (TS2344).
    pub fn report_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let type_str = self.format_type(type_arg);
            let constraint_str = self.format_type(constraint);
            let message = format_message(
                diagnostic_messages::TYPE_NOT_SATISFY_CONSTRAINT,
                &[&type_str, &constraint_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED,
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Name Resolution Errors
    // =========================================================================

    /// Report TS2304: Cannot find name 'X'.
    pub fn report_cannot_find_name(&mut self, name: &str, idx: NodeIndex) {
        self.error_cannot_find_name_at(name, idx);
    }

    /// Report TS2304 with "did you mean" suggestions (TS2552).
    pub fn report_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        self.error_cannot_find_name_with_suggestions(name, suggestions, idx);
    }

    /// Report TS2583: Cannot find name - suggests changing target library.
    pub fn report_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        self.error_cannot_find_name_change_lib(name, idx);
    }

    /// Report TS2318: Cannot find global type.
    pub fn report_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        use crate::lib_loader;

        // Check if this is an ES2015+ type that would require a specific lib
        let is_es2015_type = lib_loader::is_es2015_plus_type(name);

        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let (code, message) = if is_es2015_type {
                (
                    lib_loader::MISSING_ES2015_LIB_SUPPORT,
                    format!(
                        "Cannot find name '{}'. Do you need to change your target library? Try changing the 'lib' compiler option to 'es2015' or later.",
                        name
                    ),
                )
            } else {
                (
                    lib_loader::CANNOT_FIND_GLOBAL_TYPE,
                    format!("Cannot find global type '{}'.", name),
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

    // =========================================================================
    // Property Access Errors
    // =========================================================================

    /// Report a property missing error.
    pub fn report_property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        self.error_property_missing_at(prop_name, source, target, idx);
    }

    /// Report a property not exist error.
    pub fn report_property_not_exist(&mut self, prop_name: &str, type_id: TypeId, idx: NodeIndex) {
        self.error_property_not_exist_at(prop_name, type_id, idx);
    }

    /// Report an excess property error.
    pub fn report_excess_property(&mut self, prop_name: &str, target: TypeId, idx: NodeIndex) {
        self.error_excess_property_at(prop_name, target, idx);
    }

    /// Report a "Cannot assign to readonly property" error.
    pub fn report_readonly_property(&mut self, prop_name: &str, idx: NodeIndex) {
        self.error_readonly_property_at(prop_name, idx);
    }

    // =========================================================================
    // Function Call Errors
    // =========================================================================

    /// Report an argument not assignable error.
    pub fn report_argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_argument_not_assignable_at(arg_type, param_type, idx);
    }

    /// Report an argument count mismatch error.
    pub fn report_argument_count_mismatch(&mut self, expected: usize, got: usize, idx: NodeIndex) {
        self.error_argument_count_mismatch_at(expected, got, idx);
    }

    /// Report "expected at least N arguments" error (TS2555).
    pub fn report_expected_at_least_arguments(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        self.error_expected_at_least_arguments_at(expected_min, got, idx);
    }

    /// Report a "type is not callable" error.
    pub fn report_not_callable(&mut self, type_id: TypeId, idx: NodeIndex) {
        self.error_not_callable_at(type_id, idx);
    }

    /// Report "No overload matches this call" error.
    pub fn report_no_overload_matches(
        &mut self,
        idx: NodeIndex,
        failures: &[crate::solver::PendingDiagnostic],
    ) {
        self.error_no_overload_matches_at(idx, failures);
    }

    // =========================================================================
    // Type/Value Mismatch Errors
    // =========================================================================

    /// Report TS2693/TS2585: Symbol only refers to a type, but is used as a value.
    pub fn report_type_only_value(&mut self, name: &str, idx: NodeIndex) {
        self.error_type_only_value_at(name, idx);
    }

    /// Report TS2749: Symbol refers to a value, but is used as a type.
    pub fn report_value_only_type(&mut self, name: &str, idx: NodeIndex) {
        self.error_value_only_type_at(name, idx);
    }

    // =========================================================================
    // Variable/Declaration Errors
    // =========================================================================

    /// Report TS2454: Variable is used before being assigned.
    pub fn report_variable_used_before_assigned(&mut self, name: &str, idx: NodeIndex) {
        self.error_variable_used_before_assigned_at(name, idx);
    }

    /// Report TS2454: Subsequent variable declarations must have the same type.
    pub fn report_subsequent_variable_declaration(
        &mut self,
        name: &str,
        type1: TypeId,
        type2: TypeId,
        idx: NodeIndex,
    ) {
        self.error_subsequent_variable_declaration(name, type1, type2, idx);
    }

    // =========================================================================
    // Class-Related Errors
    // =========================================================================

    /// Report TS2564: Property has no initializer and is not definitely assigned.
    pub fn report_property_no_initializer(&mut self, prop_name: &str, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{
            diagnostic_codes, diagnostic_messages, format_message,
        };

        if let Some((start, end)) = self.get_node_span(idx) {
            let length = end.saturating_sub(start);
            let message = format_message(
                diagnostic_messages::PROPERTY_NO_INITIALIZER_NO_DEFINITE_ASSIGNMENT,
                &[prop_name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_NOT_DEFINITELY_ASSIGNED,
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2715: Abstract property in constructor.
    pub fn report_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        self.error_abstract_property_in_constructor(prop_name, class_name, idx);
    }

    // =========================================================================
    // Module/Namespace Errors
    // =========================================================================

    /// Report TS2694: Namespace has no exported member.
    pub fn report_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        self.error_namespace_no_export(namespace_name, member_name, idx);
    }

    // =========================================================================
    // Generic Type Errors
    // =========================================================================

    /// Report TS2314: Generic type 'X' requires N type argument(s).
    pub fn report_generic_type_requires_type_arguments(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        self.error_generic_type_requires_type_arguments_at(name, required_count, idx);
    }

    // =========================================================================
    // Private Member Errors
    // =========================================================================

    /// Report TS2803: Cannot assign to private method.
    pub fn report_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        self.error_private_method_not_writable(prop_name, idx);
    }
}
