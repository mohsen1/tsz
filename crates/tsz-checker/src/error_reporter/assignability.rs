//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::state::CheckerState;
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Returns true if the formatted type name matches a built-in wrapper type
/// (Boolean, Number, String, Object). These types inherit properties from Object
/// and missing-property diagnostics should be suppressed in favor of TS2322.
pub(super) fn is_builtin_wrapper_name(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String" | "Object")
}

/// Returns true if the formatted type name represents a TypeScript primitive type.
/// This catches cases where a complex type (e.g., homomorphic mapped type over a
/// primitive) evaluates/displays as a primitive, even if the solver's TypeId doesn't
/// directly represent the primitive.
pub(super) fn is_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "number"
            | "boolean"
            | "bigint"
            | "symbol"
            | "void"
            | "undefined"
            | "null"
            | "never"
    )
}

/// Returns true if the property name is a standard Object.prototype method.
/// These are implicitly available on all interfaces/objects through the Object
/// prototype chain. When such a property appears as "missing" in a subtype check,
/// it typically means the source type inherits it implicitly but its `ObjectShape`
/// doesn't include it. In this case, the mismatch is a type compatibility issue
/// (TS2322), not a missing property issue (TS2741).
pub(super) fn is_object_prototype_method(name: impl AsRef<str>) -> bool {
    matches!(
        name.as_ref(),
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
    /// Check if the assignment failure is due to exact optional property types.
    ///
    /// When `exactOptionalPropertyTypes` is enabled, optional properties don't
    /// implicitly include `undefined`. If the source has `undefined` for properties
    /// that are optional in the target, this is an exact optional property mismatch
    /// and should produce TS2375 instead of TS2322.
    ///
    /// Mirrors tsc's `getExactOptionalUnassignableProperties` + `isExactOptionalPropertyMismatch`.
    pub(super) fn has_exact_optional_property_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.ctx.compiler_options.exact_optional_property_types {
            return false;
        }
        // Evaluate types to resolve Lazy(DefId) references to concrete Object shapes
        let target_eval = self.evaluate_type_for_assignability(target);
        let source_eval = self.evaluate_type_for_assignability(source);
        let Some(target_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_eval)
        else {
            return false;
        };
        let source_shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, source_eval);
        for target_prop in &target_shape.properties {
            if !target_prop.optional {
                continue;
            }
            // Check if the source has a property with the same name that includes undefined
            let source_prop_type = source_shape
                .as_ref()
                .and_then(|s| s.properties.iter().find(|p| p.name == target_prop.name))
                .map(|p| p.type_id);
            if let Some(src_type) = source_prop_type
                && tsz_solver::type_queries::type_includes_undefined(self.ctx.types, src_type)
            {
                return true;
            }
        }
        false
    }

    /// Get the declaring type name for a property in a target type.
    /// For inherited properties (e.g., from a base class), returns the base class name.
    /// Falls back to formatting the target type if no parent info is available.
    pub(super) fn property_declaring_type_name(
        &self,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let prop_info = self.property_info_for_display(target_type, property_name)?;
        prop_info
            .parent_id
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .map(|sym| sym.escaped_name.clone())
    }

    pub(super) fn property_info_for_display(
        &self,
        ty: TypeId,
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)
            .and_then(|shape| {
                shape
                    .properties
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .cloned()
            })
            .or_else(|| {
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty).and_then(|shape| {
                    shape
                        .properties
                        .iter()
                        .find(|candidate| candidate.name == name)
                        .cloned()
                })
            })
    }

    pub(super) fn private_or_protected_member_missing_display(
        &self,
        source_type: TypeId,
        target_type: TypeId,
        required_property_name: Option<tsz_common::interner::Atom>,
    ) -> Option<(String, String, tsz_solver::Visibility)> {
        let source_has_prop = |name| self.property_info_for_display(source_type, name).is_some();

        let find_missing = |props: &[tsz_solver::PropertyInfo]| {
            props.iter().find_map(|prop| {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if prop_name.starts_with("__private_brand_")
                    || required_property_name.is_some_and(|required| prop.name != required)
                    || prop.visibility == tsz_solver::Visibility::Public
                    || source_has_prop(prop.name)
                {
                    return None;
                }

                let owner_name = prop
                    .parent_id
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .map(|sym| sym.escaped_name.clone())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_type));
                Some((prop_name, owner_name, prop.visibility))
            })
        };

        tsz_solver::type_queries::get_object_shape(self.ctx.types, target_type)
            .and_then(|shape| find_missing(&shape.properties))
            .or_else(|| {
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_type)
                    .and_then(|shape| find_missing(&shape.properties))
            })
    }

    // =========================================================================
    // Type Assignability Errors
    // =========================================================================

    /// Report a type not assignable error (delegates to `diagnose_assignment_failure`).
    pub fn error_type_not_assignable_at(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report a type not assignable error at an exact AST node anchor.
    pub fn error_type_not_assignable_at_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(anchor_idx, DiagnosticAnchorKind::Exact);
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
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::Exact)
        } else {
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment)
        };

        let mut base_diag = match reason {
            Some(reason) => self.render_failure_reason(&reason, source, target, anchor_idx, 0),
            None => {
                let Some(anchor) =
                    self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                else {
                    return;
                };
                let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                    self.ctx.file_name.as_str(),
                )
                .with_def_store(&self.ctx.definition_store)
                .with_namespace_module_names(&self.ctx.namespace_module_names);
                let diag = builder.type_not_assignable(source, target, anchor.start, anchor.length);
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
            let new_related = self
                .related_from_diagnostic(&base_diag, RelatedInformationPolicy::WRAPPED_DIAGNOSTIC);
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

        self.ctx.push_diagnostic(base_diag);
    }

    /// Diagnose why an assignment failed and report a detailed error.
    pub fn diagnose_assignment_failure(&mut self, source: TypeId, target: TypeId, idx: NodeIndex) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Internal helper that reports a detailed assignability failure using an
    /// already-resolved diagnostic anchor.
    pub(super) fn diagnose_assignment_failure_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        // Same TypeId → no actual type mismatch (failure at a higher structural level).
        if source == target {
            return;
        }
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
            let Some(anchor) =
                self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
            else {
                return;
            };

            let source_type = self.format_type_diagnostic(source);
            let target_type = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_type, &target_type],
            );

            let related = vec![DiagnosticRelatedInformation {
                category: DiagnosticCategory::Error,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                file: self.ctx.file_name.clone(),
                start: anchor.start,
                length: anchor.length,
                message_text: detail,
            }];

            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::with_related(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    message,
                    related,
                    RelatedInformationPolicy::ELABORATION,
                ),
            );
            return;
        }

        // TS2375: exactOptionalPropertyTypes — undefined assigned to optional property without undefined.
        if self.has_exact_optional_property_mismatch(source, target) {
            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                &[&src_str, &tgt_str],
            );
            if !self.emit_render_request(
                anchor_idx,
                DiagnosticRenderRequest::simple(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD,
                    message,
                ),
            ) {
                return;
            }
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
            Some(ref failure_reason) => {
                // Skip ExcessProperty — handled by check_object_literal_excess_properties (avoids duplicate TS2353/TS2561).
                if matches!(
                    failure_reason,
                    tsz_solver::SubtypeFailureReason::ExcessProperty { .. }
                ) {
                    return;
                }
                // Skip MissingProperty for computed symbol expressions (TS2339 emitted separately).
                if let tsz_solver::SubtypeFailureReason::MissingProperty { property_name, .. } =
                    &failure_reason
                {
                    let pn = self.ctx.types.resolve_atom_ref(*property_name);
                    if pn.starts_with("[Symbol.") || pn.starts_with("__js_ctor_brand_") {
                        return;
                    }
                }
                let diag =
                    self.render_failure_reason(failure_reason, source, target, anchor_idx, 0);
                self.ctx.push_diagnostic(diag);
            }
            None => {
                // Fallback to generic message
                self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
            }
        }
    }

    pub(super) fn format_top_level_assignability_message_types(
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
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
    }

    fn error_type_not_assignable_generic_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if source == target {
            return;
        }

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

        if let Some(anchor) =
            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
        {
            // Precedence gate: suppress fallback TS2322 when a more specific
            // diagnostic is already present at the same span.
            if self.has_more_specific_diagnostic_at_span(anchor.start, anchor.length) {
                return;
            }

            if let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            {
                let src_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
                let tgt_str = self.format_assignability_type_for_message(target, source);
                let (message, code) = if missing_props.len() == 1 {
                    let prop_name = self
                        .ctx
                        .types
                        .resolve_atom_ref(missing_props[0])
                        .to_string();
                    if prop_name.starts_with("__js_ctor_brand_") {
                        // Synthetic brand from JS constructor functions — TSC
                        // doesn't report these as missing properties.
                        return;
                    }
                    if prop_name.starts_with("__private_brand") {
                        if let Some((display_prop, owner_name, visibility)) =
                            self.private_or_protected_brand_backing_member_display(target, None)
                        {
                            (
                                self.private_or_protected_assignability_message(
                                    &src_str,
                                    &tgt_str,
                                    &display_prop,
                                    &owner_name,
                                    visibility,
                                    self.property_info_for_display(
                                        source,
                                        self.ctx.types.intern_string(&display_prop),
                                    )
                                    .map(|prop| prop.visibility),
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        } else {
                            (
                                format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&src_str, &tgt_str],
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        }
                    } else {
                        (
                            format_message(
                                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                                &[&prop_name, &src_str, &tgt_str],
                            ),
                            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        )
                    }
                } else {
                    let prop_list: Vec<String> = missing_props
                        .iter()
                        .take(4)
                        .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                        .collect();
                    let props_joined = prop_list.join(", ");
                    if missing_props.len() > 4 {
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
                    }
                };
                self.emit_render_request_at_anchor(
                    anchor,
                    DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
                );
                return;
            }

            let src_str =
                self.format_assignment_source_type_for_diagnostic(source, target, anchor_idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);

            // TS2719: when both types display identically but are different,
            // emit "Two different types with this name exist" instead of TS2322.
            let (message, code) = if src_str == tgt_str {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                        &[&src_str, &tgt_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                )
            } else {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            };
            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
            );
        }
    }

    // `render_failure_reason` has been moved to `render_failure.rs`.
    // The `format_top_level_assignability_message_types` helper remains here
    // because it is used by both assignability entry points and the render module.
}
