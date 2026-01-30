//! Error Handler Trait
//!
//! This module defines the ErrorHandler trait and its implementation
//! as part of Phase 3 architecture refactoring.
//!
//! The trait provides a consistent API for error reporting across the codebase,
//! eliminating code duplication and making error emission more maintainable.

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{Diagnostic, DiagnosticCategory};
use crate::parser::NodeIndex;
use crate::solver::TypeId;

// =============================================================================
// Error Handler Trait
// =============================================================================

/// Trait for error emission in the type checker.
///
/// This trait provides a consistent API for reporting diagnostics,
/// with implementations that can emit errors to different backends
/// (e.g., diagnostics collection, error collector, etc.).
///
/// The trait is designed to:
/// 1. Provide type-safe methods for common error patterns
/// 2. Eliminate code duplication across error functions
/// 3. Make error emission more consistent and maintainable
pub trait ErrorHandler {
    // =========================================================================
    // Core Error Emission
    // =========================================================================

    /// Emit an error at a specific node.
    fn emit_error(&mut self, node_idx: NodeIndex, message: &str, code: u32);

    /// Emit an error at a specific position.
    fn emit_error_at(&mut self, start: u32, length: u32, message: &str, code: u32);

    /// Emit a diagnostic directly.
    fn emit_diagnostic(&mut self, diagnostic: Diagnostic);

    // =========================================================================
    // Type Error Patterns
    // =========================================================================

    /// Emit a "type not assignable" error (TS2322).
    ///
    /// This handles the common pattern where a source type cannot be
    /// assigned to a target type due to incompatibility.
    fn emit_type_not_assignable(&mut self, source: TypeId, target: TypeId, idx: NodeIndex);

    /// Emit a "type not assignable" error with detailed reason.
    ///
    /// This provides more context about WHY the types are incompatible
    /// (e.g., "property 'x' is missing", "types have different declarations").
    fn emit_type_not_assignable_with_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
        reason: &str,
        idx: NodeIndex,
    );

    // =========================================================================
    // Property Access Error Patterns
    // =========================================================================

    /// Emit a "property does not exist" error (TS2339).
    fn emit_property_not_exist(&mut self, prop_name: &str, type_id: TypeId, idx: NodeIndex);

    /// Emit a "property missing" error (TS2740).
    fn emit_property_missing(
        &mut self,
        prop_name: &str,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    );

    /// Emit a "readonly property" error (TS2540).
    fn emit_readonly_property(&mut self, prop_name: &str, idx: NodeIndex);

    /// Emit an "excess property" error (TS2353).
    fn emit_excess_property(&mut self, prop_name: &str, target_type: TypeId, idx: NodeIndex);

    // =========================================================================
    // Function Call Error Patterns
    // =========================================================================

    /// Emit an "argument not assignable" error.
    fn emit_argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    );

    /// Emit an "argument count mismatch" error (TS2554).
    fn emit_argument_count_mismatch(&mut self, expected: usize, got: usize, idx: NodeIndex);

    /// Emit an "expected at least N arguments" error (TS2555).
    fn emit_expected_at_least_arguments(&mut self, expected_min: usize, got: usize, idx: NodeIndex);

    /// Emit a "type is not callable" error.
    fn emit_not_callable(&mut self, type_id: TypeId, idx: NodeIndex);

    // =========================================================================
    // Name Resolution Error Patterns
    // =========================================================================

    /// Emit a "cannot find name" error (TS2304).
    fn emit_cannot_find_name(&mut self, name: &str, idx: NodeIndex);

    /// Emit a "cannot find name" error with suggestions (TS2552).
    fn emit_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    );

    /// Emit a "cannot find global type" error with lib suggestion.
    fn emit_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex);

    /// Emit a "cannot find name - change lib" error (TS2583).
    fn emit_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex);

    // =========================================================================
    // Constructor/Class Error Patterns
    // =========================================================================

    /// Emit a "class constructor without new" error (TS2350).
    fn emit_class_constructor_without_new(&mut self, type_id: TypeId, idx: NodeIndex);

    /// Emit a "cannot instantiate abstract class" error (TS2511).
    fn emit_cannot_instantiate_abstract_class(&mut self, class_name: &str, idx: NodeIndex);

    /// Emit an "abstract property in constructor" error (TS2715).
    fn emit_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    );

    // =========================================================================
    // Variable/Declaration Error Patterns
    // =========================================================================

    /// Emit a "variable used before assignment" error (TS2454).
    fn emit_variable_used_before_assigned(&mut self, name: &str, idx: NodeIndex);

    /// Emit a "subsequent variable declarations" error (TS2454).
    fn emit_subsequent_variable_declaration(
        &mut self,
        name: &str,
        type1: TypeId,
        type2: TypeId,
        idx: NodeIndex,
    );

    // =========================================================================
    // Generic Error Patterns
    // =========================================================================

    /// Emit a "type constraint not satisfied" error (TS2344).
    fn emit_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    );

    /// Emit a "generic type requires type arguments" error (TS2314).
    fn emit_generic_type_requires_type_arguments(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    );

    // =========================================================================
    // Type/Value Mismatch Error Patterns
    // =========================================================================

    /// Emit a "type only refers to a type" error (TS2693/TS2585).
    fn emit_type_only_value(&mut self, name: &str, idx: NodeIndex);

    /// Emit a "value only refers to a type" error (TS2749).
    fn emit_value_only_type(&mut self, name: &str, idx: NodeIndex);

    // =========================================================================
    // Module/Namespace Error Patterns
    // =========================================================================

    /// Emit a "namespace has no exported member" error (TS2694).
    fn emit_namespace_no_export(&mut self, namespace_name: &str, member_name: &str, idx: NodeIndex);

    // =========================================================================
    // Getter/Setter Error Patterns
    // =========================================================================

    /// Emit a "get accessor must return a value" error (TS2378).
    fn emit_get_accessor_must_return(&mut self, idx: NodeIndex);

    /// Emit a "private method not writable" error (TS2803).
    fn emit_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex);
}

// =============================================================================
// Diagnostic Builder
// =============================================================================

/// Builder for creating diagnostics with a fluent API.
///
/// This provides a more ergonomic way to construct diagnostics,
/// reducing boilerplate and ensuring all required fields are set.
pub struct DiagnosticBuilder<'a> {
    file_name: &'a str,
    start: u32,
    length: u32,
    message: String,
    code: u32,
}

impl<'a> DiagnosticBuilder<'a> {
    /// Create a new diagnostic builder.
    pub fn new(file_name: &'a str) -> Self {
        Self {
            file_name,
            start: 0,
            length: 0,
            message: String::new(),
            code: 0,
        }
    }

    /// Set the position for the diagnostic.
    pub fn position(mut self, start: u32, length: u32) -> Self {
        self.start = start;
        self.length = length;
        self
    }

    /// Set the message for the diagnostic.
    pub fn message(mut self, message: impl AsRef<str>) -> Self {
        self.message = message.as_ref().to_string();
        self
    }

    /// Set the error code for the diagnostic.
    pub fn code(mut self, code: u32) -> Self {
        self.code = code;
        self
    }

    /// Build the diagnostic.
    pub fn build(self) -> Diagnostic {
        Diagnostic {
            file: self.file_name.to_string(),
            start: self.start,
            length: self.length,
            message_text: self.message,
            category: DiagnosticCategory::Error,
            code: self.code,
            related_information: Vec::new(),
        }
    }
}

impl<'a> ErrorHandler for CheckerState<'a> {
    // =========================================================================
    // Core Error Emission
    // =========================================================================

    fn emit_error(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        match self.get_node_span(node_idx) {
            Some((start, end)) => {
                let length = end.saturating_sub(start);
                self.emit_error_at(start, length, message, code);
            }
            None => {}
        }
    }

    fn emit_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
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

    fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.ctx.diagnostics.push(diagnostic);
    }

    // =========================================================================
    // Type Error Patterns
    // =========================================================================

    fn emit_type_not_assignable(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        // Error type suppression: don't emit ERROR type errors
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return;
        }
        // ANY type suppression: any is assignable to/from everything
        if source == TypeId::ANY || target == TypeId::ANY {
            return;
        }
        // UNKNOWN type suppression: prevents cascade errors from unresolved types
        if source == TypeId::UNKNOWN || target == TypeId::UNKNOWN {
            return;
        }

        self.error_type_not_assignable_at(source, target, idx);
    }

    fn emit_type_not_assignable_with_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
        _reason: &str,
        idx: NodeIndex,
    ) {
        // Error type suppression
        if source == TypeId::ERROR || target == TypeId::ERROR {
            return;
        }
        // ANY type suppression
        if source == TypeId::ANY || target == TypeId::ANY {
            return;
        }
        // UNKNOWN type suppression: prevents cascade errors from unresolved types
        if source == TypeId::UNKNOWN || target == TypeId::UNKNOWN {
            return;
        }

        self.error_type_not_assignable_with_reason_at(source, target, idx);
    }

    // =========================================================================
    // Property Access Error Patterns
    // =========================================================================

    fn emit_property_not_exist(&mut self, prop_name: &str, type_id: TypeId, idx: NodeIndex) {
        self.error_property_not_exist_at(prop_name, type_id, idx);
    }

    fn emit_property_missing(
        &mut self,
        prop_name: &str,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_property_missing_at(prop_name, source_type, target_type, idx);
    }

    fn emit_readonly_property(&mut self, prop_name: &str, idx: NodeIndex) {
        self.error_readonly_property_at(prop_name, idx);
    }

    fn emit_excess_property(&mut self, prop_name: &str, target_type: TypeId, idx: NodeIndex) {
        self.error_excess_property_at(prop_name, target_type, idx);
    }

    // =========================================================================
    // Function Call Error Patterns
    // =========================================================================

    fn emit_argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_argument_not_assignable_at(arg_type, param_type, idx);
    }

    fn emit_argument_count_mismatch(&mut self, expected: usize, got: usize, idx: NodeIndex) {
        self.error_argument_count_mismatch_at(expected, got, idx);
    }

    fn emit_expected_at_least_arguments(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        self.error_expected_at_least_arguments_at(expected_min, got, idx);
    }

    fn emit_not_callable(&mut self, type_id: TypeId, idx: NodeIndex) {
        self.error_not_callable_at(type_id, idx);
    }

    // =========================================================================
    // Name Resolution Error Patterns
    // =========================================================================

    fn emit_cannot_find_name(&mut self, name: &str, idx: NodeIndex) {
        self.error_cannot_find_name_at(name, idx);
    }

    fn emit_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        self.error_cannot_find_name_with_suggestions(name, suggestions, idx);
    }

    fn emit_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        self.error_cannot_find_global_type(name, idx);
    }

    fn emit_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        self.error_cannot_find_name_change_lib(name, idx);
    }

    // =========================================================================
    // Constructor/Class Error Patterns
    // =========================================================================

    fn emit_class_constructor_without_new(&mut self, type_id: TypeId, idx: NodeIndex) {
        self.error_class_constructor_without_new_at(type_id, idx);
    }

    fn emit_cannot_instantiate_abstract_class(&mut self, class_name: &str, idx: NodeIndex) {
        self.report_cannot_instantiate_abstract_class(class_name, idx);
    }

    fn emit_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        self.error_abstract_property_in_constructor(prop_name, class_name, idx);
    }

    // =========================================================================
    // Variable/Declaration Error Patterns
    // =========================================================================

    fn emit_variable_used_before_assigned(&mut self, name: &str, idx: NodeIndex) {
        self.error_variable_used_before_assigned_at(name, idx);
    }

    fn emit_subsequent_variable_declaration(
        &mut self,
        name: &str,
        type1: TypeId,
        type2: TypeId,
        idx: NodeIndex,
    ) {
        self.error_subsequent_variable_declaration(name, type1, type2, idx);
    }

    // =========================================================================
    // Generic Error Patterns
    // =========================================================================

    fn emit_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        self.error_type_constraint_not_satisfied(type_arg, constraint, idx);
    }

    fn emit_generic_type_requires_type_arguments(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        self.error_generic_type_requires_type_arguments_at(name, required_count, idx);
    }

    // =========================================================================
    // Type/Value Mismatch Error Patterns
    // =========================================================================

    fn emit_type_only_value(&mut self, name: &str, idx: NodeIndex) {
        self.error_type_only_value_at(name, idx);
    }

    fn emit_value_only_type(&mut self, name: &str, idx: NodeIndex) {
        self.error_value_only_type_at(name, idx);
    }

    // =========================================================================
    // Module/Namespace Error Patterns
    // =========================================================================

    fn emit_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        self.error_namespace_no_export(namespace_name, member_name, idx);
    }

    // =========================================================================
    // Getter/Setter Error Patterns
    // =========================================================================

    fn emit_get_accessor_must_return(&mut self, idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        match self.get_node_span(idx) {
            Some((start, end)) => {
                let length = end.saturating_sub(start);
                let file = &self.ctx.file_name;
                self.ctx.diagnostics.push(Diagnostic {
                    file: file.clone(),
                    start,
                    length,
                    message_text: diagnostic_messages::GET_ACCESSOR_MUST_RETURN_VALUE.to_string(),
                    category: crate::checker::types::diagnostics::DiagnosticCategory::Error,
                    code: diagnostic_codes::GET_ACCESSOR_MUST_RETURN_VALUE,
                    related_information: Vec::new(),
                });
            }
            None => {}
        }
    }

    fn emit_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        self.error_private_method_not_writable(prop_name, idx);
    }
}

// =============================================================================
// Re-export the ErrorHandler implementation
// =============================================================================

impl<'a> ErrorHandler for &mut CheckerState<'a> {
    fn emit_error(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        (*self).emit_error(node_idx, message, code);
    }

    fn emit_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        (*self).emit_error_at(start, length, message, code);
    }

    fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        (*self).emit_diagnostic(diagnostic);
    }

    fn emit_type_not_assignable(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        (*self).emit_type_not_assignable(source, target, idx);
    }

    fn emit_type_not_assignable_with_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
        reason: &str,
        idx: NodeIndex,
    ) {
        (*self).emit_type_not_assignable_with_reason(source, target, reason, idx);
    }

    fn emit_property_not_exist(&mut self, prop_name: &str, type_id: TypeId, idx: NodeIndex) {
        (*self).emit_property_not_exist(prop_name, type_id, idx);
    }

    fn emit_property_missing(
        &mut self,
        prop_name: &str,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        (*self).emit_property_missing(prop_name, source_type, target_type, idx);
    }

    fn emit_readonly_property(&mut self, prop_name: &str, idx: NodeIndex) {
        (*self).emit_readonly_property(prop_name, idx);
    }

    fn emit_excess_property(&mut self, prop_name: &str, target_type: TypeId, idx: NodeIndex) {
        (*self).emit_excess_property(prop_name, target_type, idx);
    }

    fn emit_argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        (*self).emit_argument_not_assignable(arg_type, param_type, idx);
    }

    fn emit_argument_count_mismatch(&mut self, expected: usize, got: usize, idx: NodeIndex) {
        (*self).emit_argument_count_mismatch(expected, got, idx);
    }

    fn emit_expected_at_least_arguments(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        (*self).emit_expected_at_least_arguments(expected_min, got, idx);
    }

    fn emit_not_callable(&mut self, type_id: TypeId, idx: NodeIndex) {
        (*self).emit_not_callable(type_id, idx);
    }

    fn emit_cannot_find_name(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_cannot_find_name(name, idx);
    }

    fn emit_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        (*self).emit_cannot_find_name_with_suggestions(name, suggestions, idx);
    }

    fn emit_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_cannot_find_global_type(name, idx);
    }

    fn emit_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_cannot_find_name_change_lib(name, idx);
    }

    fn emit_class_constructor_without_new(&mut self, type_id: TypeId, idx: NodeIndex) {
        (*self).emit_class_constructor_without_new(type_id, idx);
    }

    fn emit_cannot_instantiate_abstract_class(&mut self, class_name: &str, idx: NodeIndex) {
        (*self).emit_cannot_instantiate_abstract_class(class_name, idx);
    }

    fn emit_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        (*self).emit_abstract_property_in_constructor(prop_name, class_name, idx);
    }

    fn emit_variable_used_before_assigned(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_variable_used_before_assigned(name, idx);
    }

    fn emit_subsequent_variable_declaration(
        &mut self,
        name: &str,
        type1: TypeId,
        type2: TypeId,
        idx: NodeIndex,
    ) {
        (*self).emit_subsequent_variable_declaration(name, type1, type2, idx);
    }

    fn emit_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        (*self).emit_type_constraint_not_satisfied(type_arg, constraint, idx);
    }

    fn emit_generic_type_requires_type_arguments(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        (*self).emit_generic_type_requires_type_arguments(name, required_count, idx);
    }

    fn emit_type_only_value(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_type_only_value(name, idx);
    }

    fn emit_value_only_type(&mut self, name: &str, idx: NodeIndex) {
        (*self).emit_value_only_type(name, idx);
    }

    fn emit_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        (*self).emit_namespace_no_export(namespace_name, member_name, idx);
    }

    fn emit_get_accessor_must_return(&mut self, idx: NodeIndex) {
        (*self).emit_get_accessor_must_return(idx);
    }

    fn emit_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        (*self).emit_private_method_not_writable(prop_name, idx);
    }
}
