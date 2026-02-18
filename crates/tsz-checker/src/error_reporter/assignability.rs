//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::state::{CheckerState, MemberAccessLevel};
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Assignability Errors
    // =========================================================================

    /// Report a type not assignable error (delegates to `diagnose_assignment_failure`).
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx = self.assignment_diagnostic_anchor_idx(idx);

        // Centralized suppression for TS2322 cascades on unresolved escape-hatch types.
        if self.should_suppress_assignability_diagnostic(source, target) {
            if tracing::enabled!(Level::TRACE) {
                trace!(
                    source = source.0,
                    target = target.0,
                    node_idx = idx.0,
                    file = %self.ctx.file_name,
                    "suppressing TS2322 for non-actionable source/target types"
                );
            }
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
                anchor_idx,
            );
            return;
        }

        // Check for private brand mismatch
        if let Some(detail) = self.private_brand_mismatch_error(source, target) {
            let Some(loc) = self.get_node_span(anchor_idx) else {
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

        // Use one solver-boundary analysis path for TS2322 metadata.
        let analysis = self.analyze_assignability_failure(source, target);
        let reason = analysis.failure_reason;

        if tracing::enabled!(Level::TRACE) {
            let source_type = self.format_type(source);
            let target_type = self.format_type(target);
            let reason_ref = reason.as_ref();
            trace!(
                source = %source_type,
                target = %target_type,
                reason = ?reason_ref,
                node_idx = anchor_idx.0,
                file = %self.ctx.file_name,
                "assignability failure diagnostics"
            );
        }

        match reason {
            Some(failure_reason) => {
                let diag =
                    self.render_failure_reason(&failure_reason, source, target, anchor_idx, 0);
                self.ctx.diagnostics.push(diag);
            }
            None => {
                // Fallback to generic message
                self.error_type_not_assignable_generic_at(source, target, anchor_idx);
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
        let anchor_idx = self.assignment_diagnostic_anchor_idx(idx);

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

        if let Some(loc) = self.get_source_location(anchor_idx) {
            // Precedence gate: suppress fallback TS2322 when a more specific
            // diagnostic is already present at the same span.
            if self.has_more_specific_diagnostic_at_span(loc.start, loc.length()) {
                return;
            }

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

    /// Recursively render a `SubtypeFailureReason` into a Diagnostic.
    fn render_failure_reason(
        &mut self,
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

                // Private brand properties are internal implementation details for
                // nominal private member checking. They should never appear in
                // user-facing diagnostics — emit TS2322 instead of TS2741.
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                if prop_name.starts_with("__private_brand") {
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

                // Filter out private brand properties — they are internal implementation
                // details and should never appear in user-facing diagnostics.
                let filtered_names: Vec<_> = property_names
                    .iter()
                    .filter(|name| {
                        !self
                            .ctx
                            .types
                            .resolve_atom_ref(**name)
                            .starts_with("__private_brand")
                    })
                    .collect();

                // If all missing properties were private brands, emit TS2322 instead.
                if filtered_names.is_empty() {
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

                // TS2739: Type 'A' is missing the following properties from type 'B': x, y, z
                // TS2740: Type 'A' is missing the following properties from type 'B': x, y, z, and N more.
                let src_str = self.format_type(*source_type);
                let tgt_str = self.format_type(*target_type);
                let prop_list: Vec<String> = filtered_names
                    .iter()
                    .take(5)
                    .map(|name| self.ctx.types.resolve_atom_ref(**name).to_string())
                    .collect();
                let props_joined = prop_list.join(", ");
                // Use TS2740 when there are 5+ missing properties (tsc behavior)
                if filtered_names.len() > 5 {
                    let more_count = (filtered_names.len() - 5).to_string();
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
                if depth == 0 {
                    let source_str = self.format_type_for_assignability_message(source);
                    let target_str = self.format_type_for_assignability_message(target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let prop_message = format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                        &[&prop_name],
                    );
                    let nested_message = if let Some(nested) = nested_reason {
                        self.render_failure_reason(
                            nested,
                            *source_property_type,
                            *target_property_type,
                            idx,
                            depth + 1,
                        )
                        .message_text
                    } else {
                        let src = self.format_type_for_assignability_message(*source_property_type);
                        let tgt = self.format_type_for_assignability_message(*target_property_type);
                        format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&src, &tgt],
                        )
                    };
                    let message = format!("{base} {prop_message} {nested_message}");
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message = format_message(
                    diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    &[&prop_name],
                );
                let mut diag =
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

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
                    let source_str = self.format_type_for_assignability_message(source);
                    let target_str = self.format_type_for_assignability_message(target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let detail = format_message(
                        diagnostic_messages::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&prop_name, &source_str, &target_str],
                    );
                    let message = format!("{base} {detail}");
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
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

            SubtypeFailureReason::PropertyVisibilityMismatch {
                property_name,
                source_visibility,
                target_visibility,
            } => {
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
                let base = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let detail = match (source_visibility, target_visibility) {
                    (tsz_solver::Visibility::Public, tsz_solver::Visibility::Private) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                            &[&prop_name, &target_str, &source_str],
                        )
                    }
                    (tsz_solver::Visibility::Private, tsz_solver::Visibility::Public) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                            &[&prop_name, &source_str, &target_str],
                        )
                    }
                    (tsz_solver::Visibility::Public, tsz_solver::Visibility::Protected) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &target_str, &source_str],
                        )
                    }
                    (tsz_solver::Visibility::Protected, tsz_solver::Visibility::Public) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &source_str, &target_str],
                        )
                    }
                    _ => format_message(
                        diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                        &[&prop_name],
                    ),
                };
                let message = format!("{base} {detail}");
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::PropertyNominalMismatch { property_name } => {
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
                let base = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let detail = match self.property_visibility_pair(source, target, *property_name) {
                    Some((tsz_solver::Visibility::Public, tsz_solver::Visibility::Private)) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                            &[&prop_name, &target_str, &source_str],
                        )
                    }
                    Some((tsz_solver::Visibility::Private, tsz_solver::Visibility::Public)) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                            &[&prop_name, &source_str, &target_str],
                        )
                    }
                    Some((tsz_solver::Visibility::Public, tsz_solver::Visibility::Protected)) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &target_str, &source_str],
                        )
                    }
                    Some((tsz_solver::Visibility::Protected, tsz_solver::Visibility::Public)) => {
                        format_message(
                            diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                            &[&prop_name, &source_str, &target_str],
                        )
                    }
                    _ => format_message(
                        diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                        &[&prop_name],
                    ),
                };
                let message = format!("{base} {detail}");
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
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
                let message =
                    format!("Return type '{source_str}' is not assignable to '{target_str}'.");
                let mut diag =
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

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
                    "Tuple type has {source_count} elements but target requires {target_count}."
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
                    "Type of element at index {index} is incompatible: '{source_str}' is not assignable to '{target_str}'."
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
                    "Array element type '{source_str}' is not assignable to '{target_str}'."
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
                    "{index_kind} index signature is incompatible: '{source_str}' is not assignable to '{target_str}'."
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
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                )
            }

            SubtypeFailureReason::TypeMismatch {
                source_type: _,
                target_type: _,
            } => {
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);

                if depth == 0
                    && (target_str == "Callable" || target_str == "Applicable")
                    && !tsz_solver::is_primitive_type(self.ctx.types, source)
                {
                    let prop_name = if target_str == "Callable" {
                        "call"
                    } else {
                        "apply"
                    };
                    let message = format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[prop_name, &source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    );
                }

                if depth == 0
                    && let Some(property_name) =
                        self.missing_single_required_property(source, target)
                {
                    let prop_name = self.ctx.types.resolve_atom_ref(property_name);
                    let message = format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&prop_name, &source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    );
                }

                let base = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );

                if depth == 0 {
                    let nonpublic = self.first_nonpublic_constructor_param_property(target);
                    if tracing::enabled!(Level::TRACE) {
                        trace!(
                            target = %target_str,
                            nonpublic = ?nonpublic,
                            "nonpublic constructor param property probe"
                        );
                    }
                    if let Some((member_name, level)) = nonpublic {
                        let detail = match level {
                            MemberAccessLevel::Private => format_message(
                                diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                                &[&member_name, &target_str, &source_str],
                            ),
                            MemberAccessLevel::Protected => format_message(
                                diagnostic_messages::PROPERTY_IS_PROTECTED_IN_TYPE_BUT_PUBLIC_IN_TYPE,
                                &[&member_name, &target_str, &source_str],
                            ),
                        };
                        let message = format!("{base} {detail}");
                        return Diagnostic::error(
                            file_name,
                            start,
                            length,
                            message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                    }
                }

                if depth == 0
                    && let Some(detail) = self.elaborate_type_mismatch_detail(source, target)
                {
                    let message = format!("{base} {detail}");
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                }
            }

            _ => {
                // All remaining variants produce a generic "Type X is not assignable to type Y"
                // with TS2322 code. This covers: PropertyVisibilityMismatch,
                // PropertyNominalMismatch, ParameterTypeMismatch, NoIntersectionMemberMatches,
                // IntrinsicTypeMismatch, LiteralTypeMismatch, ErrorType,
                // RecursionLimitExceeded, ParameterCountMismatch.
                let source_str = self.format_type_for_assignability_message(source);
                let target_str = self.format_type_for_assignability_message(target);
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
}
