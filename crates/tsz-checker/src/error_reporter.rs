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

use crate::state::{CheckerState, MemberAccessLevel};
use crate::types::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

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

    /// Emit a templated diagnostic error at a node.
    ///
    /// Looks up the message template for `code` via `get_message_template`,
    /// formats it with `args`, and emits the error at `node_idx`.
    /// Panics in debug mode if the code has no registered template.
    pub(crate) fn error_at_node_msg(&mut self, node_idx: NodeIndex, code: u32, args: &[&str]) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code)
            .unwrap_or_else(|| panic!("no message template for diagnostic code {code}"));
        let message = format_message(template, args);
        self.error_at_node(node_idx, &message, code);
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
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_type, &target_type],
            );

            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.0,
                loc.1 - loc.0,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
            .with_related(self.ctx.file_name.clone(), loc.0, loc.1 - loc.0, detail);

            self.ctx.diagnostics.push(diag);
            return;
        }

        // Use the solver's explain API to get the detailed reason
        // Use the type environment to resolve TypeQuery and Ref types
        let reason = {
            let env = self.ctx.type_env.borrow();
            let mut checker = tsz_solver::CompatChecker::with_resolver(self.ctx.types, &*env);
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        use tsz_solver::SubtypeFailureReason;

        let (start, length) = self.get_node_span(idx).unwrap_or((0, 0));
        let file_name = self.ctx.file_name.clone();

        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => {
                // TSC emits TS2322 (generic assignability error) instead of TS2741
                // when the source is a primitive type. Primitives can't have "missing properties".
                // Example: `x: number = moduleA` → "Type '...' is not assignable to type 'number'"
                //          NOT "Property 'someClass' is missing in type 'number'..."
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    let src_str = self.format_type(*source_type);
                    let tgt_str = self.format_type(*target_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
                // instead of TS2741.
                // These built-in types inherit properties from Object, and object literals don't
                // explicitly list inherited properties, so TS2741 would be incorrect.
                // Example: `b: Boolean = {}` → TS2322 "Type '{}' is not assignable to type 'Boolean'"
                //          NOT TS2741 "Property 'valueOf' is missing in type '{}'..."
                let tgt_str = self.format_type(*target_type);
                if tgt_str == "Boolean"
                    || tgt_str == "Number"
                    || tgt_str == "String"
                    || tgt_str == "Object"
                {
                    let src_str = self.format_type(*source_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TS2741: Property 'x' is missing in type 'A' but required in type 'B'.
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let src_str = self.format_type(*source_type);
                let message = format_message(
                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop_name, &src_str, &tgt_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                )
            }

            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => {
                // TSC emits TS2322 (generic assignability error) instead of TS2739/TS2740
                // when the source is a primitive type. Primitives can't have "missing properties".
                // Example: `arguments = 10` where arguments is IArguments
                //          → "Type 'number' is not assignable to type '...'"
                //          NOT "Type 'number' is missing properties from type '...'"
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    let src_str = self.format_type(*source_type);
                    let tgt_str = self.format_type(*target_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
                // instead of TS2739/TS2740.
                // These built-in types inherit properties from Object, and object literals don't
                // explicitly list inherited properties, so TS2739 would be incorrect.
                let tgt_str_check = self.format_type(*target_type);
                if tgt_str_check == "Boolean"
                    || tgt_str_check == "Number"
                    || tgt_str_check == "String"
                    || tgt_str_check == "Object"
                {
                    let src_str = self.format_type(*source_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str_check],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TS2739: Type 'A' is missing the following properties from type 'B': x, y, z
                // TS2740: Type 'A' is missing the following properties from type 'B': x, y, z, and N more.
                let src_str = self.format_type(*source_type);
                let tgt_str = self.format_type(*target_type);
                let prop_list: Vec<String> = property_names
                    .iter()
                    .take(5)
                    .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                    .collect();
                let props_joined = prop_list.join(", ");
                // Use TS2740 when there are 5+ missing properties (tsc behavior)
                if property_names.len() > 5 {
                    let more_count = (property_names.len() - 5).to_string();
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        &[&src_str, &tgt_str, &props_joined, &more_count],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                    )
                } else {
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        &[&src_str, &tgt_str, &props_joined],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    )
                }
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
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name.clone(),
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );

                    // Add property incompatibility as related info
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let prop_message = format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                        &[&prop_name],
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: file_name.clone(),
                        start,
                        length,
                        message_text: prop_message,
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
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
                    diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    &[&prop_name],
                );
                let mut diag = Diagnostic::error(
                    file_name.clone(),
                    start,
                    length,
                    message,
                    reason.diagnostic_code(),
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
                // At depth 0, emit TS2322 as the primary error (matching tsc behavior).
                if depth == 0 {
                    let source_str = self.format_type(source);
                    let target_str = self.format_type(target);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let detail = format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&prop_name, &source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name.clone(),
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                    .with_related(file_name, start, length, detail)
                } else {
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let source_str = self.format_type(source);
                    let target_str = self.format_type(target);
                    let message = format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&prop_name, &source_str, &target_str],
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message = format_message(
                    diagnostic_messages::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                    &[&prop_name],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let target_str = self.format_type(*target_type);
                let message = format_message(
                    diagnostic_messages::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE,
                    &[&prop_name, &target_str],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
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
                    reason.diagnostic_code(),
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
                    diagnostic_messages::EXPECTED_ARGUMENTS_BUT_GOT,
                    &[&target_count.to_string(), &source_count.to_string()],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                let message = format!(
                    "Tuple type has {} elements but target requires {}.",
                    source_count, target_count
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
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
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
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
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
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
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let source_str = self.format_type(*source_type);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::NoCommonProperties {
                source_type: _,
                target_type: _,
            } => {
                let source_str = self.format_type(source);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            _ => {
                // All remaining variants produce a generic "Type X is not assignable to type Y"
                // with TS2322 code. This covers: PropertyVisibilityMismatch,
                // PropertyNominalMismatch, ParameterTypeMismatch, NoIntersectionMemberMatches,
                // TypeMismatch, IntrinsicTypeMismatch, LiteralTypeMismatch, ErrorType,
                // RecursionLimitExceeded, ParameterCountMismatch.
                let source_str = self.format_type(source);
                let target_str = self.format_type(target);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
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
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
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
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
    /// If a similar property name is found on the type, emits TS2551 ("Did you mean?")
    /// instead of TS2339.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        use tsz_solver::type_queries;

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
            let suppress_did_you_mean =
                self.has_syntax_parse_errors() || self.class_extends_any_base(type_id);
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);

            // On files with syntax parse errors, TypeScript generally avoids TS2551
            // suggestion diagnostics and sticks with TS2339 to reduce cascades.
            let suggestion = if suppress_did_you_mean {
                None
            } else {
                self.find_similar_property(prop_name, type_id)
            };

            let diag = if let Some(ref suggestion) = suggestion {
                builder.property_not_exist_did_you_mean(
                    prop_name,
                    type_id,
                    suggestion,
                    loc.start,
                    loc.length(),
                )
            } else {
                builder.property_not_exist(prop_name, type_id, loc.start, loc.length())
            };
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
                diagnostic_messages::CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE,
                &[prop_name],
            );
            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE,
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

        self.error_at_node(idx, &message, diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN);
    }

    // =========================================================================
    // Name Resolution Errors
    // =========================================================================

    /// Report a cannot find name error using solver diagnostics with source tracking.
    /// Enhanced to provide suggestions for similar names, import suggestions, and
    /// library change suggestions for ES2015+ types.
    pub fn error_cannot_find_name_at(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        // Keep TS2304 for ambiguous generic assertions such as `<<T>(x: T) => T>f`.
        // These nodes can carry parse-error flags, but TypeScript still reports
        // unresolved `T` alongside TS1005/TS1109.
        let force_emit_for_ambiguous_generic = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| {
                let source = self.ctx.arena.source_files.first()?.text.as_ref();
                let pos = node.pos as usize;
                if pos < 2 {
                    return Some(false);
                }
                let bytes = source.as_bytes();
                Some(
                    bytes.get(pos.saturating_sub(2)) == Some(&b'<')
                        && bytes.get(pos.saturating_sub(1)) == Some(&b'<'),
                )
            })
            .unwrap_or(false);

        let is_primitive_type_keyword = matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        );
        let is_import_equals_module_specifier = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .is_some_and(|parent_node| {
                if parent_node.kind
                    != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    return false;
                }
                self.ctx
                    .arena
                    .get_import_decl(parent_node)
                    .is_some_and(|imp| imp.module_specifier == idx)
            });

        if is_primitive_type_keyword && !is_import_equals_module_specifier {
            self.error_type_only_value_at(name, idx);
            return;
        }

        // In `import x = <expr>` module reference position, unresolved names should
        // report namespace/module diagnostics (TS2503/TS2307), not TS2304.
        let mut cur = idx;
        while let Some(ext) = self.ctx.arena.get_extended(cur) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return;
            }
            cur = parent;
        }

        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(", or empty names) that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1003 "Identifier expected").
        if name.is_empty() {
            return;
        }
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

        // In parse-recovery inside class bodies, contextual modifier keywords
        // can appear as pseudo-identifiers (e.g. `static f = 3` in a ctor).
        // Suppress TS2304 for those to avoid cascades after the primary syntax error.
        if self.has_parse_errors()
            && matches!(
                name,
                "static"
                    | "public"
                    | "private"
                    | "protected"
                    | "readonly"
                    | "abstract"
                    | "declare"
                    | "override"
                    | "accessor"
            )
        {
            let mut current = idx;
            let mut guard = 0;
            while !current.is_none() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                let Some(node) = self.ctx.arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        // In parse-error files, identifiers inside class member bodies are often
        // parser-recovery artifacts (e.g. malformed `static` statements in ctors).
        // Suppress TS2304 there to avoid cascades from the primary syntax error.
        if self.has_parse_errors() {
            let mut current = idx;
            let mut guard = 0;
            let mut in_class = false;
            let mut in_class_member_body = false;
            while !current.is_none() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                let Some(node) = self.ctx.arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    in_class = true;
                }
                if node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    in_class_member_body = true;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
            if in_class && in_class_member_body {
                if is_primitive_type_keyword {
                    self.error_type_only_value_at(name, idx);
                }
                return;
            }
        }

        // Skip TS2304 for nodes/ancestors that have parse errors.
        // This prevents cascading "Cannot find name" errors on malformed AST
        // subtrees while still allowing TS2304 for unrelated valid code.
        let mut current = idx;
        let mut walk_guard = 0;
        while !current.is_none() {
            walk_guard += 1;
            if walk_guard > 256 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                let flags = node.flags as u32;
                if !force_emit_for_ambiguous_generic
                    && ((flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                        || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0)
                {
                    return;
                }
            } else {
                break;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        // Also suppress TS2304 for identifiers that appear shortly after a parse error.
        // These identifiers are likely artifacts of error recovery.
        if !force_emit_for_ambiguous_generic && !self.ctx.syntax_parse_error_positions.is_empty() {
            if let Some(node) = self.ctx.arena.get(idx) {
                let ident_pos = node.pos;
                for &err_pos in &self.ctx.syntax_parse_error_positions {
                    if err_pos <= ident_pos && (ident_pos - err_pos) <= 8 {
                        return;
                    }
                }
            }
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

        // Check if this is a known Node.js global → TS2580
        if is_known_node_global(name) {
            self.error_cannot_find_name_install_node_types(name, idx);
            return;
        }

        // Check if this is a known test runner global → TS2582
        if is_known_test_runner_global(name) {
            self.error_cannot_find_name_install_test_types(name, idx);
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
        use tsz_binder::lib_loader;

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
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB,
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
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2580: Cannot find name 'X' - suggest installing @types/node.
    pub fn error_cannot_find_name_install_node_types(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2582: Cannot find name 'X' - suggest installing test runner types.
    pub fn error_cannot_find_name_install_test_types(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
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
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER,
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
    // Property Suggestion Helpers
    // =========================================================================

    /// Find a similar property name on a type for "did you mean?" suggestions (TS2551).
    /// Uses the same algorithm as tsc's `getSpellingSuggestion`.
    fn find_similar_property(&self, prop_name: &str, type_id: TypeId) -> Option<String> {
        let property_names = self.collect_type_property_names(type_id);
        if property_names.is_empty() {
            return None;
        }

        let name_len = prop_name.len();
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = name_len * 4 / 10 + 1;
        let mut best_candidate: Option<String> = None;

        for candidate in &property_names {
            if candidate == prop_name {
                continue;
            }
            let candidate_len = candidate.len();
            let len_diff = name_len.abs_diff(candidate_len);
            if len_diff > maximum_length_difference {
                continue;
            }
            if name_len < 3 && candidate.to_lowercase() != prop_name.to_lowercase() {
                continue;
            }
            if candidate.to_lowercase() == prop_name.to_lowercase() {
                let distance = 1;
                if distance < best_distance {
                    best_distance = distance;
                    best_candidate = Some(candidate.clone());
                }
                continue;
            }
            let distance = Self::levenshtein_distance(prop_name, candidate);
            if distance < best_distance {
                best_distance = distance;
                best_candidate = Some(candidate.clone());
            }
        }

        best_candidate
    }

    /// Returns true when `type_id` is a class instance type whose declaration extends
    /// a base expression currently typed as `any`.
    ///
    /// In that case TypeScript treats unknown member accesses as `any` and does not
    /// surface typo suggestions (TS2551).
    fn class_extends_any_base(&mut self, type_id: TypeId) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::CLASS == 0 {
            return false;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };
        let Some(class_decl) = self.ctx.arena.get_class_at(decl_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &class_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = clause.types.nodes.first() else {
                continue;
            };
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }

    /// Collect all property names from a type, handling objects, callables, unions,
    /// and intersections.
    fn collect_type_property_names(&self, type_id: TypeId) -> Vec<String> {
        let mut names = Vec::new();
        self.collect_type_property_names_inner(type_id, &mut names, 0);

        // Deduplicate
        names.sort();
        names.dedup();
        names
    }

    fn collect_type_property_names_inner(
        &self,
        type_id: TypeId,
        names: &mut Vec<String>,
        depth: usize,
    ) {
        use crate::query_boundaries::diagnostics::{
            PropertyTraversal, classify_property_traversal,
        };

        if depth > 5 {
            return;
        }

        match classify_property_traversal(self.ctx.types, type_id) {
            PropertyTraversal::Object(shape) => {
                for prop in shape.properties.iter() {
                    names.push(self.ctx.types.resolve_atom_ref(prop.name).to_string());
                }
            }
            PropertyTraversal::Callable(shape) => {
                for prop in shape.properties.iter() {
                    names.push(self.ctx.types.resolve_atom_ref(prop.name).to_string());
                }
            }
            PropertyTraversal::Members(members) => {
                for &member in members.iter() {
                    self.collect_type_property_names_inner(member, names, depth + 1);
                }
            }
            _ => {}
        }
    }

    // =========================================================================
    // Identifier Suggestion Helpers
    // =========================================================================

    fn consider_identifier_suggestion(
        name: &str,
        candidate: &str,
        name_len: usize,
        maximum_length_difference: usize,
        best_distance: &mut usize,
        best_candidate: &mut Option<String>,
    ) {
        if candidate == name {
            return;
        }
        let candidate_len = candidate.len();

        // tsc: skip candidates whose length is too different
        let len_diff = name_len.abs_diff(candidate_len);
        if len_diff > maximum_length_difference {
            return;
        }

        // tsc: for short names (<3), only suggest if differs by case
        if name_len < 3 && candidate.to_lowercase() != name.to_lowercase() {
            return;
        }

        // Case-insensitive exact match is distance 1
        if candidate.to_lowercase() == name.to_lowercase() {
            let distance = 1;
            if distance < *best_distance {
                *best_distance = distance;
                *best_candidate = Some(candidate.to_string());
            }
            return;
        }

        let distance = Self::levenshtein_distance(name, candidate);
        if distance < *best_distance {
            *best_distance = distance;
            *best_candidate = Some(candidate.to_string());
        }
    }

    /// Find the best spelling suggestion for a name, matching tsc's `getSpellingSuggestion`.
    /// Returns `Some(best_name)` if a close-enough match is found.
    pub(crate) fn find_similar_identifiers(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<Vec<String>> {
        let visible_names = self.ctx.binder.collect_visible_symbol_names_filtered(
            self.ctx.arena,
            idx,
            tsz_binder::symbol_flags::VALUE,
        );

        let name_len = name.len();
        // tsc: bestDistance = (name.length + 2) * 0.34 rounded down, min 2
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        // tsc: initial bestDistance = floor(name.length * 0.4) + 1
        let mut best_distance = name_len * 4 / 10 + 1;
        let mut best_candidate: Option<String> = None;

        for candidate in visible_names {
            Self::consider_identifier_suggestion(
                name,
                &candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        // Fall back to lib globals for spelling suggestions when local scope
        // candidates don't produce a close enough match.
        if best_candidate.is_none() {
            let lib_binders = self.get_lib_binders();
            for lib_binder in &lib_binders {
                for (candidate, sym_id) in lib_binder.file_locals.iter() {
                    if lib_binder
                        .get_symbol(*sym_id).is_none_or(|sym| sym.flags & tsz_binder::symbol_flags::VALUE == 0)
                    {
                        continue;
                    }
                    Self::consider_identifier_suggestion(
                        name,
                        candidate,
                        name_len,
                        maximum_length_difference,
                        &mut best_distance,
                        &mut best_candidate,
                    );
                }
            }
        }

        best_candidate.map(|c| vec![c])
    }

    /// Calculate Levenshtein distance between two strings (case-sensitive, matching tsc).
    fn levenshtein_distance(a: &str, b: &str) -> usize {
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

        let mut prev = vec![0usize; b_len + 1];
        let mut curr = vec![0usize; b_len + 1];

        for j in 0..=b_len {
            prev[j] = j;
        }

        for i in 1..=a_len {
            curr[0] = i;
            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[b_len]
    }

    // =========================================================================
    // Function Call Errors
    // =========================================================================

    /// Try to elaborate an argument type mismatch for object/array literal arguments.
    ///
    /// When an object literal argument has a property whose value type doesn't match
    /// the expected property type, tsc reports TS2322 on the specific property name
    /// rather than TS2345 on the whole argument. Similarly for array literals, tsc
    /// reports TS2322 on each element that doesn't match the expected element type.
    ///
    /// Returns `true` if elaboration produced at least one property-level error (TS2322),
    /// meaning the caller should NOT emit TS2345 on the whole argument.
    pub fn try_elaborate_object_literal_arg_error(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        match arg_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.try_elaborate_object_literal_properties(arg_idx, param_type)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.try_elaborate_array_literal_elements(arg_idx, param_type)
            }
            _ => false,
        }
    }

    /// Elaborate object literal property type mismatches with TS2322.
    fn try_elaborate_object_literal_properties(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) => node,
            None => return false,
        };

        let obj = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(obj) => obj.clone(),
            None => return false,
        };

        let mut elaborated = false;

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Only elaborate regular property assignments and shorthand properties
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_property_assignment(elem_node) {
                        Some(prop) => (prop.name, prop.initializer),
                        None => continue,
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    match self.ctx.arena.get_shorthand_property(elem_node) {
                        Some(prop) => (prop.name, prop.name),
                        None => continue,
                    }
                }
                _ => continue,
            };

            // Get the property name string
            let prop_name = match self.ctx.arena.get_identifier_at(prop_name_idx) {
                Some(ident) => ident.escaped_text.clone(),
                None => continue,
            };

            // Look up the expected property type in the target parameter type
            let target_prop_type = match self
                .resolve_property_access_with_env(param_type, &prop_name)
            {
                tsz_solver::operations_property::PropertyAccessResult::Success {
                    type_id, ..
                } => type_id,
                _ => continue,
            };

            // Get the type of the property value in the object literal
            let source_prop_type = self.get_type_of_node(prop_value_idx);

            // Skip if types are unresolved
            if source_prop_type == TypeId::ERROR
                || source_prop_type == TypeId::ANY
                || target_prop_type == TypeId::ERROR
                || target_prop_type == TypeId::ANY
            {
                continue;
            }

            // Check if the property value type is assignable to the target property type
            if !self.is_assignable_to(source_prop_type, target_prop_type) {
                // Emit TS2322 on the property name node
                self.error_type_not_assignable_at(
                    source_prop_type,
                    target_prop_type,
                    prop_name_idx,
                );
                elaborated = true;
            }
        }

        elaborated
    }

    /// Elaborate array literal element type mismatches with TS2322.
    fn try_elaborate_array_literal_elements(
        &mut self,
        arg_idx: NodeIndex,
        param_type: TypeId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let arg_node = match self.ctx.arena.get(arg_idx) {
            Some(node) if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => node,
            _ => return false,
        };

        // Get the expected element type from the parameter array type
        let target_element_type =
            match tsz_solver::visitor::array_element_type(self.ctx.types, param_type) {
                Some(elem_type) => elem_type,
                None => return false,
            };

        let arr = match self.ctx.arena.get_literal_expr(arg_node) {
            Some(arr) => arr.clone(),
            None => return false,
        };

        let mut elaborated = false;

        for &elem_idx in &arr.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Skip spread elements
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                continue;
            }

            let elem_type = self.get_type_of_node(elem_idx);

            // Skip if types are unresolved
            if elem_type == TypeId::ERROR
                || elem_type == TypeId::ANY
                || target_element_type == TypeId::ERROR
                || target_element_type == TypeId::ANY
            {
                continue;
            }

            if !self.is_assignable_to(elem_type, target_element_type) {
                self.error_type_not_assignable_at(elem_type, target_element_type, elem_idx);
                elaborated = true;
            }
        }

        elaborated
    }

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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
                code: diagnostic_codes::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
                category: DiagnosticCategory::Error,
                message_text: diagnostic_messages::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER.to_string(),
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
                code: diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT,
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
        failures: &[tsz_solver::PendingDiagnostic],
    ) {
        use tsz_solver::PendingDiagnostic;

        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let mut formatter = self.ctx.create_type_formatter();
        let mut related = Vec::new();
        let span =
            tsz_solver::SourceSpan::new(self.ctx.file_name.as_str(), loc.start, loc.length());

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
            code: diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
            category: DiagnosticCategory::Error,
            message_text: diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL.to_string(),
            file: self.ctx.file_name.clone(),
            start: loc.start,
            length: loc.length(),
            related_information: related,
        });
    }

    /// Report TS2693: type parameter used as value
    pub fn error_type_parameter_used_as_value(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            use tsz_common::diagnostics::diagnostic_codes;

            let message = format!(
                "'{}' only refers to a type, but is being used as a value here.",
                name
            );

            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report a "type is not callable" error using solver diagnostics with source tracking.
    pub fn error_not_callable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
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
            use crate::types::diagnostics::diagnostic_codes;
            self.ctx.diagnostics.push(
                crate::types::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?".to_string(),
                    diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE,
                ),
            );
        }
    }

    /// Report TS2348: "Value of type '{0}' is not callable. Did you mean to include 'new'?"
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

        let message =
            diagnostic_messages::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                .replace("{0}", &type_str);

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW,
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

        let message = format_message(
            diagnostic_messages::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
            &[&name],
        );

        self.ctx.diagnostics.push(Diagnostic {
            code: diagnostic_codes::IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_ITS_OWN_BASE_EXPRESSION,
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

        // Track nullish operands for proper error reporting
        // NOTE: TSC emits TS2365 for '+' operator with null/undefined, but TS18050 for other arithmetic operators
        let left_is_nullish = left_type == TypeId::NULL || left_type == TypeId::UNDEFINED;
        let right_is_nullish = right_type == TypeId::NULL || right_type == TypeId::UNDEFINED;
        let mut emitted_nullish_error = false;

        // TS18050 is only emitted for strictly-arithmetic and bitwise operators with null/undefined operands.
        // The `+` operator is NOT included: tsc emits TS2365 for `null + null`, not TS18050,
        // because `+` can be string concatenation and has its own type-checking path.
        // Relational operators (<, >, <=, >=) also emit TS18050, but only for literal null/undefined.
        // For now, we only handle arithmetic/bitwise since our evaluator doesn't distinguish
        // literal values from variables typed as null/undefined.
        let should_emit_nullish_error = matches!(
            op,
            "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>"
        );

        // Emit TS18050 for null/undefined operands in arithmetic operations (except +)
        if left_is_nullish && should_emit_nullish_error {
            let value_name = if left_type == TypeId::NULL {
                "null"
            } else {
                "undefined"
            };
            if let Some(loc) = self.get_source_location(left_idx) {
                let message = format_message(
                    diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
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
                    diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE,
                    &[value_name],
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
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

        // If BOTH operands are null/undefined AND we emitted TS18050 for them, we're done
        if left_is_nullish && right_is_nullish && emitted_nullish_error {
            return;
        }

        use tsz_solver::BinaryOpEvaluator;

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

        // Check if this is an arithmetic or bitwise operator
        // These operators require integer operands and emit TS2362/TS2363
        // Note: + is handled separately - it can be string concatenation or arithmetic
        let is_arithmetic = matches!(op, "-" | "*" | "/" | "%" | "**");
        let is_bitwise = matches!(op, "&" | "|" | "^" | "<<" | ">>" | ">>>");
        let requires_numeric_operands = is_arithmetic || is_bitwise;

        // Evaluate types to resolve unevaluated conditional/mapped types before checking.
        // e.g., DeepPartial<number> | number → number
        let eval_left = self.evaluate_type_for_binary_ops(left_type);
        let eval_right = self.evaluate_type_for_binary_ops(right_type);

        // Check if operands have valid arithmetic types using BinaryOpEvaluator
        // This properly handles number, bigint, any, and enum types (unions of number literals)
        // Note: evaluator was already created above for symbol checking
        // Skip arithmetic checks for symbol operands (we already emitted TS2469)
        let left_is_valid_arithmetic =
            !left_is_symbol && evaluator.is_arithmetic_operand(eval_left);
        let right_is_valid_arithmetic =
            !right_is_symbol && evaluator.is_arithmetic_operand(eval_right);

        // For + operator, TSC always emits TS2365 ("Operator '+' cannot be applied to types"),
        // never TS2362/TS2363. This is because + can be either string concatenation or arithmetic,
        // so TSC uses the general error regardless of the operand types.
        if op == "+" {
            if let Some(loc) = self.get_source_location(node_idx) {
                let message = format!(
                    "Operator '{}' cannot be applied to types '{}' and '{}'.",
                    op, left_str, right_str
                );
                self.ctx.diagnostics.push(Diagnostic {
                    code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
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

        if requires_numeric_operands {
            // For arithmetic and bitwise operators, emit specific left/right errors (TS2362, TS2363)
            // Skip operands that already got TS18050 (null/undefined with strictNullChecks)
            let mut emitted_specific_error = emitted_nullish_error;
            if !left_is_valid_arithmetic && (!left_is_nullish || !emitted_nullish_error) {
                if let Some(loc) = self.get_source_location(left_idx) {
                    let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
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
                        code: diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
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
                        code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
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

        // Handle bitwise operators: &, |, ^, <<, >>, >>>
        let is_bitwise = matches!(op, "&" | "|" | "^" | "<<" | ">>" | ">>>");
        if is_bitwise {
            // TS2447: For &, |, ^ with both boolean operands, emit special error
            let left_is_boolean = evaluator.is_boolean_like(left_type);
            let right_is_boolean = evaluator.is_boolean_like(right_type);
            let is_boolean_bitwise =
                matches!(op, "&" | "|" | "^") && left_is_boolean && right_is_boolean;

            if is_boolean_bitwise {
                let suggestion = match op {
                    "&" => "&&",
                    "|" => "||",
                    "^" => "!==",
                    _ => unreachable!(),
                };
                if let Some(loc) = self.get_source_location(node_idx) {
                    let message = format!(
                        "The '{}' operator is not allowed for boolean types. Consider using '{}' instead.",
                        op, suggestion
                    );
                    self.ctx.diagnostics.push(Diagnostic {
                        code: diagnostic_codes::THE_OPERATOR_IS_NOT_ALLOWED_FOR_BOOLEAN_TYPES_CONSIDER_USING_INSTEAD,
                        category: DiagnosticCategory::Error,
                        message_text: message,
                        file: self.ctx.file_name.clone(),
                        start: loc.start,
                        length: loc.length(),
                        related_information: Vec::new(),
                    });
                }
            } else {
                // For other invalid bitwise operands, emit TS2362/TS2363
                let mut emitted_specific_error = emitted_nullish_error;
                if !left_is_valid_arithmetic && (!left_is_nullish || !emitted_nullish_error) {
                    if let Some(loc) = self.get_source_location(left_idx) {
                        let message = "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string();
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
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
                            code: diagnostic_codes::THE_RIGHT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT,
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
                if !emitted_specific_error {
                    if let Some(loc) = self.get_source_location(node_idx) {
                        let message = format!(
                            "Operator '{}' cannot be applied to types '{}' and '{}'.",
                            op, left_str, right_str
                        );
                        self.ctx.diagnostics.push(Diagnostic {
                            code: diagnostic_codes::OPERATOR_CANNOT_BE_APPLIED_TO_TYPES_AND,
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
                code: diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP,
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
            let message = format_message(
                diagnostic_messages::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
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
                code: diagnostic_codes::ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR,
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
        use tsz_binder::lib_loader;

        // Don't emit TS2693 for identifiers used as import equals module references.
        // `import r = undefined` already gets TS2503 from check_namespace_import.
        if self.ctx.arena.get_extended(idx).is_some_and(|ext| {
            self.ctx.arena.get(ext.parent).is_some_and(|p| {
                p.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            })
        }) {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Check if this is an ES2015+ type that requires specific lib support
            let is_es2015_type = lib_loader::is_es2015_plus_type(name);

            let (code, message) = if is_es2015_type {
                // TS2585: Type only refers to a type, suggest changing target library
                (
                    diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
                    format_message(
                        diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
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
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2709: Cannot use namespace '{0}' as a type.
    pub fn error_namespace_used_as_type_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_TYPE, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_TYPE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2708: Cannot use namespace '{0}' as a value.
    pub fn error_namespace_used_as_value_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_VALUE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE,
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
            let message =
                format_message(diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
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
                diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                &[name, &required_count.to_string()],
            );
            // Use push_diagnostic for deduplication - same type may be resolved multiple times
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
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

        // Also suppress when either side CONTAINS error types (e.g., { new(): error }).
        // This happens when a forward-referenced class hasn't been fully resolved yet.
        if tsz_solver::type_queries::contains_error_type_db(self.ctx.types, type_arg)
            || tsz_solver::type_queries::contains_error_type_db(self.ctx.types, constraint)
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Deduplicate: get_type_from_type_node may re-resolve type references when
            // type_parameter_scope changes, causing validate_type_reference_type_arguments
            // to be called multiple times for the same node.
            let key = (
                loc.start,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            );
            if self.ctx.emitted_diagnostics.contains(&key) {
                return;
            }
            self.ctx.emitted_diagnostics.insert(key);

            let type_str = self.format_type(type_arg);
            let constraint_str = self.format_type(constraint);
            let message = format_message(
                diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[&type_str, &constraint_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
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
                diagnostic_messages::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA,
                &[result, &left_str, &right_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::THIS_COMPARISON_APPEARS_TO_BE_UNINTENTIONAL_BECAUSE_THE_TYPES_AND_HAVE_NO_OVERLA,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2352: Conversion of type 'X' to type 'Y' may be a mistake because neither type
    /// sufficiently overlaps with the other. If this was intentional, convert the expression to 'unknown' first.
    pub fn error_type_assertion_no_overlap(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let source_str = self.format_type(source_type);
            let target_str = self.format_type(target_type);
            let message = format_message(
                diagnostic_messages::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
                &[&source_str, &target_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV,
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
    pub fn create_diagnostic_collector(&self) -> tsz_solver::DiagnosticCollector<'_> {
        tsz_solver::DiagnosticCollector::new(self.ctx.types, self.ctx.file_name.as_str())
    }

    /// Merge diagnostics from a collector into the checker's diagnostics.
    pub fn merge_diagnostics(&mut self, collector: &tsz_solver::DiagnosticCollector) {
        for diag in collector.to_checker_diagnostics() {
            self.ctx.diagnostics.push(diag);
        }
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
                diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[&type_str, &constraint_str],
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
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
        use tsz_binder::lib_loader;

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
        failures: &[tsz_solver::PendingDiagnostic],
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
                diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                &[prop_name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                file: self.ctx.file_name.clone(),
                start,
                length,
                message_text: message,
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
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

/// Check if a name is a known Node.js global that requires @types/node (TS2580).
pub fn is_known_node_global(name: &str) -> bool {
    matches!(
        name,
        "require" | "exports" | "module" | "process" | "Buffer" | "__filename" | "__dirname"
    )
}

/// Check if a name is a known test runner global that requires @types/jest or @types/mocha (TS2582).
pub fn is_known_test_runner_global(name: &str) -> bool {
    matches!(name, "describe" | "suite" | "it" | "test")
}
