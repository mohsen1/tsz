//! Error Reporter Module
//!
//! This module contains all error reporting methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! ## Naming Convention
//!
//! - `error_*` methods: Core error emission functions
//! - `report_*` methods: Higher-level wrapper methods with additional logic
//!
//! This module extends CheckerState with additional impl blocks rather than moving
//! existing code, to maintain backward compatibility during the refactoring.

use crate::checker::state::{CheckerState, MemberAccessLevel};
use crate::checker::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::parser::NodeIndex;
use crate::solver::TypeId;
use tracing::{Level, trace};

// =============================================================================
// Core Error Emission (Low-Level)
// =============================================================================
//
// These methods directly create and emit diagnostics. They are the foundation
// for all error reporting in the type checker.

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Fundamental Error Emitters
    // =========================================================================

    /// Report an error at a specific node.
    pub(crate) fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let length = end.saturating_sub(start);
            // Use the error() function which has deduplication by (start, code)
            self.error(start, length, message.to_string(), code);
        }
    }

    /// Report an error at a specific position.
    pub(crate) fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
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

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    pub(crate) fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }

    // =========================================================================
    // Type Assignability Errors
    // =========================================================================

    /// Report a type not assignable error (delegates to diagnose_assignment_failure).
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        // ERROR TYPE SUPPRESSION
        if source == TypeId::ERROR || target == TypeId::ERROR {
            if tracing::enabled!(Level::TRACE) {
                trace!(
                    source = source.0,
                    target = target.0,
                    node_idx = idx.0,
                    file = %self.ctx.file_name,
                    "suppressing TS2322 for error type"
                );
            }
            return;
        }

        // ANY TYPE SUPPRESSION
        if source == TypeId::ANY || target == TypeId::ANY {
            return;
        }

        // UNKNOWN TYPE SUPPRESSION - when types couldn't be fully resolved
        // (e.g., from unresolved imports, incomplete lib loading), suppress
        // to prevent false positive cascading errors
        if source == TypeId::UNKNOWN || target == TypeId::UNKNOWN {
            return;
        }

        // Check for constructor accessibility mismatch
        if let Some((source_level, target_level)) =
            self.constructor_accessibility_mismatch(source, target, None)
        {
            self.error_constructor_accessibility_not_assignable(
                source,
                target,
                source_level,
                target_level,
                idx,
            );
            return;
        }

        // Check for private brand mismatch
        if let Some(detail) = self.private_brand_mismatch_error(source, target) {
            let Some(loc) = self.get_node_span(idx) else {
                return;
            };

            let source_type = self.format_type(source);
            let target_type = self.format_type(target);
            let message = format_message(
                diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                &[&source_type, &target_type],
            );

            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.0,
                loc.1 - loc.0,
                message,
                diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
            )
            .with_related(self.ctx.file_name.clone(), loc.0, loc.1 - loc.0, detail);

            self.ctx.diagnostics.push(diag);
            return;
        }

        // Use the solver's explain API to get the detailed reason
        // Use the type environment to resolve TypeQuery and Ref types
        let reason = {
            let env = self.ctx.type_env.borrow();
            let mut checker = crate::solver::CompatChecker::with_resolver(self.ctx.types, &*env);
            self.ctx.configure_compat_checker(&mut checker);
            checker.explain_failure(source, target)
        };

        if tracing::enabled!(Level::TRACE) {
            let source_type = self.format_type(source);
            let target_type = self.format_type(target);
            let reason_ref = reason.as_ref();
            trace!(
                source = %source_type,
                target = %target_type,
                reason = ?reason_ref,
                node_idx = idx.0,
                file = %self.ctx.file_name,
                "assignability failure diagnostics"
            );
        }

        match reason {
            Some(failure_reason) => {
                let diag = self.render_failure_reason(&failure_reason, source, target, idx, 0);
                self.ctx.diagnostics.push(diag);
            }
            None => {
                // Fallback to generic message
                self.error_type_not_assignable_generic_at(source, target, idx);
            }
        }
    }

    /// Internal generic error reporting for type assignability failures.
    pub(crate) fn error_type_not_assignable_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            || source == TypeId::ANY
            || target == TypeId::ANY
            || source == TypeId::UNKNOWN
            || target == TypeId::UNKNOWN
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.type_not_assignable(source, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Recursively render a SubtypeFailureReason into a Diagnostic.
    fn render_failure_reason(
        &self,
        reason: &crate::solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        use crate::solver::SubtypeFailureReason;

        let (start, length) = self.get_node_span(idx).unwrap_or((0, 0));
        let file_name = self.ctx.file_name.clone();

        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let source_str = self.format_type(*source_type);
                let target_str = self.format_type(*target_type);
                let message = format_message(
                    diagnostic_messages::PROPERTY_MISSING_BUT_REQUIRED,
                    &[&prop_name, &source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::PROPERTY_MISSING_IN_TYPE,
                )
            }

            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => {
                // At depth 0, emit TS2322 as the primary error (matching tsc behavior).
                // TS2326 details go into related_information.
                if depth == 0 {
                    let source_str = self.format_type(source);
                    let target_str = self.format_type(target);
                    let message = format_message(
                        diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name.clone(),
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                    );

                    // Add property incompatibility as related info
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let prop_message = format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_INCOMPATIBLE,
                        &[&prop_name],
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: prop_message,
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::TYPES_OF_PROPERTY_INCOMPATIBLE,
                    });

                    if let Some(nested) = nested_reason {
                        let nested_diag = self.render_failure_reason(
                            nested,
                            *source_property_type,
                            *target_property_type,
                            idx,
                            depth + 1,
                        );
                        diag.related_information.push(DiagnosticRelatedInformation {
                            file: nested_diag.file,
                            start: nested_diag.start,
                            length: nested_diag.length,
                            message_text: nested_diag.message_text,
                            category: DiagnosticCategory::Message,
                            code: nested_diag.code,
                        });
                    }
                    return diag;
                }

                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message = format_message(
                    diagnostic_messages::TYPES_OF_PROPERTY_INCOMPATIBLE,
                    &[&prop_name],
                );
                let mut diag = Diagnostic::error(
                    file_name.clone(),
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPES_OF_PROPERTY_INCOMPATIBLE,
                );

                if let Some(nested) = nested_reason
                    && depth < 5
                {
                    let nested_diag = self.render_failure_reason(
                        nested,
                        *source_property_type,
                        *target_property_type,
                        idx,
                        depth + 1,
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: nested_diag.file,
                        start: nested_diag.start,
                        length: nested_diag.length,
                        message_text: nested_diag.message_text,
                        category: DiagnosticCategory::Message,
                        code: nested_diag.code,
                    });
                }
                diag
            }

            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let source_str = self.format_type(source);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::PROPERTY_MISSING_BUT_REQUIRED,
                    &[&prop_name, &source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::PROPERTY_MISSING_IN_TYPE,
                )
            }

            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message =
                    format_message(diagnostic_messages::CANNOT_ASSIGN_READONLY, &[&prop_name]);
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::CANNOT_ASSIGN_TO_READONLY_PROPERTY,
                )
            }

            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let target_str = self.format_type(*target_type);
                let message = format_message(
                    diagnostic_messages::EXCESS_PROPERTY,
                    &[&prop_name, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::EXCESS_PROPERTY_CHECK,
                )
            }

            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => {
                let source_str = self.format_type(*source_return);
                let target_str = self.format_type(*target_return);
                let message = format!(
                    "Return type '{}' is not assignable to '{}'.",
                    source_str, target_str
                );
                let mut diag = Diagnostic::error(
                    file_name.clone(),
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                );

                if let Some(nested) = nested_reason
                    && depth < 5
                {
                    let nested_diag = self.render_failure_reason(
                        nested,
                        *source_return,
                        *target_return,
                        idx,
                        depth + 1,
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: nested_diag.file,
                        start: nested_diag.start,
                        length: nested_diag.length,
                        message_text: nested_diag.message_text,
                        category: DiagnosticCategory::Message,
                        code: nested_diag.code,
                    });
                }
                diag
            }

            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            } => {
                let message = format_message(
                    diagnostic_messages::EXPECTED_ARGUMENTS,
                    &[&target_count.to_string(), &source_count.to_string()],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::EXPECTED_ARGUMENTS,
                )
            }

            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                let message = format!(
                    "Tuple type has {} elements but target requires {}.",
                    source_count, target_count
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::TupleElementTypeMismatch {
                index,
                source_element,
                target_element,
            } => {
                let source_str = self.format_type(*source_element);
                let target_str = self.format_type(*target_element);
                let message = format!(
                    "Type of element at index {} is incompatible: '{}' is not assignable to '{}'.",
                    index, source_str, target_str
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => {
                let source_str = self.format_type(*source_element);
                let target_str = self.format_type(*target_element);
                let message = format!(
                    "Array element type '{}' is not assignable to '{}'.",
                    source_str, target_str
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind,
                source_value_type,
                target_value_type,
            } => {
                let source_str = self.format_type(*source_value_type);
                let target_str = self.format_type(*target_value_type);
                let message = format!(
                    "{} index signature is incompatible: '{}' is not assignable to '{}'.",
                    index_kind, source_str, target_str
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let source_str = self.format_type(*source_type);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::NoCommonProperties {
                source_type: _,
                target_type: _,
            } => {
                let source_str = self.format_type(source);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            _ => {
                let source_str = self.format_type(source);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_NOT_ASSIGNABLE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
        }
    }

    /// Report a type not assignable error with detailed elaboration.
    ///
    /// This method uses the solver's "explain" API to determine WHY the types
    /// are incompatible (e.g., missing property, incompatible property types,
    /// etc.) and produces a richer diagnostic with that information.
    ///
    /// **Architecture Note**: This follows the "Check Fast, Explain Slow" pattern.
    /// The `is_assignable_to` check is fast (boolean). This explain call is slower
    /// but produces better error messages. Only call this after a failed check.
    pub fn error_type_not_assignable_with_reason_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Report constructor accessibility mismatch error.
    pub(crate) fn error_constructor_accessibility_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_level: Option<MemberAccessLevel>,
        target_level: Option<MemberAccessLevel>,
        idx: NodeIndex,
    ) {
        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let source_type = self.format_type(source);
        let target_type = self.format_type(target);
        let message = format_message(
            diagnostic_messages::TYPE_NOT_ASSIGNABLE,
            &[&source_type, &target_type],
        );
        let detail = format!(
            "Cannot assign a '{}' constructor type to a '{}' constructor type.",
            Self::constructor_access_name(source_level),
            Self::constructor_access_name(target_level),
        );

        let diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message,
            diagnostic_codes::TYPE_NOT_ASSIGNABLE_TO_TYPE,
        )
        .with_related(self.ctx.file_name.clone(), loc.start, loc.length(), detail);
        self.ctx.diagnostics.push(diag);
    }

    // =========================================================================
    // Property Errors
    // =========================================================================

    /// Report a property missing error using solver diagnostics with source tracking.
    pub fn error_property_missing_at(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            || source == TypeId::ANY
            || target == TypeId::ANY
            || source == TypeId::UNKNOWN
            || target == TypeId::UNKNOWN
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.property_missing(prop_name, source, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a property not exist error using solver diagnostics with source tracking.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        use crate::solver::type_queries;

        // Suppress error if type is ERROR/ANY or an Error type wrapper
        // This prevents cascading errors when accessing properties on error types
        // NOTE: We do NOT suppress for UNKNOWN - accessing properties on unknown should error (TS2339)
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || type_queries::is_error_type(self.ctx.types, type_id)
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.property_not_exist(prop_name, type_id, loc.start, loc.length());
            // Use push_diagnostic for deduplication
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report an excess property error using solver diagnostics with source tracking.
    pub fn error_excess_property_at(&mut self, prop_name: &str, target: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if target == TypeId::ERROR || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.excess_property(prop_name, target, loc.start, loc.length());
            // Use push_diagnostic for deduplication
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a "Cannot assign to readonly property" error using solver diagnostics with source tracking.
    pub fn error_readonly_property_at(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.readonly_property(prop_name, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS2803: Cannot assign to private method. Private methods are not writable.
    pub fn error_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_ASSIGN_PRIVATE_METHOD,
                &[prop_name],
            );
            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_ASSIGN_TO_PRIVATE_METHOD,
            );
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Report no index signature error.
    pub(crate) fn error_no_index_signature_at(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        idx: NodeIndex,
    ) {
        // TS7053 is a noImplicitAny error - suppress without it
        if !self.ctx.no_implicit_any() {
            return;
        }
        // Suppress when types are unresolved
        if index_type == TypeId::ANY || index_type == TypeId::ERROR || index_type == TypeId::UNKNOWN
        {
            return;
        }
        if object_type == TypeId::ANY
            || object_type == TypeId::ERROR
            || object_type == TypeId::UNKNOWN
        {
            return;
        }

        let mut formatter = self.ctx.create_type_formatter();
        let index_str = formatter.format(index_type);
        let object_str = formatter.format(object_type);
        let message = format!(
            "Element implicitly has an 'any' type because expression of type '{}' can't be used to index type '{}'.",
            index_str, object_str
        );

        self.error_at_node(idx, &message, diagnostic_codes::NO_INDEX_SIGNATURE);
    }

    // =========================================================================
    // Name Resolution Errors
    // =========================================================================

    /// Report a cannot find name error using solver diagnostics with source tracking.
    /// Enhanced to provide suggestions for similar names, import suggestions, and
    /// library change suggestions for ES2015+ types.
    pub fn error_cannot_find_name_at(&mut self, name: &str, idx: NodeIndex) {
        use crate::lib_loader;

        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(") that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1136 "Property assignment expected").
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        // Check if this is an ES2015+ type that requires a specific lib
        // If so, emit TS2583 with a suggestion to change the lib
        if lib_loader::is_es2015_plus_type(name) {
            self.error_cannot_find_name_change_lib(name, idx);
            return;
        }

        // Check if this is a known DOM/ScriptHost global that requires the 'dom' lib
        // If so, emit TS2584 with a suggestion to include 'dom'
        if is_known_dom_global(name) {
            self.error_cannot_find_name_change_target_lib(name, idx);
            return;
        }

        // Try to find similar identifiers in scope for better error messages
        if let Some(suggestions) = self.find_similar_identifiers(name, idx)
            && !suggestions.is_empty()
        {
            // Use the first suggestion for "Did you mean?" error
            self.error_cannot_find_name_with_suggestions(name, &suggestions, idx);
            return;
        }

        // Fall back to standard error without suggestions
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.cannot_find_name(name, loc.start, loc.length());
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report error 2318/2583: Cannot find global type 'X'.
    /// - TS2318: Cannot find global type (for @noLib tests)
    /// - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)
    pub fn error_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        use crate::lib_loader;

        // Check if this is an ES2015+ type that would require a specific lib
        let is_es2015_type = lib_loader::is_es2015_plus_type(name);

        if let Some(loc) = self.get_source_location(idx) {
            let (code, message) = if is_es2015_type {
                (
                    lib_loader::MISSING_ES2015_LIB_SUPPORT,
                    format!(
                        "Cannot find name '{}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later.",
                        name
                    ),
                )
            } else {
                (
                    lib_loader::CANNOT_FIND_GLOBAL_TYPE,
                    format!("Cannot find global type '{}'.", name),
                )
            };

            self.ctx.push_diagnostic(Diagnostic {
                code,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2583: Cannot find name 'X' - suggest changing target library.
    ///
    /// This error is emitted when an ES2015+ global (Promise, Map, Set, Symbol, etc.)
    /// is used as a value but is not available in the current lib configuration.
    /// It provides a helpful suggestion to change the lib compiler option.
    pub fn error_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(diagnostic_messages::CANNOT_FIND_NAME_CHANGE_LIB, &[name]);
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_CHANGE_LIB,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2584: Cannot find name 'X' - suggest including 'dom' lib.
    ///
    /// This error is emitted when a known DOM/ScriptHost global (console, window,
    /// document, HTMLElement, etc.) is used but the 'dom' lib is not included.
    pub fn error_cannot_find_name_change_target_lib(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_CHANGE_TARGET_LIB,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_CHANGE_TARGET_LIB,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2304/2552: Cannot find name 'X' with suggestions.
    /// Provides a list of similar names that might be what the user intended.
    pub fn error_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors that were added to the AST for error recovery.
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Format the suggestions list
            let suggestions_text = if suggestions.len() == 1 {
                format!("'{}'", suggestions[0])
            } else {
                let formatted: Vec<String> =
                    suggestions.iter().map(|s| format!("'{}", s)).collect();
                formatted.join(", ")
            };

            let message = if suggestions.len() == 1 {
                format!(
                    "Cannot find name '{}'. Did you mean {}?",
                    name, suggestions_text
                )
            } else {
                format!(
                    "Cannot find name '{}'. Did you mean one of: {}?",
                    name, suggestions_text
                )
            };

            self.ctx.push_diagnostic(Diagnostic {
                code: if suggestions.len() == 1 {
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
                } else {
                    diagnostic_codes::CANNOT_FIND_NAME
                },
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2552: Cannot find name 'X'. Did you mean 'Y'?
    pub fn error_cannot_find_name_did_you_mean_at(
        &mut self,
        name: &str,
        suggestion: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Cannot find name '{}'. Did you mean '{}'?",
                name, suggestion
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2662: Cannot find name 'X'. Did you mean the static member 'C.X'?
    pub fn error_cannot_find_name_static_member_at(
        &mut self,
        name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Cannot find name '{}'. Did you mean the static member '{}.{}'?",
                name, class_name, name
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_STATIC,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Identifier Suggestion Helpers
    // =========================================================================

    /// Find identifiers in scope that are similar to the given name.
    /// Returns a list of suggestions sorted by similarity (empty if none found).
    pub(crate) fn find_similar_identifiers(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<Vec<String>> {
        let mut suggestions = Vec::new();

        let visible_names = self
            .ctx
            .binder
            .collect_visible_symbol_names(self.ctx.arena, idx);
        for symbol_name in visible_names {
            if symbol_name != name {
                let similarity = self.calculate_string_similarity(name, &symbol_name);
                // Use a high threshold (0.85) to match TypeScript's conservative suggestions
                // TypeScript only suggests names that are very similar (case changes, typos)
                if similarity > 0.85 {
                    suggestions.push((symbol_name, similarity));
                }
            }
        }

        // Sort by similarity (descending) and take top 3
        suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.truncate(3);

        if suggestions.is_empty() {
            None
        } else {
            Some(suggestions.into_iter().map(|(n, _)| n).collect())
        }
    }

    /// Calculate string similarity using a simple edit distance algorithm.
    /// Returns a value between 0.0 (no similarity) and 1.0 (exact match).
    fn calculate_string_similarity(&self, a: &str, b: &str) -> f64 {
        if a == b {
            return 1.0;
        }

        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();

        if a_lower == b_lower {
            return 0.95; // Very similar, just different case
        }

        // Check for prefix/suffix similarity
        if a_lower.starts_with(&b_lower) || b_lower.starts_with(&a_lower) {
            return 0.8;
        }

        // Simple Levenshtein distance
        let max_len = a_lower.len().max(b_lower.len());
        if max_len == 0 {
            return 1.0;
        }

        let distance = self.levenshtein_distance(&a_lower, &b_lower);

        1.0 - (distance as f64 / max_len as f64)
    }

    /// Calculate Levenshtein distance between two strings.
    fn levenshtein_distance(&self, a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let a_len = a_chars.len();
        let b_len = b_chars.len();

        if a_len == 0 {
            return b_len;
        }
        if b_len == 0 {
            return a_len;
        }

        let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

        // Initialize first row and column
        for i in 0..=a_len {
            matrix[i][0] = i;
        }
        for j in 0..=b_len {
            matrix[0][j] = j;
        }

        // Fill the matrix
        for i in 1..=a_len {
            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                matrix[i][j] = [
                    matrix[i - 1][j] + 1,        // deletion
                    matrix[i][j - 1] + 1,        // insertion
                    matrix[i - 1][j - 1] + cost, // substitution
                ]
                .iter()
                .min()
                .copied()
                .unwrap_or_else(|| {
                    // This should never happen as we have a non-empty array
                    // but provide a safe fallback
                    usize::MAX
                });
            }
        }

        matrix[a_len][b_len]
    }

    // =========================================================================
    // Function Call Errors
    // =========================================================================

    /// Report an argument not assignable error using solver diagnostics with source tracking.
    pub fn error_argument_not_assignable_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascading errors when either type is ERROR, ANY, or UNKNOWN
        if arg_type == TypeId::ERROR || param_type == TypeId::ERROR {
            return;
        }
        if arg_type == TypeId::ANY || param_type == TypeId::ANY {
            return;
        }
        if arg_type == TypeId::UNKNOWN || param_type == TypeId::UNKNOWN {
            return;
        }
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag =
                builder.argument_not_assignable(arg_type, param_type, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report an argument count mismatch error using solver diagnostics with source tracking.
    /// TS2554: Expected {0} arguments, but got {1}.
    pub fn error_argument_count_mismatch_at(
        &mut self,
        expected: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.argument_count_mismatch(expected, got, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a spread argument type error (TS2556).
    /// TS2556: A spread argument must either have a tuple type or be passed to a rest parameter.
    pub fn error_spread_must_be_tuple_or_rest_at(&mut self, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SPREAD_MUST_BE_TUPLE_OR_REST,
                category: DiagnosticCategory::Error,
                message_text: diagnostic_messages::SPREAD_MUST_BE_TUPLE_OR_REST.to_string(),
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report an "expected at least N arguments" error (TS2555).
    /// TS2555: Expected at least {0} arguments, but got {1}.
    pub fn error_expected_at_least_arguments_at(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Expected at least {} arguments, but got {}.",
                expected_min, got
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report "No overload matches this call" with related overload failures.
    pub fn error_no_overload_matches_at(
        &mut self,
        idx: NodeIndex,
        failures: &[crate::solver::PendingDiagnostic],
    ) {
        use crate::solver::PendingDiagnostic;

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let mut related = Vec::new();
        let span =
            crate::solver::SourceSpan::new(self.ctx.file_name.as_str(), loc.start, loc.length());

        for failure in failures {
            let pending = PendingDiagnostic {
                span: Some(span.clone()),
                ..failure.clone()
            };
            let diag = formatter.render(&pending);
            if let Some(diag_span) = diag.span.as_ref() {
                related.push(DiagnosticRelatedInformation {
                    file: diag_span.file.to_string(),
                    start: diag_span.start,
                    length: diag_span.length,
                    message_text: diag.message.clone(),
                    category: DiagnosticCategory::Message,
                    code: diag.code,
                });
            }
        }

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::NO_OVERLOAD_MATCHES_CALL,
            category: DiagnosticCategory::Error,
            message_text: diagnostic_messages::NO_OVERLOAD_MATCHES.to_string(),
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: related,
        });
    }

    /// Report a "type is not callable" error using solver diagnostics with source tracking.
    pub fn error_not_callable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = crate::solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.not_callable(type_id, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS6234: "This expression is not callable because it is a 'get' accessor.
    /// Did you mean to access it without '()'?"
    pub fn error_get_accessor_not_callable_at(&mut self, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.ctx.diagnostics.push(
                crate::checker::types::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    "This expression is not callable because it is a 'get' accessor. Did you mean to access it without '()'?".to_string(),
                    diagnostic_codes::GET_ACCESSOR_NOT_CALLABLE,
                ),
            );
        }
    }

    /// Report TS2348: "Cannot invoke an expression whose type lacks a call signature"
    /// This is specifically for class constructors called without 'new'.
    pub fn error_class_constructor_without_new_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message = diagnostic_messages::CANNOT_INVOKE_EXPRESSION_LACKING_CALL_SIGNATURE
            .replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::CANNOT_INVOKE_EXPRESSION_WHOSE_TYPE_LACKS_CALL_SIGNATURE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report TS2506: Circular class inheritance (class C extends C).
    pub(crate) fn error_circular_class_inheritance(
        &mut self,
        extends_expr_idx: NodeIndex,
        class_idx: NodeIndex,
    ) {
        // Get the class name for the error message
        let class_name = if let Some(class_node) = self.ctx.arena.get(class_idx)
            && let Some(class) = self.ctx.arena.get_class(class_node)
            && !class.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(class.name)
        {
            self.ctx
                .arena
                .get_identifier(name_node)
                .map(|id| id.escaped_text.clone())
        } else {
            None
        };

        let name = class_name.unwrap_or_else(|| String::from("<class>"));

        let Some(loc) = self.get_source_location(extends_expr_idx) else {
            return;
        };

        let message = format_message(diagnostic_messages::CIRCULAR_BASE_REFERENCE, &[&name]);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::CIRCULAR_BASE_REFERENCE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report TS2507: "Type 'X' is not a constructor function type"
    /// This is for extends clauses where the base type isn't a constructor.
    pub fn error_not_a_constructor_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress error if type is ERROR/ANY/UNKNOWN - prevents cascading errors
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE.replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    /// Report TS2351: "This expression is not constructable. Type 'X' has no construct signatures."
    /// This is for `new` expressions where the expression type has no construct signatures.
    pub fn error_not_constructable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        if type_id == TypeId::ERROR || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return;
        }

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let type_str = formatter.format(type_id);

        let message =
            diagnostic_messages::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE.replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::THIS_EXPRESSION_IS_NOT_CONSTRUCTABLE,
            category: DiagnosticCategory::Error,
            message_text: message,
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: Vec::new(),
        });
    }

    // =========================================================================
    // Binary Operator Errors
    // =========================================================================

    /// Emit errors for binary operator type mismatches.
    /// Emits TS18050 for null/undefined operands, TS2362 for left-hand side,
    /// TS2363 for right-hand side, or TS2365 for general operator errors.
    pub(crate) fn emit_binary_operator_error(
        &mut self,
        node_idx: NodeIndex,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
        left_type: TypeId,
        right_type: TypeId,
        op: &str,
    ) {
        // Suppress cascade errors from unresolved types
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return;
        }

        // TS18050: "The value 'X' cannot be used here" for null/undefined operands (STRICT mode only)
        // TSC emits TS18050 for the null/undefined operand AND TS2362/TS2363 for any OTHER invalid operand
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;
        let mut emitted_nullish_error = false;

        // Only emit TS18050 for null/undefined operands when strictNullChecks is enabled
        let should_emit_nullish_error = self.ctx.compiler_options.strict_null_checks;

        // Emit TS18050 for null/undefined operands
        if left_is_nullish && should_emit_nullish_error {
            let value_name = if left_type == TypeId::NULL {
                "null"
            } else {
                "undefined"
            };
            if let Some(loc) = self.get_source_location(left_idx) {
                let message = format_message(
                    diagnostic_messages::VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
                emitted_nullish_error = true;
            }
        }

        if right_is_nullish && should_emit_nullish_error {
            let value_name = if right_type == TypeId::NULL {
                "null"
            } else {
                "undefined"
            };
            if let Some(loc) = self.get_source_location(right_idx) {
                let message = format_message(
                    diagnostic_messages::VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
                emitted_nullish_error = true;
            }
        }

        // If BOTH operands are null/undefined AND we emitted TS18050 for them, we're done (no TS2362/TS2363 needed)
        if left_is_nullish && right_is_nullish && emitted_nullish_error {
            return;
        }

        use crate::solver::BinaryOpEvaluator;

        let evaluator = BinaryOpEvaluator::new(self.ctx.types);

        // TS2469: Check if either operand is a symbol type
        // TS2469 is emitted when an operator cannot be applied to type 'symbol'
        // We check both operands and emit TS2469 for the symbol operand(s)
        let left_is_symbol = evaluator.is_symbol_like(left_type);
        let right_is_symbol = evaluator.is_symbol_like(right_type);

        if left_is_symbol || right_is_symbol {
            // Format type strings first to avoid holding formatter across mutable borrows
            let left_type_str = if left_is_symbol {
                Some(self.ctx.create_type_formatter().format(left_type))
            } else {
                None
            };
            let right_type_str = if right_is_symbol {
                Some(self.ctx.create_type_formatter().format(right_type))
            } else {
                None
            };

            // Emit TS2469 for symbol operands
            if let (Some(loc), Some(type_str)) =
                (self.get_source_location(left_idx), left_type_str.as_deref())
            {
                let message = format_message(
                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    &[op, type_str],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }

            if let (Some(loc), Some(type_str)) = (
                self.get_source_location(right_idx),
                right_type_str.as_deref(),
            ) {
                let message = format_message(
                    diagnostic_messages::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    &[op, type_str],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPE,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }

            // If both are symbols, we're done (no need for TS2365)
            if left_is_symbol && right_is_symbol {
                return;
            }

            // If only one is symbol, continue to check the other operand
            // (but we've already emitted TS2469 for the symbol)
        }

        let mut formatter = self.ctx.create_type_formatter();
        let left_str = formatter.format(left_type);
        let right_str = formatter.format(right_type);

        // Check if this is an arithmetic operator (-, *, /, %, **)
        // Note: + is handled separately - it can be string concatenation or arithmetic
        let is_arithmetic = matches!(op, "-" | "*" | "/" | "%" | "**");

        // Check if operands have valid arithmetic types using BinaryOpEvaluator
        // This properly handles number, bigint, any, and enum types (unions of number literals)
        // Note: evaluator was already created above for symbol checking
        // Skip arithmetic checks for symbol operands (we already emitted TS2469)
        let left_is_valid_arithmetic =
            !left_is_symbol && evaluator.is_arithmetic_operand(left_type);
        let right_is_valid_arithmetic =
            !right_is_symbol && evaluator.is_arithmetic_operand(right_type);

        // For + operator, check if we should emit TS2362/TS2363 or TS2365
        // TS2362/TS2363 are emitted when the operation cannot be string concatenation
        // (i.e., when one operand is clearly not compatible with the other)
        if op == "+" {
            // Check if + could be string concatenation
            let left_could_be_string = left_type == TypeId::STRING
                || left_type == TypeId::ANY
                || self.type_has_string_union_member(left_type);
            let right_could_be_string = right_type == TypeId::STRING
                || right_type == TypeId::ANY
                || self.type_has_string_union_member(right_type);

            // If neither operand can be a string, this must be arithmetic - emit TS2362/TS2363
            let is_arithmetic_context = !left_could_be_string && !right_could_be_string;

            if is_arithmetic_context {
                // Treat as arithmetic operation
                // Skip operands that already got TS18050 (null/undefined with strictNullChecks)
                let mut emitted_specific_error = emitted_nullish_error;
                if !left_is_valid_arithmetic && (!left_is_nullish || !emitted_nullish_error) {
                    if let Some(loc) = self.get_source_location(left_idx) {
                        let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                        emitted_specific_error = true;
                    }
                }
                if !right_is_valid_arithmetic && (!right_is_nullish || !emitted_nullish_error) {
                    if let Some(loc) = self.get_source_location(right_idx) {
                        let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::RIGHT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                        emitted_specific_error = true;
                    }
                }
                // If both operands are valid arithmetic types but the operation still failed
                // (e.g., mixing number and bigint), emit TS2365
                if !emitted_specific_error {
                    if let Some(loc) = self.get_source_location(node_idx) {
                        let message = format!(
                            "Operator '{}' cannot be applied to types '{}' and '{}'.",
                            op, left_str, right_str
                        );
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES,
                            category: DiagnosticCategory::Error,
                            message_text: message,
                            file: self.ctx.file_name.clone(),
                            start: loc.start,
                            length: loc.length(),
                            related_information: Vec::new(),
                        });
                    }
                }
                return;
            }

            // For string concatenation context or ambiguous, emit TS2365
            if let Some(loc) = self.get_source_location(node_idx) {
                let message = format!(
                    "Operator '{}' cannot be applied to types '{}' and '{}'.",
                    op, left_str, right_str
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES,
                    category: DiagnosticCategory::Error,
                    message_text: message,
                    file: self.ctx.file_name.clone(),
                    start: loc.start,
                    length: loc.length(),
                    related_information: Vec::new(),
                });
            }
            return;
        }

        if is_arithmetic {
            // For arithmetic operators, emit specific left/right errors (TS2362, TS2363)
            // Skip operands that already got TS18050 (null/undefined with strictNullChecks)
            let mut emitted_specific_error = emitted_nullish_error;
            if !left_is_valid_arithmetic && (!left_is_nullish || !emitted_nullish_error) {
                if let Some(loc) = self.get_source_location(left_idx) {
                    let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::LEFT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                    emitted_specific_error = true;
                }
            }
            if !right_is_valid_arithmetic && (!right_is_nullish || !emitted_nullish_error) {
                if let Some(loc) = self.get_source_location(right_idx) {
                    let message = "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::RIGHT_HAND_SIDE_OF_ARITHMETIC_MUST_BE_NUMBER,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                    emitted_specific_error = true;
                }
            }
            // If both operands are valid arithmetic types but the operation still failed
            // (e.g., mixing number and bigint), emit TS2365
            if !emitted_specific_error {
                if let Some(loc) = self.get_source_location(node_idx) {
                    let message = format!(
                        "Operator '{}' cannot be applied to types '{}' and '{}'.",
                        op, left_str, right_str
                    );
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                }
            }
        }
    }

    // =========================================================================
    // Variable/Declaration Errors
    // =========================================================================

    /// Report error 2403: Subsequent variable declarations must have the same type.
    pub fn error_subsequent_variable_declaration(
        &mut self,
        name: &str,
        prev_type: TypeId,
        current_type: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress when types are unresolved (ANY/ERROR/UNKNOWN)
        if prev_type == TypeId::ANY || prev_type == TypeId::ERROR || prev_type == TypeId::UNKNOWN {
            return;
        }
        if current_type == TypeId::ANY
            || current_type == TypeId::ERROR
            || current_type == TypeId::UNKNOWN
        {
            return;
        }
        if let Some(loc) = self.get_source_location(idx) {
            let prev_type_str = self.format_type(prev_type);
            let current_type_str = self.format_type(current_type);
            let message = format!(
                "Subsequent variable declarations must have the same type. Variable '{}' must be of type '{}', but here has type '{}'.",
                name, prev_type_str, current_type_str
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_SAME_TYPE,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2454: Variable is used before being assigned.
    pub fn error_variable_used_before_assigned_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::VARIABLE_USED_BEFORE_ASSIGNED, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::VARIABLE_USED_BEFORE_ASSIGNED,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Class-Related Errors
    // =========================================================================

    /// Report error 2715: Abstract property 'X' in class 'C' cannot be accessed in the constructor.
    pub fn error_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Abstract property '{}' in class '{}' cannot be accessed in the constructor.",
                prop_name, class_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ABSTRACT_PROPERTY_IN_CONSTRUCTOR,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Module/Namespace Errors
    // =========================================================================

    /// Report TS2694: Namespace has no exported member.
    pub fn error_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Namespace '{}' has no exported member '{}'.",
                namespace_name, member_name
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: 2694,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Type/Value Mismatch Errors
    // =========================================================================

    /// Report TS2693/TS2585: Symbol only refers to a type, but is used as a value.
    ///
    /// For ES2015+ types (Promise, Map, Set, Symbol, etc.), emits TS2585 with a suggestion
    /// to change the target library. For other types, emits TS2693 without the lib suggestion.
    pub fn error_type_only_value_at(&mut self, name: &str, idx: NodeIndex) {
        use crate::lib_loader;

        if let Some(loc) = self.get_source_location(idx) {
            // Check if this is an ES2015+ type that requires specific lib support
            let is_es2015_type = lib_loader::is_es2015_plus_type(name);

            let (code, message) = if is_es2015_type {
                // TS2585: Type only refers to a type, suggest changing lib
                (
                    diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB,
                    format_message(
                        diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_WITH_LIB,
                        &[name],
                    ),
                )
            } else {
                // TS2693: Generic type-only error
                (
                    diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                    format_message(
                        diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                        &[name],
                    ),
                )
            };

            self.ctx.diagnostics.push(Diagnostic {
                code,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2749: Symbol refers to a value, but is used as a type.
    pub fn error_value_only_type_at(&mut self, name: &str, idx: NodeIndex) {
        // In single-file mode, type/value classification can be incomplete
        // (e.g., class from another file resolves as value-only).
        // Suppress to prevent false positives.
        if !self.ctx.report_unresolved_imports {
            return;
        }
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ONLY_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS18050: The value 'X' cannot be used here.
    /// Emitted when a value (like a variable or literal) is used where it's not permitted.
    pub fn error_value_cannot_be_used_here_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(diagnostic_messages::VALUE_CANNOT_BE_USED_HERE, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::VALUE_CANNOT_BE_USED_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Generic Type Errors
    // =========================================================================

    /// Report TS2314: Generic type 'X' requires N type argument(s).
    pub fn error_generic_type_requires_type_arguments_at(
        &mut self,
        name: &str,
        required_count: usize,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::GENERIC_TYPE_REQUIRES_ARGS,
                &[name, &required_count.to_string()],
            );
            // Use push_diagnostic for deduplication - same type may be resolved multiple times
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENTS,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2344: Type does not satisfy constraint.
    pub fn error_type_constraint_not_satisfied(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if type_arg == TypeId::ERROR
            || constraint == TypeId::ERROR
            || type_arg == TypeId::UNKNOWN
            || constraint == TypeId::UNKNOWN
            || type_arg == TypeId::ANY
            || constraint == TypeId::ANY
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let type_str = self.format_type(type_arg);
            let constraint_str = self.format_type(constraint);
            let message = format_message(
                diagnostic_messages::TYPE_NOT_SATISFY_CONSTRAINT,
                &[&type_str, &constraint_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::TYPE_PARAMETER_CONSTRAINT_NOT_SATISFIED,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2367: This condition will always return 'false'/'true' since the types have no overlap.
    ///
    /// The message depends on the operator:
    /// - For `===` and `==`: "always return 'false'"
    /// - For `!==` and `!=`: "always return 'true'"
    pub fn error_comparison_no_overlap(
        &mut self,
        left_type: TypeId,
        right_type: TypeId,
        is_equality: bool,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if left_type == TypeId::ERROR
            || right_type == TypeId::ERROR
            || left_type == TypeId::ANY
            || right_type == TypeId::ANY
            || left_type == TypeId::UNKNOWN
            || right_type == TypeId::UNKNOWN
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let left_str = self.format_type(left_type);
            let right_str = self.format_type(right_type);
            let result = if is_equality { "false" } else { "true" };
            let message = format_message(
                diagnostic_messages::TYPES_HAVE_NO_OVERLAP,
                &[result, &left_str, &right_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::TYPES_HAVE_NO_OVERLAP,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Diagnostic Utilities
    // =========================================================================

    /// Create a diagnostic collector for batch error reporting.
    pub fn create_diagnostic_collector(&self) -> crate::solver::DiagnosticCollector<'_> {
        crate::solver::DiagnosticCollector::new(self.ctx.types, self.ctx.file_name.as_str())
    }

    /// Merge diagnostics from a collector into the checker's diagnostics.
    pub fn merge_diagnostics(&mut self, collector: &crate::solver::DiagnosticCollector) {
        for diag in collector.to_checker_diagnostics() {
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Check if a type has string as a union member (directly or nested).
    /// Used to determine if + operator could be string concatenation.
    fn type_has_string_union_member(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries::{
            LiteralTypeKind, UnionMembersKind, classify_for_union_members, classify_literal_type,
            is_template_literal_type,
        };

        if type_id == TypeId::STRING {
            return true;
        }

        // Check if this is a string literal
        if let LiteralTypeKind::String(_) = classify_literal_type(self.ctx.types, type_id) {
            return true;
        }

        // Check if this is a template literal type
        if is_template_literal_type(self.ctx.types, type_id) {
            return true;
        }

        // Check if this is a union type containing string
        if let UnionMembersKind::Union(members) =
            classify_for_union_members(self.ctx.types, type_id)
        {
            for member in members {
                if member == TypeId::STRING {
                    return true;
                }
                // Recursively check nested unions
                if self.type_has_string_union_member(member) {
                    return true;
                }
            }
        }

        false
    }
}

// =============================================================================
// Higher-Level Error Reporting (report_* Methods)
// =============================================================================
//
// These methods provide a cleaner, more consistent API for error reporting.
// They use `report_` prefix instead of `error_` for better discoverability.

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

/// Check if a name is a known DOM or ScriptHost global that requires the 'dom' lib.
/// These names are well-known browser/runtime APIs that tsc suggests including
/// the 'dom' lib for when they can't be resolved (TS2584).
pub fn is_known_dom_global(name: &str) -> bool {
    match name {
        // Console
        "console"
        // Window/Document
        | "window" | "document" | "self"
        // DOM elements
        | "HTMLElement" | "HTMLDivElement" | "HTMLSpanElement" | "HTMLInputElement"
        | "HTMLButtonElement" | "HTMLAnchorElement" | "HTMLImageElement"
        | "HTMLCanvasElement" | "HTMLFormElement" | "HTMLSelectElement"
        | "HTMLTextAreaElement" | "HTMLTableElement" | "HTMLMediaElement"
        | "HTMLVideoElement" | "HTMLAudioElement"
        // Core DOM interfaces
        | "Element" | "Node" | "Document" | "Event" | "EventTarget"
        | "NodeList" | "HTMLCollection" | "DOMTokenList"
        // Common Web APIs
        | "XMLHttpRequest" | "fetch" | "Request" | "Response" | "Headers"
        | "URL" | "URLSearchParams"
        | "setTimeout" | "clearTimeout" | "setInterval" | "clearInterval"
        | "requestAnimationFrame" | "cancelAnimationFrame"
        | "alert" | "confirm" | "prompt"
        // Storage
        | "localStorage" | "sessionStorage" | "Storage"
        // Navigator/Location/History
        | "navigator" | "Navigator" | "location" | "Location" | "history" | "History"
        // Events
        | "MouseEvent" | "KeyboardEvent" | "TouchEvent" | "FocusEvent"
        | "CustomEvent" | "MessageEvent" | "ErrorEvent"
        | "addEventListener" | "removeEventListener"
        // Canvas/Media
        | "CanvasRenderingContext2D" | "WebGLRenderingContext"
        | "MediaStream" | "MediaRecorder"
        // Workers/ServiceWorker
        | "Worker" | "ServiceWorker" | "SharedWorker"
        // Misc browser globals
        | "MutationObserver" | "IntersectionObserver" | "ResizeObserver"
        | "Performance" | "performance"
        | "Blob" | "File" | "FileReader" | "FormData"
        | "WebSocket" | "ClipboardEvent" | "DragEvent"
        | "getComputedStyle" | "matchMedia"
        | "DOMException" | "AbortController" | "AbortSignal"
        | "TextEncoder" | "TextDecoder"
        | "crypto" | "Crypto" | "SubtleCrypto"
        | "queueMicrotask" | "structuredClone"
        | "atob" | "btoa" => true,
        _ => false,
    }
}
