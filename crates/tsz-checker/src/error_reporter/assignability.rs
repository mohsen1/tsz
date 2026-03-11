//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::state::{CheckerState, MemberAccessLevel};
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Returns true if the formatted type name matches a built-in wrapper type
/// (Boolean, Number, String, Object). These types inherit properties from Object
/// and missing-property diagnostics should be suppressed in favor of TS2322.
fn is_builtin_wrapper_name(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String" | "Object")
}

/// Returns true if the property name is a standard Object.prototype method.
/// These are implicitly available on all interfaces/objects through the Object
/// prototype chain. When such a property appears as "missing" in a subtype check,
/// it typically means the source type inherits it implicitly but its `ObjectShape`
/// doesn't include it. In this case, the mismatch is a type compatibility issue
/// (TS2322), not a missing property issue (TS2741).
pub(super) fn is_object_prototype_method(name: &str) -> bool {
    matches!(
        name,
        "valueOf"
            | "toString"
            | "toLocaleString"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "constructor"
    )
}

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Assignability Errors
    // =========================================================================

    /// Report a type not assignable error (delegates to `diagnose_assignment_failure`).
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx = self.assignment_diagnostic_anchor_idx(idx);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report a type not assignable error at an exact AST node anchor.
    pub fn error_type_not_assignable_at_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }
    pub fn error_type_does_not_satisfy_the_expected_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return;
        }

        let reason = self
            .analyze_assignability_failure(source, target)
            .failure_reason;

        // For TS1360, point the diagnostic at the `satisfies` keyword position
        // when available, rather than walking up to the enclosing statement.
        let anchor_idx = if keyword_pos.is_some() {
            idx
        } else {
            self.assignment_diagnostic_anchor_idx(idx)
        };

        let mut base_diag = match reason {
            Some(reason) => self.render_failure_reason(&reason, source, target, anchor_idx, 0),
            None => {
                let Some(loc) = self.get_source_location(anchor_idx) else {
                    return;
                };
                let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                    self.ctx.file_name.as_str(),
                )
                .with_def_store(&self.ctx.definition_store)
                .with_namespace_module_names(&self.ctx.namespace_module_names);
                let diag = builder.type_not_assignable(source, target, loc.start, loc.length());
                diag.to_checker_diagnostic(&self.ctx.file_name)
            }
        };

        // Mutate the top-level diagnostic to be TS1360
        let src_str = self.format_type_for_assignability_message(source);
        let tgt_str = self.format_type_for_assignability_message(target);
        use tsz_common::diagnostics::data::diagnostic_codes;
        use tsz_common::diagnostics::data::diagnostic_messages;
        use tsz_common::diagnostics::format_message;

        let msg = format_message(
            diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE,
            &[&src_str, &tgt_str],
        );

        if base_diag.code != diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE {
            let mut new_related = vec![];

            new_related.push(tsz_common::diagnostics::DiagnosticRelatedInformation {
                category: tsz_common::diagnostics::DiagnosticCategory::Error,
                code: base_diag.code,
                file: base_diag.file.clone(),
                start: base_diag.start,
                length: base_diag.length,
                message_text: base_diag.message_text.clone(),
            });

            new_related.extend(base_diag.related_information);

            base_diag.code = diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_EXPECTED_TYPE;
            base_diag.message_text = msg;
            base_diag.related_information = new_related;
        }

        // Override the diagnostic start position to the `satisfies` keyword
        // when available. tsc points TS1360 at the keyword, not the expression.
        if let Some(kw_pos) = keyword_pos {
            base_diag.start = kw_pos;
            // "satisfies" is 9 characters long
            base_diag.length = 9;
        }

        self.ctx.diagnostics.push(base_diag);
    }

    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx = self.assignment_diagnostic_anchor_idx(idx);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Internal helper that reports a detailed assignability failure using an
    /// already-resolved diagnostic anchor.
    fn diagnose_assignment_failure_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        // Centralized suppression for TS2322 cascades on unresolved escape-hatch types.
        if self.should_suppress_assignability_diagnostic(source, target) {
            if tracing::enabled!(Level::TRACE) {
                trace!(
                    source = source.0,
                    target = target.0,
                    node_idx = anchor_idx.0,
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

            let source_type = self.format_type_diagnostic(source);
            let target_type = self.format_type_diagnostic(target);
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
            let source_type = self.format_type_diagnostic(source);
            let target_type = self.format_type_diagnostic(target);
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
                // Skip ExcessProperty diagnostics here — they are handled by
                // check_object_literal_excess_properties which also checks for
                // spelling suggestions (TS2561). Emitting here would cause
                // duplicate diagnostics: TS2353 from the solver reason + TS2561
                // from the explicit checker.
                if matches!(
                    failure_reason,
                    tsz_solver::SubtypeFailureReason::ExcessProperty { .. }
                ) {
                    return;
                }
                let diag =
                    self.render_failure_reason(&failure_reason, source, target, anchor_idx, 0);
                self.ctx.diagnostics.push(diag);
            }
            None => {
                // Fallback to generic message
                self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
            }
        }
    }

    fn format_top_level_assignability_message_types(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (String, String) {
        (
            self.format_assignability_type_for_message(source, target),
            self.format_assignability_type_for_message(target, source),
        )
    }

    /// Internal generic error reporting for type assignability failures.
    pub(crate) fn error_type_not_assignable_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        let anchor_idx = self.assignment_diagnostic_anchor_idx(idx);
        self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
    }

    fn error_type_not_assignable_generic_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
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

            if let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
                && missing_props.len() > 1
            {
                let src_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
                let tgt_str = self.format_assignability_type_for_message(target, source);
                let prop_list: Vec<String> = missing_props
                    .iter()
                    .take(4)
                    .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                    .collect();
                let props_joined = prop_list.join(", ");
                let (message, code) = if missing_props.len() > 4 {
                    let more_count = (missing_props.len() - 4).to_string();
                    (
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                            &[&src_str, &tgt_str, &props_joined, &more_count],
                        ),
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                    )
                } else {
                    (
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                            &[&src_str, &tgt_str, &props_joined],
                        ),
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    )
                };
                self.ctx.diagnostics.push(Diagnostic::error(
                    self.ctx.file_name.clone(),
                    loc.start,
                    loc.length(),
                    message,
                    code,
                ));
                return;
            }

            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ));
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
                // Note: `object` (TypeId::OBJECT) is explicitly non-primitive — it represents
                // all non-primitive values and behaves like `{}` structurally, so missing
                // properties are meaningful and should produce TS2741.
                if *source_type != tsz_solver::TypeId::OBJECT
                    && tsz_solver::is_primitive_type(self.ctx.types, *source_type)
                {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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
                // Check both the solver's target_type (inner shape) and the original target
                // (may be the named interface when solver resolves to anonymous shape).
                let tgt_str = self.format_type_diagnostic(*target_type);
                let original_tgt_str = self.format_type_diagnostic(target);
                if is_builtin_wrapper_name(&tgt_str) || is_builtin_wrapper_name(&original_tgt_str) {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let display_tgt = if is_builtin_wrapper_name(&original_tgt_str) {
                        &original_tgt_str
                    } else {
                        &tgt_str
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, display_tgt],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TSC emits TS2322 instead of TS2741 when the target type is an
                // intersection type. For example: `anb: A & B = a` where `a: A`
                // → TS2322 "Type 'A' is not assignable to type 'A & B'"
                // not TS2741 "Property 'b' is missing in type 'A'..."
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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
                    let src_str = self.format_type_for_assignability_message(*source_type);
                    let tgt_str_with_args =
                        self.format_type_for_assignability_message(*target_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str_with_args],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TSC emits TS2322 instead of TS2741 when the target is an
                // intersection type. Intersection types are not concrete object
                // types, so "Property X is missing" is misleading — use the
                // generic "not assignable" message instead.
                // Example: `anb: A & B = a` → TS2322 "Type 'A' is not assignable
                //          to type 'A & B'", NOT TS2741 "Property 'b' is missing..."
                // Check both the reason's target_type (may be flattened by solver)
                // and the original target (preserves intersection structure).
                if tsz_solver::is_intersection_type(self.ctx.types, *target_type)
                    || tsz_solver::is_intersection_type(self.ctx.types, target)
                {
                    let src_str = self.format_type_diagnostic(source);
                    let tgt_str_full = self.format_type_diagnostic(target);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str_full],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TSC emits TS2322 when the target's declared type annotation is an
                // intersection type. Our normalizer eagerly merges `{a:string} & {b:string}`
                // into `{a:string, b:string}`, losing the intersection identity. Check the
                // AST to detect whether the assignment target was declared with an
                // intersection type annotation.
                if self.anchor_target_has_intersection_annotation(idx) {
                    let src_str = self.format_type_diagnostic(source);
                    let tgt_str_full = self.format_type_diagnostic(target);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str_full],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // When the missing property is an Object.prototype method (valueOf,
                // toString, etc.), the source type likely has it through implicit
                // Object inheritance — its ObjectShape just doesn't include it.
                // The real failure is type incompatibility (different return types),
                // not a missing property. Emit TS2322 instead of TS2741.
                if is_object_prototype_method(&prop_name) {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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
                // In tsc, `object` type uses its apparent type `{}` in property-missing
                // diagnostics (getApparentType(object) = {}).
                // Use format_type_pair for import-qualification when the source and target
                // types have the same name but come from different modules.
                let (src_str, tgt_str_qualified) = if *source_type == TypeId::OBJECT {
                    ("{}".to_string(), tgt_str)
                } else {
                    self.format_type_pair_diagnostic(*source_type, target)
                };
                let message = format_message(
                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop_name, &src_str, &tgt_str_qualified],
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
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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

                // TS2696: When the source is the `Object` wrapper type, TSC emits a
                // special diagnostic instead of TS2739/TS2740/TS2322.
                // "The 'Object' type is assignable to very few other types."
                {
                    let src_str = self.format_type_diagnostic(*source_type);
                    if src_str == "Object" {
                        return Diagnostic::error(
                            file_name,
                            start,
                            length,
                            diagnostic_messages::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN
                                .to_string(),
                            diagnostic_codes::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN,
                        );
                    }
                }

                // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
                // instead of TS2739/TS2740.
                // These built-in types inherit properties from Object, and object literals don't
                // explicitly list inherited properties, so TS2739 would be incorrect.
                // Check both the solver's target_type and the original target.
                let tgt_str_check = self.format_type_diagnostic(*target_type);
                let original_tgt_check = self.format_type_diagnostic(target);
                if is_builtin_wrapper_name(&tgt_str_check)
                    || is_builtin_wrapper_name(&original_tgt_check)
                {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let display_tgt = if is_builtin_wrapper_name(&original_tgt_check) {
                        &original_tgt_check
                    } else {
                        &tgt_str_check
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, display_tgt],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }

                // TSC emits TS2322 instead of TS2739/TS2740 when the target is an
                // intersection type (same reasoning as the TS2741 guard above).
                if tsz_solver::is_intersection_type(self.ctx.types, *target_type)
                    || tsz_solver::is_intersection_type(self.ctx.types, target)
                {
                    let src_str = self.format_type_diagnostic(source);
                    let tgt_str = self.format_type_diagnostic(target);
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

                // Filter out private brand properties and Object.prototype methods.
                // Private brands are internal implementation details.
                // Object.prototype methods (toString, valueOf, etc.) exist on every
                // object via prototype inheritance — tsc's getPropertiesOfType includes
                // them on the source's apparent type, so they're never "missing".
                let filtered_names: Vec<_> = property_names
                    .iter()
                    .filter(|name| {
                        let s = self.ctx.types.resolve_atom_ref(**name);
                        !s.starts_with("__private_brand") && !is_object_prototype_method(&s)
                    })
                    .collect();

                // If all missing properties are numeric indices, emit TS2322.
                // TSC often emits TS2322 instead of TS2739 when assigning arrays/tuples to tuple-like interfaces.
                let all_numeric = !filtered_names.is_empty()
                    && filtered_names.iter().all(|name| {
                        let s = self.ctx.types.resolve_atom_ref(**name);
                        s.parse::<usize>().is_ok()
                    });

                if all_numeric {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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

                // Note: Object.prototype methods are already filtered from filtered_names above.
                // If ALL missing properties were Object.prototype methods, filtered_names
                // will be empty and the empty check below emits TS2322 (matching tsc).

                // If all missing properties were private brands, emit TS2322 instead.
                if filtered_names.is_empty() {
                    let src_str = self.format_type_diagnostic(*source_type);
                    let tgt_str = self.format_type_diagnostic(*target_type);
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
                let display_source = if self
                    .missing_required_properties_from_index_signature_source(
                        *source_type,
                        *target_type,
                    )
                    .is_some()
                {
                    self.evaluate_type_for_assignability(*source_type)
                } else {
                    *source_type
                };
                let src_str = self.format_type_diagnostic(display_source);
                let tgt_str = self.format_type_diagnostic(*target_type);
                let prop_list: Vec<String> = filtered_names
                    .iter()
                    .take(4)
                    .map(|name| self.ctx.types.resolve_atom_ref(**name).to_string())
                    .collect();
                let props_joined = prop_list.join(", ");
                // Use TS2740 when there are 5+ missing properties (tsc shows first 4 + "and N more")
                if filtered_names.len() > 4 {
                    let more_count = (filtered_names.len() - 4).to_string();
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
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    // TODO: tsc emits property type mismatch elaboration as related information
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
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
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    // TODO: tsc emits property optional/required elaboration as related information
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                    let source_str = self.format_type_diagnostic(source);
                    let target_str = self.format_type_diagnostic(target);
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

            SubtypeFailureReason::PropertyVisibilityMismatch { .. } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types(source, target);
                let base = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                // TODO: tsc emits visibility elaboration as related information
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::PropertyNominalMismatch { .. } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types(source, target);
                let base = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                // TODO: tsc emits nominal mismatch elaboration as related information
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let target_str = self.format_type_diagnostic(*target_type);
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
                if depth == 0 {
                    // At depth 0, tsc emits the top-level "Type X is not assignable to type Y"
                    // as the primary diagnostic and uses "Return type..." as elaboration.
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    let mut diag = Diagnostic::error(
                        file_name.clone(),
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );

                    // Add "Return type..." as elaboration
                    let ret_source_str = self.format_type_diagnostic(*source_return);
                    let ret_target_str = self.format_type_diagnostic(*target_return);
                    let ret_msg = format!(
                        "Return type '{ret_source_str}' is not assignable to '{ret_target_str}'."
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: file_name,
                        start,
                        length,
                        message_text: ret_msg,
                        category: DiagnosticCategory::Message,
                        code: reason.diagnostic_code(),
                    });

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
                } else {
                    let source_str = self.format_type_diagnostic(*source_return);
                    let target_str = self.format_type_diagnostic(*target_return);
                    let message =
                        format!("Return type '{source_str}' is not assignable to '{target_str}'.");
                    let mut diag = Diagnostic::error(
                        file_name,
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
            }

            SubtypeFailureReason::TooManyParameters { .. } => {
                // In assignability context, too-many-parameters is a type mismatch (TS2322),
                // not an argument count error (TS2554). TS2554 is only for call expressions.
                // tsc emits: "Type '(x: number) => number' is not assignable to type '() => number'."
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types(source, target);
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

            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let message = format!(
                        "Tuple type has {source_count} elements but target requires {target_count}."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::TupleElementTypeMismatch {
                index,
                source_element,
                target_element,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let source_str = self.format_type_diagnostic(*source_element);
                    let target_str = self.format_type_diagnostic(*target_element);
                    let message = format!(
                        "Type of element at index {index} is incompatible: '{source_str}' is not assignable to '{target_str}'."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => {
                if depth == 0 {
                    // At depth 0, tsc emits "Type X is not assignable to type Y" as
                    // the primary diagnostic; array-element detail is elaboration only.
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types(source, target);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let source_str = self.format_type_diagnostic(*source_element);
                    let target_str = self.format_type_diagnostic(*target_element);
                    let message = format!(
                        "Array element type '{source_str}' is not assignable to '{target_str}'."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind,
                source_value_type,
                target_value_type,
            } => {
                // At depth 0, tsc emits the top-level "Type 'X' is not assignable to type 'Y'"
                // message as the primary diagnostic. The index-signature detail is only shown
                // as secondary/related information (not captured in conformance fingerprints).
                // At depth > 0 (nested reasons), emit the specific detail message.
                if depth == 0 {
                    let source_str =
                        self.format_assignment_source_type_for_diagnostic(source, target, idx);
                    let target_str = self.format_assignability_type_for_message(target, source);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
                let source_str = self.format_type_diagnostic(*source_value_type);
                let target_str = self.format_type_diagnostic(*target_value_type);
                let message = format!(
                    "{index_kind} index signature is incompatible: '{source_str}' is not assignable to '{target_str}'."
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let use_assignment_source_display = depth == 0
                    && self
                        .assignment_source_expression(idx)
                        .and_then(|expr_idx| self.literal_expression_display(expr_idx))
                        .is_some();
                let source_str = if use_assignment_source_display {
                    self.format_assignment_source_type_for_diagnostic(source, target, idx)
                } else {
                    self.format_type_diagnostic(*source_type)
                };
                let target_str = if use_assignment_source_display {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target)
                };
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
                let source_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, idx);
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
                let source_str = if depth == 0 {
                    self.format_assignment_source_type_for_diagnostic(source, target, idx)
                } else {
                    self.format_nested_assignment_source_type_for_diagnostic(source, target, idx)
                };
                let target_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_for_assignability_message(target)
                };

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

                if depth == 0
                    && let Some(missing_props) =
                        self.missing_required_properties_from_index_signature_source(source, target)
                    && missing_props.len() > 1
                {
                    let evaluated_source = self.evaluate_type_for_assignability(source);
                    let src_str = self.format_type_diagnostic(evaluated_source);
                    let tgt_str = self.format_type_diagnostic(target);
                    let prop_list: Vec<String> = missing_props
                        .iter()
                        .take(4)
                        .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                        .collect();
                    let props_joined = prop_list.join(", ");
                    let message = if missing_props.len() > 4 {
                        let more_count = (missing_props.len() - 4).to_string();
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                            &[&src_str, &tgt_str, &props_joined, &more_count],
                        )
                    } else {
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                            &[&src_str, &tgt_str, &props_joined],
                        )
                    };
                    let code = if missing_props.len() > 4 {
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                    } else {
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    };
                    return Diagnostic::error(file_name, start, length, message, code);
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
                    if nonpublic.is_some() {
                        // TODO: tsc emits constructor param visibility as related information
                        return Diagnostic::error(
                            file_name,
                            start,
                            length,
                            base,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                    }
                }

                // TODO: tsc would emit elaboration from elaborate_type_mismatch_detail as related info
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            } => {
                // TS4104: "The type 'X' is 'readonly' and cannot be assigned to the mutable type 'Y'."
                // TSC emits this as the primary error (replacing TS2322) when a readonly
                // array/tuple is assigned to a mutable target in a variable assignment context.
                let source_str = self.format_type_diagnostic(*source_type);
                let target_str = self.format_type_diagnostic(*target_type);
                let message = format_message(
                    diagnostic_messages::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                )
            }

            _ => {
                // All remaining variants produce a generic "Type X is not assignable to type Y"
                // with TS2322 code. This covers: PropertyVisibilityMismatch,
                // PropertyNominalMismatch, ParameterTypeMismatch, NoIntersectionMemberMatches,
                // IntrinsicTypeMismatch, LiteralTypeMismatch, ErrorType,
                // RecursionLimitExceeded, ParameterCountMismatch.
                let source_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, idx);
                let target_str = self.format_assignability_type_for_message(target, source);
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

    /// Report a type not assignable error with detailed elaboration, preserving
    /// the provided anchor exactly instead of walking to an assignment anchor.
    pub fn error_type_not_assignable_with_reason_at_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
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

        let source_type = self.format_type_diagnostic(source);
        let target_type = self.format_type_diagnostic(target);
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

    /// Check if the diagnostic anchor node traces back to an assignment target
    /// whose variable declaration has an intersection type annotation.
    ///
    /// For `y = a;` where `y: { a: string } & { b: string }`:
    ///   anchor (`ExpressionStatement`) → expression (`BinaryExpression`) → left (Identifier)
    ///   → symbol → `value_declaration` (`VariableDeclaration`) → `type_annotation` (`IntersectionType`)
    fn anchor_target_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_target_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
    }

    /// Inner helper returning `Option` so we can use `?` for early returns.
    fn anchor_target_intersection_check_inner(&self, anchor_idx: NodeIndex) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let anchor_node = self.ctx.arena.get(anchor_idx)?;

        // Walk from anchor to the assignment target identifier
        let target_ident_idx = if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let expr_stmt = self.ctx.arena.get_expression_statement(anchor_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                binary.left
            } else {
                return Some(false);
            }
        } else {
            return Some(false);
        };

        // Check if the target is an identifier
        let ident_node = self.ctx.arena.get(target_ident_idx)?;
        if ident_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return Some(false);
        }

        // Resolve identifier to symbol
        let sym_id = self.resolve_identifier_symbol(target_ident_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Get value declaration
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;

        // Check if it's a variable declaration with an intersection type annotation
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
                return Some(type_node.kind == syntax_kind_ext::INTERSECTION_TYPE);
            }
        }

        Some(false)
    }

    fn missing_required_properties_from_index_signature_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        use tsz_solver::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, source) {
            return None;
        }

        let source_env_evaluated = self.evaluate_type_with_env(source);
        let source_evaluated = self.evaluate_type_for_assignability(source);
        let target_env_evaluated = self.evaluate_type_with_env(target);
        let target_evaluated = self.evaluate_type_for_assignability(target);

        let resolver = IndexSignatureResolver::new(self.ctx.types);
        let source_has_index = [source, source_env_evaluated, source_evaluated]
            .into_iter()
            .any(|candidate| {
                resolver.has_index_signature(candidate, IndexKind::String)
                    || resolver.has_index_signature(candidate, IndexKind::Number)
            });
        if !source_has_index {
            return None;
        }

        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                target_env_evaluated,
                target_evaluated,
            ]
            .into_iter()
            .find(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
            })?
        };

        let source_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                source_env_evaluated,
                source_evaluated,
            ]
            .into_iter()
            .find_map(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
        };
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let mut missing: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .filter(|prop| {
                !source_shape.as_ref().is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .any(|source_prop| source_prop.name == prop.name)
                })
            })
            .map(|prop| prop.name)
            .collect();
        missing.sort_by(|left, right| {
            self.ctx
                .types
                .resolve_atom_ref(*left)
                .cmp(&self.ctx.types.resolve_atom_ref(*right))
        });

        (!missing.is_empty()).then_some(missing)
    }
}
