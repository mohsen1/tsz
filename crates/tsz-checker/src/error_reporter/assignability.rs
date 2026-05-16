//! Type assignability error reporting (TS2322 and related).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};
use crate::error_reporter::assignability_literal_display::display_has_boolean_member_literal_assignability;
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tracing::{Level, trace};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

pub(crate) use super::assignability_type_helpers::{
    display_is_literal_value, is_primitive_type_name, is_reserved_type_name,
};
pub(super) use super::assignability_type_helpers::{
    has_own_signature_type_params, is_builtin_wrapper_name, is_callable_application_type,
    is_object_prototype_method, is_object_prototype_method_for_array_target,
};

impl<'a> CheckerState<'a> {
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
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
            .and_then(|shape| {
                shape
                    .properties
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .cloned()
            })
            .or_else(|| {
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
                    .and_then(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|candidate| candidate.name == name)
                            .cloned()
                    })
            })
    }

    fn property_info_for_missing_property_satisfaction(
        &mut self,
        ty: TypeId,
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        let resolved = self.resolve_type_for_property_access(ty);
        let judged = self.judge_evaluate(resolved);
        let evaluated = self.evaluate_type_with_env(ty);
        let evaluated_resolved = self.resolve_type_for_property_access(evaluated);

        [ty, resolved, judged, evaluated, evaluated_resolved]
            .into_iter()
            .find_map(|candidate| self.property_info_for_display(candidate, name))
            .or_else(|| self.property_info_from_current_interface_declarations(ty, name))
    }

    fn property_info_from_current_interface_declarations(
        &mut self,
        ty: TypeId,
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        let sym_id = self.ctx.resolve_type_to_symbol_id(ty)?;
        let declarations = self.ctx.binder.get_symbol(sym_id)?.declarations.clone();

        declarations.into_iter().find_map(|decl_idx| {
            let is_current_interface = {
                let arena =
                    self.ctx
                        .binder
                        .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
                std::ptr::eq(arena, self.ctx.arena)
                    && arena
                        .get(decl_idx)
                        .is_some_and(|node| arena.get_interface(node).is_some())
            };
            if !is_current_interface {
                return None;
            }

            let diag_count_before = self.ctx.diagnostics.len();
            let interface_type = self.get_type_of_interface(decl_idx);
            self.ctx.diagnostics.truncate(diag_count_before);
            self.property_info_for_display(interface_type, name)
        })
    }

    fn property_info_for_any_missing_property_satisfaction_type(
        &mut self,
        types: &[TypeId],
        name: tsz_common::interner::Atom,
    ) -> Option<tsz_solver::PropertyInfo> {
        types
            .iter()
            .copied()
            .find_map(|ty| self.property_info_for_missing_property_satisfaction(ty, name))
    }

    fn missing_property_is_satisfied_by_source(
        &mut self,
        source_types: &[TypeId],
        target_types: &[TypeId],
        property_name: tsz_common::interner::Atom,
    ) -> bool {
        let Some(source_prop) = self
            .property_info_for_any_missing_property_satisfaction_type(source_types, property_name)
        else {
            return false;
        };
        if source_prop.optional || source_prop.visibility != tsz_solver::Visibility::Public {
            return false;
        }

        let Some(target_prop) = self
            .property_info_for_any_missing_property_satisfaction_type(target_types, property_name)
        else {
            return false;
        };
        if target_prop.visibility != tsz_solver::Visibility::Public {
            return false;
        }

        let read_ok = if source_prop.is_method || target_prop.is_method {
            self.is_assignable_to_bivariant(source_prop.type_id, target_prop.type_id)
        } else {
            self.is_assignable_to(source_prop.type_id, target_prop.type_id)
        };
        let write_ok = target_prop.readonly
            || self.is_assignable_to(target_prop.write_type, source_prop.write_type);

        read_ok && write_ok
    }

    fn should_suppress_outer_callback_return_assignability(
        &mut self,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        let Some(callback_idx) = self.callback_initializer_for_assignability_anchor(anchor_idx)
        else {
            return false;
        };
        if self.callback_has_explicit_param_type_conflict(callback_idx, target) {
            return false;
        }

        let Some(callback_node) = self.ctx.arena.get(callback_idx) else {
            return false;
        };
        let Some(function) = self.ctx.arena.get_function(callback_node) else {
            return false;
        };
        let Some(body_node) = self.ctx.arena.get(function.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return false;
        }

        self.has_diagnostic_code_within_span(
            body_node.pos,
            body_node.end,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        ) || self.has_diagnostic_code_within_span(
            body_node.pos,
            body_node.end,
            diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
        )
    }

    fn should_suppress_assignment_after_overload_failure(&self, anchor_idx: NodeIndex) -> bool {
        let Some(anchor_node) = self.ctx.arena.get(anchor_idx) else {
            return false;
        };
        if anchor_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return false;
        }
        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(anchor_node) else {
            return false;
        };
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_stmt.expression);
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
            return false;
        };
        if !self.is_assignment_operator(binary.operator_token) {
            return false;
        }
        let rhs_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(binary.right);
        let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
            return false;
        };
        if rhs_node.kind != syntax_kind_ext::CALL_EXPRESSION
            && rhs_node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return false;
        }
        self.ctx.diagnostics.iter().any(|diag| {
            diag.code == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                && diag.start >= rhs_node.pos
                && diag.start < rhs_node.end
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
                if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name)
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

        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_type)
            .and_then(|shape| find_missing(&shape.properties))
            .or_else(|| {
                crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    target_type,
                )
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

    /// Like `error_type_not_assignable_at_with_anchor`, but for object literal
    /// property-value elaboration contexts. TSC's `elaborateElementwise` reports
    /// TS2322 at the property name for property-value type mismatches, not
    /// TS2741/TS2739/TS2740 (missing property codes). This variant uses full
    /// failure analysis for accurate message formatting (e.g., union best-match),
    /// then downgrades any "missing property" code to TS2322.
    /// Like `error_type_not_assignable_at_with_anchor`, but for object literal
    /// property-value elaboration contexts. TSC's `elaborateElementwise` reports
    /// TS2322 at the property name for property-value type mismatches, not
    /// TS2741/TS2739/TS2740 (missing property codes). This variant uses full
    /// failure analysis for accurate message formatting (e.g., union best-match),
    /// then downgrades any "missing property" code to TS2322.
    ///
    /// NOTE: For empty object literals `{}` that are missing required properties,
    /// we should NOT downgrade TS2741 to TS2322 - we should keep TS2741 because
    /// the issue is missing properties, not type mismatch. Only downgrade when
    /// there are actual property-value type mismatches.
    pub fn error_type_not_assignable_at_with_anchor_elaboration(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.error_type_not_assignable_at_with_anchor_elaboration_inner(
            source, target, anchor_idx, false,
        );
    }

    /// Like `error_type_not_assignable_at_with_anchor_elaboration`, but when
    /// `downgrade_missing_to_2322` is true, converts TS2741/TS2739/TS2740
    /// (missing-property) diagnostics to TS2322 ("Type X is not assignable to
    /// type Y"). tsc's `elaborateElementwise` uses TS2322 for `this` keyword
    /// property values instead of the more specific missing-property codes.
    pub fn error_type_not_assignable_at_with_anchor_elaboration_inner(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        downgrade_missing_to_2322: bool,
    ) {
        self.error_type_not_assignable_at_with_anchor_elaboration_inner_with_value_anchor(
            source,
            target,
            anchor_idx,
            None,
            downgrade_missing_to_2322,
        );
    }

    /// Like [`error_type_not_assignable_at_with_anchor_elaboration_inner`], but
    /// also relocates any emitted missing-property diagnostics (TS2741/TS2739/
    /// TS2740) to `value_anchor_idx` when provided. tsc's
    /// `elaborateElementwise` anchors missing-property elaborations on the
    /// property initializer (the value), while plain TS2322 assignability
    /// diagnostics remain anchored on the property name — so callers pass the
    /// value anchor only when they want missing-property codes repositioned.
    pub fn error_type_not_assignable_at_with_anchor_elaboration_inner_with_value_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        value_anchor_idx: Option<NodeIndex>,
        downgrade_missing_to_2322: bool,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(anchor_idx, DiagnosticAnchorKind::Exact);
        let diag_count_before = self.ctx.diagnostics.len();
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);

        use crate::diagnostics::diagnostic_codes;

        // When a value anchor is supplied, reposition missing-property codes
        // (TS2741/TS2739/TS2740) to anchor on the property value — matching
        // tsc's `elaborateElementwise` behavior that uses the initializer as
        // the error node for missing-property elaborations.
        if let Some(value_anchor_src) = value_anchor_idx {
            let resolved_value_anchor =
                self.resolve_diagnostic_anchor_node(value_anchor_src, DiagnosticAnchorKind::Exact);
            let value_span = self
                .resolve_diagnostic_anchor(resolved_value_anchor, DiagnosticAnchorKind::Exact)
                .map(|anchor| (anchor.start, anchor.length))
                .or_else(|| {
                    self.get_node_span(resolved_value_anchor).map(|(pos, end)| {
                        self.normalized_anchor_span(
                            resolved_value_anchor,
                            pos,
                            end.saturating_sub(pos),
                        )
                    })
                });
            if let Some((start, length)) = value_span {
                for diag in &mut self.ctx.diagnostics[diag_count_before..] {
                    if matches!(
                        diag.code,
                        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                            | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                            | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                    ) {
                        diag.start = start;
                        diag.length = length;
                    }
                }
            }
        }

        if !downgrade_missing_to_2322 {
            return;
        }

        let needs_downgrade = self.ctx.diagnostics[diag_count_before..].iter().any(|d| {
            matches!(
                d.code,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            )
        });
        if needs_downgrade {
            let src_str = "this".to_string();
            let tgt_str = self.format_type_for_assignability_message(target);
            let (src_str, tgt_str) =
                self.finalize_pair_display_for_diagnostic(source, target, src_str, tgt_str);
            let new_message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            for diag in &mut self.ctx.diagnostics[diag_count_before..] {
                if matches!(
                    diag.code,
                    diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                        | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                        | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                ) {
                    diag.code = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;
                    diag.message_text = new_message.clone();
                }
            }
        }
    }
    pub fn error_type_does_not_satisfy_the_expected_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) {
        if !self.has_exact_optional_property_mismatch(source, target)
            && self.should_suppress_assignability_diagnostic(source, target)
        {
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

        // Mutate the top-level diagnostic to be TS1360.
        // When the target is not literal-sensitive (e.g. `1 satisfies boolean`),
        // widen a bare literal source for display to match tsc, which reports
        // `Type 'number' does not satisfy the expected type 'boolean'.`
        // (tsc's `typeToString` widens fresh literal primitives when the target
        // type does not preserve literal display.)
        let display_source = if self.is_literal_sensitive_assignment_target(target) {
            source
        } else {
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, source)
        };
        let src_str = self.format_type_for_assignability_message(display_source);
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
        if !self.has_exact_optional_property_mismatch(source, target)
            && self.should_suppress_assignability_diagnostic(source, target)
        {
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
        if self.should_suppress_assignment_after_overload_failure(anchor_idx) {
            return;
        }

        let has_callable_shape = |this: &mut Self, ty: TypeId| {
            crate::query_boundaries::common::function_shape_for_type(this.ctx.types, ty).is_some()
                || crate::query_boundaries::common::callable_shape_for_type(this.ctx.types, ty)
                    .is_some()
                || {
                    let evaluated = this.evaluate_type_with_env(ty);
                    crate::query_boundaries::common::function_shape_for_type(
                        this.ctx.types,
                        evaluated,
                    )
                    .is_some()
                        || crate::query_boundaries::common::callable_shape_for_type(
                            this.ctx.types,
                            evaluated,
                        )
                        .is_some()
                }
        };
        if has_callable_shape(self, source)
            && has_callable_shape(self, target)
            && let Some(arg_node) = self.ctx.arena.get(anchor_idx)
            && matches!(arg_node.kind, k if k == syntax_kind_ext::ARROW_FUNCTION || k == syntax_kind_ext::FUNCTION_EXPRESSION)
            && let Some(func) = self.ctx.arena.get_function(arg_node)
            && let Some(body_node) = self.ctx.arena.get(func.body)
            && body_node.kind != syntax_kind_ext::BLOCK
            && self.has_diagnostic_code_within_span(
                body_node.pos,
                body_node.end,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        {
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

            let (source_type, target_type) =
                self.format_top_level_assignability_message_types_at(source, target, anchor_idx);
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

        // Exact-optional presence checks make `obj.a = obj.a` safe in the present branch.
        if self.ctx.compiler_options.exact_optional_property_types
            && self.same_property_self_assignment_in_presence_true_branch_for_anchor(anchor_idx)
        {
            return;
        }

        // TS2375: exactOptionalPropertyTypes — undefined assigned to optional property without undefined.
        if self.has_exact_optional_property_mismatch(source, target) {
            let src_str = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
            );
            let tgt_str = self.format_exact_optional_target_type_for_message(target);
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

        // TS2412: exactOptionalPropertyTypes write target mismatch (property/element write).
        if self.has_exact_optional_write_target_mismatch(source, target, anchor_idx) {
            // tsc reports the offending portion of the source — when the source
            // is `T | undefined` and the target is `T`, the diagnostic narrows
            // the source to `undefined` because `T` is assignable but
            // `undefined` is not under `exactOptionalPropertyTypes`. Surface
            // that narrowed display when the union strip leaves the target's
            // shape intact.
            let narrowed_source =
                self.exact_optional_source_for_message(source, target, anchor_idx);
            let src_str = if narrowed_source == TypeId::UNDEFINED {
                self.format_type_diagnostic(narrowed_source)
            } else {
                self.format_type_for_diagnostic_role(
                    narrowed_source,
                    DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                )
            };
            let tgt_str = self.format_exact_optional_target_type_for_message(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2,
                &[&src_str, &tgt_str],
            );
            if !self.emit_render_request(
                anchor_idx,
                DiagnosticRenderRequest::simple(
                    DiagnosticAnchorKind::Exact,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_WITH_EXACTOPTIONALPROPERTYTYPES_TRUE_CONSIDER_ADD_2,
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
                // ExcessProperty errors need special handling: emit at the property position,
                // not the statement position. Find the object literal and call the excess
                // property checker to emit at the correct position.
                if matches!(
                    failure_reason,
                    tsz_solver::SubtypeFailureReason::ExcessProperty { .. }
                ) {
                    // Walk through statements and binary expressions to find the object literal
                    let start_idx = if let Some(node) = self.ctx.arena.get(anchor_idx) {
                        // If anchor is a return statement, start from its expression
                        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                            self.ctx
                                .arena
                                .get_return_statement(node)
                                .and_then(|ret| {
                                    if ret.expression.is_some() {
                                        Some(ret.expression)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(anchor_idx)
                        } else {
                            anchor_idx
                        }
                    } else {
                        anchor_idx
                    };
                    let literal_idx = self.find_rhs_object_literal(start_idx);
                    if let Some(obj_idx) = literal_idx {
                        self.check_object_literal_excess_properties(source, target, obj_idx);
                    }
                    // If we can't find an object literal, the solver's excess property
                    // check may be from a non-literal fresh type (shouldn't happen in
                    // typical code, but fallback to avoid silent suppression).
                    return;
                }
                // Skip MissingProperty for computed symbol expressions (TS2339 emitted separately).
                if let tsz_solver::SubtypeFailureReason::MissingProperty {
                    property_name,
                    source_type,
                    target_type,
                } = &failure_reason
                {
                    let pn = self.ctx.types.resolve_atom_ref(*property_name);
                    if pn.starts_with("[Symbol.") || pn.starts_with("__js_ctor_brand_") {
                        return;
                    }
                    if self.missing_property_is_satisfied_by_source(
                        &[source, *source_type],
                        &[target, *target_type],
                        *property_name,
                    ) {
                        return;
                    }
                }
                if is_callable_application_type(self.ctx.types, source)
                    && is_callable_application_type(self.ctx.types, target)
                    && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
                {
                    return;
                }
                let mut diag =
                    self.render_failure_reason(failure_reason, source, target, anchor_idx, 0);
                if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
                    diag.message_text = self
                        .rewrite_declared_generic_alias_source_in_ts2322_message(
                            anchor_idx,
                            diag.message_text,
                        );
                }
                self.ctx.push_diagnostic(diag);
            }
            None => {
                // Before falling back to generic TS2322, check if there are missing
                // properties from index signature source. If so, emit TS2741 instead.
                if let Some(anchor) =
                    self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
                    && let Some(missing_props) =
                        self.missing_required_properties_from_index_signature_source(source, target)
                {
                    // For TS2739, when the source is a non-generic type alias
                    // whose body is a generic Application (`type B = A<X1, X2, ...>`),
                    // tsc unfolds one level to display the application form
                    // `A<X1, X2, ...>` rather than the wrapper alias name `B`.
                    // See `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
                    // line 91, which expects `Type 'NumberTo<number>'` for
                    // `type NumberToNumber = NumberTo<number>` source. The unfold
                    // is scoped to the missing-properties source only — TS2322
                    // target context and TS2339 receiver keep the alias name.
                    let src_str = if let Some(display) =
                        self.ts2739_alias_of_application_source_display_text(source)
                    {
                        display
                    } else {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                        )
                    };
                    let tgt_str = self.format_type_for_diagnostic_role(
                        target,
                        DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
                    );
                    let (message, code) = if missing_props.len() == 1 {
                        let prop_name = self
                            .ctx
                            .types
                            .resolve_atom_ref(missing_props[0])
                            .to_string();
                        if prop_name.starts_with("__js_ctor_brand_") {
                            // Synthetic brand from JS constructor functions — TSC
                            // doesn't report these as missing properties.
                            self.error_type_not_assignable_generic_with_anchor(
                                source, target, anchor_idx,
                            );
                            return;
                        }
                        if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                            // Private brand mismatch
                            self.error_type_not_assignable_generic_with_anchor(
                                source, target, anchor_idx,
                            );
                            return;
                        }
                        (
                                format_message(
                                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                                    &[&prop_name, &src_str, &tgt_str],
                                ),
                                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                            )
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
                // Fallback to generic message
                self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
            }
        }
    }

    /// Narrow the TS2412 source display to the offending member when the
    /// source is a union that contains the target type's shape. In that case
    /// only the `null` / `undefined` (or other non-overlapping) members are
    /// the actual mismatch, and tsc reports just those rather than the full
    /// source union.
    fn exact_optional_source_for_message(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> TypeId {
        if self.same_property_self_assignment_in_presence_false_branch(anchor_idx) {
            return TypeId::UNDEFINED;
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, source_eval)
        else {
            return source;
        };
        let mismatched: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| !self.is_assignable_to(m, target_eval))
            .collect();
        if mismatched.len() == members.len()
            && members.len() == 2
            && members.contains(&TypeId::UNDEFINED)
            && !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target)
        {
            return TypeId::UNDEFINED;
        }
        source
    }

    fn format_exact_optional_target_type_for_message(&mut self, target: TypeId) -> String {
        // Honor any display-alias attached during type construction (e.g.
        // JSDoc `@typedef {object} A` stores `body_type → lazy(def_for_A)`).
        // tsc reports the alias name `A` in TS2375 messages instead of
        // expanding to the body's structural form `{ value?: number; }`.
        if let Some(alias_id) = self.ctx.types.get_display_alias(target)
            && let Some(name) = self.authoritative_assignability_def_name(alias_id)
        {
            return name;
        }
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_preserve_optional_parameter_surface_syntax(true)
            .with_preserve_optional_property_surface_syntax(true);
        formatter.format(target).into_owned()
    }

    pub(super) fn format_top_level_assignability_message_types(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (String, String) {
        let source_str = self
            .related_generic_indexed_access_source_display(source, target)
            .unwrap_or_else(|| self.format_assignability_type_for_message(source, target));
        let mut source_str = self.rewrite_source_display_for_non_literal_target_assignability(
            source, target, source_str,
        );
        let target_str = self.format_assignability_type_for_message(target, source);
        let mut target_str =
            self.rewrite_target_display_for_non_literal_assignability(target, target_str);

        source_str = self.apply_ts2739_nonliteral(source, source_str);
        if target_str.trim() != "{}"
            && let Some(unfolded) = self.ts2739_alias_target_display(target, &target_str)
        {
            target_str = self.format_type_diagnostic(unfolded);
        }

        let should_prefer_authoritative_name = |display: &str| {
            display.starts_with("{ ")
                || display.starts_with("typeof import(")
                || display.contains("& typeof import(")
        };

        if should_prefer_authoritative_name(&source_str)
            && let Some(authoritative) = self.authoritative_assignability_def_name(source)
        {
            source_str = authoritative;
        }
        if should_prefer_authoritative_name(&target_str)
            && let Some(authoritative) = self.authoritative_assignability_def_name(target)
        {
            target_str = authoritative;
        }

        // Non-generic aliases that wrap applications display the application.
        let rewrite_application_alias =
            |state: &Self, ty: TypeId, display: &str| -> Option<String> {
                if display.contains('<') || display.contains('{') || display.contains('|') {
                    return None; // Already expanded
                }
                if display.starts_with('"')
                    || display.starts_with('`')
                    || display == "true"
                    || display == "false"
                {
                    return None; // Keep concrete literal displays instead of repainting alias provenance.
                }
                // JSDoc typedef lazy aliases must not trigger this rewrite.
                let alias = state.ctx.types.get_display_alias(ty)?;
                crate::query_boundaries::common::application_info(state.ctx.types, alias)?;
                let mut formatter = state
                    .ctx
                    .create_diagnostic_type_formatter()
                    .with_display_properties()
                    .with_skip_application_alias_names();
                Some(formatter.format(ty).into_owned())
            };
        if let Some(rewritten) = rewrite_application_alias(self, source, &source_str) {
            source_str = rewritten;
        }
        if let Some(rewritten) = rewrite_application_alias(self, target, &target_str) {
            target_str = rewritten;
        }
        source_str = self.apply_eval_alias_nonliteral(source, source_str);
        if let Some(display) = self.evaluated_literal_alias_source_display(target) {
            target_str = display;
        }
        source_str =
            self.canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        if let Some(widened) =
            self.rewrite_standalone_literal_source_for_keyof_display(source, target)
        {
            source_str = widened;
        }
        let (source_str, mut target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
        let mut source_str = source_str;
        if let Some(display) = self.static_schema_array_structural_display(source, target) {
            source_str = display;
        }
        if let Some(display) = self.static_schema_array_structural_display(target, source) {
            target_str = display;
        }
        if let Some((direct_source, direct_target)) =
            self.direct_type_param_alias_application_pair_display(source, target)
        {
            source_str = direct_source;
            target_str = direct_target;
        }
        if let Some(display) =
            self.contextual_callable_application_target_display(target, source, &target_str)
        {
            target_str = display;
        }
        source_str =
            self.canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        (source_str, target_str)
    }

    pub(in crate::error_reporter) fn rewrite_standalone_literal_source_for_keyof_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if !self.target_is_generic_keyof_display(target) {
            return None;
        }

        crate::query_boundaries::common::literal_value(self.ctx.types, source)?;
        match crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, source) {
            TypeId::BOOLEAN => Some("boolean".to_string()),
            TypeId::STRING => Some("string".to_string()),
            TypeId::NUMBER => Some("number".to_string()),
            _ => None,
        }
    }

    fn target_is_generic_keyof_display(&mut self, target: TypeId) -> bool {
        let evaluated_target = self.evaluate_type_for_assignability(target);
        for candidate in [target, evaluated_target] {
            if self.type_is_generic_keyof(candidate) {
                return true;
            }
            if let Some(alias) = self.ctx.types.get_display_alias(candidate)
                && self.type_is_generic_keyof(alias)
            {
                return true;
            }
        }
        false
    }

    fn type_is_generic_keyof(&mut self, type_id: TypeId) -> bool {
        let Some(operand) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, type_id)
        else {
            return false;
        };
        crate::query_boundaries::common::contains_type_parameters(self.ctx.types, operand)
            || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                self.evaluate_type_for_assignability(operand),
            )
    }

    pub(super) fn format_top_level_assignability_message_types_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> (String, String) {
        let (mut source_str, _) = self.format_top_level_assignability_message_types(source, target);
        if self
            .array_literal_element_source_widening_required_for_display(anchor_idx, source, target)
        {
            let widened = self.widen_type_for_display(source);
            source_str = self.format_assignability_type_for_message(widened, target);
        }
        let mut source_from_annotation = false;
        let mut source_from_array_literal_tuple = false;
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(annotation_text) =
                self.declared_type_annotation_text_for_expression(expr_idx)
            && annotation_text.contains('&')
            && !annotation_text.trim_start().starts_with("keyof ")
            && self.should_prefer_declared_source_annotation_display(
                expr_idx,
                source,
                &annotation_text,
            )
        {
            source_str = self
                .declared_intersection_annotation_display_for_expression(expr_idx)
                .unwrap_or_else(|| {
                    self.format_declared_annotation_for_diagnostic(&annotation_text)
                });
            source_from_annotation = true;
        }
        if self
            .collapsed_anonymous_object_intersection_for_assignability_display(source)
            .is_some()
            && let Some(annotation_text) =
                self.line_rhs_declared_intersection_annotation(anchor_idx)
        {
            source_str = self.format_declared_annotation_for_diagnostic(&annotation_text);
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(object_display) =
                self.object_literal_source_type_display(anchor_idx, Some(target))
        {
            source_str = self.rewrite_source_display_for_non_literal_target_assignability(
                source,
                target,
                object_display,
            );
        }
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx));
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(display) = self.direct_type_query_primitive_source_display(expr_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(display) =
                self.declared_numeric_literal_union_alias_source_display(expr_idx, source)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && !self.declared_identifier_has_literal_only_alias_source(expr_idx)
            && let Some(display) = self.declared_identifier_source_display(expr_idx, target, source)
            && self.declared_identifier_candidate_preserves_source_surface(&source_str, &display)
        {
            source_str = display;
            source_from_annotation = true;
        }
        if !source_from_annotation
            && self.target_is_normalized_object_literal_union(target)
            && let Some(expr_idx) = expr_idx
            && let Some(object_display) =
                self.object_literal_source_type_display(expr_idx, Some(target))
        {
            source_str = object_display;
        }
        if !source_from_annotation
            && let Some(expr_idx) = expr_idx
            && let Some(tuple_display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            source_str = tuple_display;
            source_from_array_literal_tuple = true;
        }
        if self
            .array_literal_element_source_widening_required_for_display(anchor_idx, source, target)
        {
            let widened = self.widen_type_for_display(source);
            source_str = self.format_assignability_type_for_message(widened, target);
        }
        if let Some(display) = self.literal_assignment_source_display_for_target(target, anchor_idx)
        {
            source_str = display;
        }
        let target_str = self.format_type_for_diagnostic_role(
            target,
            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
        );
        if !source_from_annotation
            && let Some(display) = self.declared_generic_alias_source_display_for_target_display(
                anchor_idx,
                &source_str,
                &target_str,
            )
        {
            source_str = display;
            source_from_annotation = true;
        }
        let (source_str, mut target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
        let mut source_str = source_str;
        if !source_from_annotation && !source_from_array_literal_tuple {
            source_str = self.apply_ts2739_nonliteral(source, source_str);
        }
        if target_str.trim() != "{}"
            && let Some(unfolded) = self.ts2739_alias_target_display(target, &target_str)
        {
            target_str = self.format_type_diagnostic(unfolded);
        }
        if let Some(display) = self.static_schema_array_structural_display(source, target) {
            source_str = display;
        }
        if let Some(display) = self.static_schema_array_structural_display(target, source) {
            target_str = display;
        }
        if let Some(display) =
            self.contextual_callable_application_target_display(target, source, &target_str)
        {
            target_str = display;
        }
        if !source_from_annotation {
            source_str = self
                .canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        }
        target_str =
            self.canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        (source_str, target_str)
    }

    pub(super) fn rewrite_source_display_for_non_literal_target_assignability(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_display: String,
    ) -> String {
        let target_is_constructor_like =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, target)
                .is_some_and(|shape| shape.is_constructor)
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                    .is_some_and(|shape| !shape.construct_signatures.is_empty());
        let evaluated_source = self.evaluate_type_for_assignability(source);
        let source_has_display_props = self.ctx.types.get_display_properties(source).is_some()
            || self
                .ctx
                .types
                .get_display_properties(evaluated_source)
                .is_some();

        if source_has_display_props
            && self.target_is_normalized_object_literal_union(target)
            && display_has_boolean_member_literal_assignability(&source_display)
        {
            return source_display;
        }

        if self.is_literal_sensitive_assignment_target(target)
            || self.target_preserves_literal_surface(target)
            || (source_display.contains("=>") && !target_is_constructor_like)
            || !Self::display_has_member_literals_assignability(&source_display)
        {
            return source_display;
        }

        // Application types (generic instantiations like `Foo<{ b?: 1; x: 1 }>`)
        // carry literals in their type arguments — these come from type annotations,
        // not from fresh expression literals, and must NOT be text-widened.
        // tsc always shows literal type args as-is in assignability messages.
        if Self::type_displays_as_application(self.ctx.types, source) {
            return source_display;
        }

        // Declared type annotations (e.g. `var z: { length: 2; }`) store literal
        // property types canonically with no display_properties. Only fresh object
        // literal expressions carry display_properties (canonical=widened, display=literal).
        // tsc preserves the annotation's literal property types in error messages.
        //
        // Skip widening when source has no display_properties AND has at least one direct
        // canonical property of literal type. The "direct" check prevents false positives
        // from outer types like `{ a: inner_fresh }` where the outer is not fresh but inner
        // properties contain fresh types — their outer canonical properties are object types
        // (not literals), so they correctly fall through to the widening path.
        let source_is_array =
            crate::query_boundaries::common::array_element_type(self.ctx.types, source).is_some()
                || crate::query_boundaries::common::array_element_type(
                    self.ctx.types,
                    evaluated_source,
                )
                .is_some();
        if !source_has_display_props && !source_is_array {
            let has_direct_literal_prop = crate::query_boundaries::common::object_shape_for_type(
                self.ctx.types,
                evaluated_source,
            )
            .is_some_and(|shape| {
                shape.properties.iter().any(|p| {
                    crate::query_boundaries::common::is_literal_type(self.ctx.types, p.type_id)
                })
            });
            if has_direct_literal_prop {
                return source_display;
            }
        }

        // For intersection types with display properties (fresh object literal in an
        // intersection), check whether the *target* type has literal-typed properties.
        // tsc preserves literal display when the target expects literals (e.g.
        // `fooProp: "hello" | "world"`), but widens to primitives when the target
        // has non-literal property types (e.g. `fooProp: boolean`).
        let is_intersection_source = [source, self.evaluate_type_for_assignability(source)]
            .into_iter()
            .any(|candidate| {
                crate::query_boundaries::common::is_intersection_type(self.ctx.types, candidate)
                    && self.ctx.types.get_display_properties(candidate).is_some()
            });
        if is_intersection_source && self.target_has_literal_typed_properties(target) {
            return source_display;
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened_display = self
            .format_type_for_diagnostic_role(widened, DiagnosticTypeDisplayRole::WidenedDiagnostic);
        if Self::display_has_member_literals_assignability(&widened_display) {
            Self::widen_member_literals_in_display_text(&widened_display)
        } else {
            widened_display
        }
    }

    pub(super) fn rewrite_target_display_for_non_literal_assignability(
        &mut self,
        target: TypeId,
        target_display: String,
    ) -> String {
        // Callable types use syntax like `{ (x: "foo"): number; }` which has `: "` pattern
        // but these are parameter literals that should be preserved, not object property
        // literals that should be widened. Skip rewriting for callable types.
        let is_callable_type =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                .is_some();
        if target_display.contains("=>")
            || is_callable_type
            || !Self::display_has_member_literals_assignability(&target_display)
        {
            return target_display;
        }

        // Application types carry literals in type arguments — preserve them.
        if Self::type_displays_as_application(self.ctx.types, target) {
            return target_display;
        }
        let evaluated = self.evaluate_type_for_assignability(target);
        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, evaluated);
        let widened = self.widen_function_like_display_type(widened);
        let widened_display = self
            .format_type_for_diagnostic_role(widened, DiagnosticTypeDisplayRole::WidenedDiagnostic);
        if Self::display_has_member_literals_assignability(&widened_display) {
            Self::widen_member_literals_in_display_text(&widened_display)
        } else {
            widened_display
        }
    }

    /// Returns true when `ty` would be formatted as an Application type (e.g. `Foo<{...}>`).
    ///
    /// Application types carry their type arguments from annotations — the literals in those
    /// args represent declared types, not fresh expression values, and must never be text-widened
    /// in `rewrite_{source,target}_display_for_non_literal_*` calls.
    fn type_displays_as_application(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
        // Direct Application: Application(Lazy(Foo), [args])
        if crate::query_boundaries::common::is_generic_application(db, ty) {
            return true;
        }
        // Evaluated Application: concrete Object that carries display_alias → Application
        if let Some(alias) = db.get_display_alias(ty)
            && crate::query_boundaries::common::is_generic_application(db, alias)
        {
            return true;
        }
        false
    }

    /// Check if the target type has any properties whose types contain literal
    /// types.  Used to decide whether to preserve source literal display in
    /// intersection contexts: tsc shows `"frizzlebizzle"` when the target expects
    /// `"hello" | "world"`, but widens to `string` when the target expects `boolean`.
    fn target_has_literal_typed_properties(&mut self, target: TypeId) -> bool {
        let target = self.evaluate_type_for_assignability(target);
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            .or_else(|| {
                // For intersection/union targets, check members.
                crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                    .and_then(|members| {
                        members.iter().find_map(|&m| {
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                m,
                            )
                        })
                    })
            });
        let Some(shape) = shape else {
            return false;
        };
        shape
            .properties
            .iter()
            .any(|prop| self.is_literal_sensitive_assignment_target(prop.type_id))
    }

    pub(super) fn display_has_member_literals_assignability(display: &str) -> bool {
        let bytes = display.as_bytes();
        if bytes.len() < 3 {
            return false;
        }
        for i in 0..(bytes.len() - 2) {
            if bytes[i] != b':' || bytes[i + 1] != b' ' {
                continue;
            }
            let rest = &display[i + 2..];
            if rest.starts_with('"')
                || rest.starts_with('\'')
                || rest.starts_with("true")
                || rest.starts_with("false")
            {
                return true;
            }
            if rest
                .as_bytes()
                .first()
                .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
            {
                return true;
            }
        }
        false
    }

    /// Check if a type display string contains duplicate type names in a
    /// union (`Yep | Yep`) or tuple (`[Yep, Yep]`) context.
    pub(super) fn has_duplicate_union_member_names(display: &str) -> bool {
        // Try union split first
        if display.contains(" | ") {
            let members: Vec<&str> = display.split(" | ").collect();
            if members.len() >= 2 {
                for i in 0..members.len() {
                    for j in (i + 1)..members.len() {
                        if members[i] == members[j] {
                            return true;
                        }
                    }
                }
            }
        }
        // Try tuple split (e.g., "[Yep, Yep]")
        let inner = display.strip_prefix('[').and_then(|s| s.strip_suffix(']'));
        if let Some(inner) = inner {
            let members: Vec<&str> = inner.split(", ").collect();
            if members.len() >= 2 {
                for i in 0..members.len() {
                    for j in (i + 1)..members.len() {
                        if members[i] == members[j] {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub(super) fn widen_member_literals_in_display_text(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0usize;
        let is_boundary = |b: u8| {
            matches!(
                b,
                b';' | b',' | b'}' | b'>' | b')' | b'|' | b'&' | b']' | b' '
            )
        };
        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b' ' {
                out.push(':');
                out.push(' ');
                i += 2;

                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' && i + 1 < bytes.len() {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'"' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    out.push_str("string");
                    continue;
                }

                if display[i..].starts_with("true")
                    && (i + 4 >= bytes.len() || is_boundary(bytes[i + 4]))
                {
                    out.push_str("boolean");
                    i += 4;
                    continue;
                }
                if display[i..].starts_with("false")
                    && (i + 5 >= bytes.len() || is_boundary(bytes[i + 5]))
                {
                    out.push_str("boolean");
                    i += 5;
                    continue;
                }

                if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                    let mut j = i;
                    if bytes[j] == b'-' {
                        j += 1;
                    }
                    let mut saw_digit = false;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                        saw_digit = true;
                    }
                    if j < bytes.len() && bytes[j] == b'.' {
                        j += 1;
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            j += 1;
                            saw_digit = true;
                        }
                    }
                    if saw_digit && (j >= bytes.len() || is_boundary(bytes[j])) {
                        out.push_str("number");
                        i = j;
                        continue;
                    }
                }
            }

            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

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

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let src_callable = is_callable_application_type(self.ctx.types, source);
        let tgt_callable = is_callable_application_type(self.ctx.types, target);
        let has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, source);
        let both_have_own_sig_params = has_own_signature_type_params(self.ctx.types, source)
            && has_own_signature_type_params(self.ctx.types, target);
        if src_callable && tgt_callable && has_type_params && !both_have_own_sig_params {
            return;
        }

        if let Some(anchor) =
            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
        {
            if is_callable_application_type(self.ctx.types, source)
                && is_callable_application_type(self.ctx.types, target)
                && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
            {
                return;
            }

            // Precedence gate: suppress fallback TS2322 when a more specific
            // diagnostic is already present at the same span.
            if self.has_more_specific_diagnostic_at_span(anchor.start, anchor.length) {
                return;
            }

            if self.is_nested_same_wrapper_assignment_display_provenance(source, target, anchor_idx)
            {
                return;
            }

            if let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            {
                // TS2739/TS2741 unfold `type B = A<X>` sources to `A<X>`;
                // otherwise fall through to normal source-role formatting.
                let src_str = if let Some(display) =
                    self.ts2739_alias_of_application_source_display_text(source)
                {
                    display
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                    )
                };
                let tgt_str = self.format_type_for_diagnostic_role(
                    target,
                    DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
                );
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
                    if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
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

            let src_str = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
            );
            let tgt_str = self.format_type_for_diagnostic_role(
                target,
                DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
            );
            let (src_str, tgt_str) =
                self.finalize_pair_display_for_diagnostic(source, target, src_str, tgt_str);
            let mut src_str = src_str;
            let mut tgt_str = tgt_str;
            let source_is_direct_type_query_primitive = self
                .direct_diagnostic_source_expression(anchor_idx)
                .or_else(|| self.assignment_source_expression(anchor_idx))
                .and_then(|expr_idx| {
                    self.direct_type_query_primitive_source_display(expr_idx, source)
                })
                .is_some_and(|display| {
                    if display != src_str {
                        src_str = display;
                    }
                    true
                });
            let source_expr_idx = self
                .assignment_source_expression(anchor_idx)
                .or_else(|| self.direct_diagnostic_source_expression(anchor_idx));
            if !source_is_direct_type_query_primitive
                && let Some(expr_idx) = source_expr_idx
                && !self.declared_identifier_has_literal_only_alias_source(expr_idx)
                && let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, source)
                && self.declared_identifier_candidate_preserves_source_surface(&src_str, &display)
            {
                src_str = display;
            }
            if !source_is_direct_type_query_primitive
                && let Some(display) = self.nonmissing_ts2739_alias_source_display_text(source)
            {
                src_str = display;
            }
            if tgt_str.trim() != "{}"
                && let Some(unfolded) = self.ts2739_alias_target_display(target, &tgt_str)
            {
                tgt_str = self.format_type_diagnostic(unfolded);
            }
            if let Some(display) = self.declared_generic_alias_source_display_for_target_display(
                anchor_idx, &src_str, &tgt_str,
            ) {
                src_str = display;
            }
            if let Some(display) = self.static_schema_array_structural_display(source, target) {
                src_str = display;
            }
            if let Some(display) = self.static_schema_array_structural_display(target, source) {
                tgt_str = display;
            }
            if let Some((direct_source, direct_target)) =
                self.direct_type_param_alias_application_pair_display(source, target)
            {
                src_str = direct_source;
                tgt_str = direct_target;
            }
            if let Some(display) = self.type_query_static_array_structural_display(&src_str) {
                src_str = display;
            }
            let source_from_annotation = self
                .direct_diagnostic_source_expression(anchor_idx)
                .or_else(|| self.assignment_source_expression(anchor_idx))
                .and_then(|expr_idx| {
                    self.declared_numeric_literal_union_alias_source_display(expr_idx, source)
                })
                .map(|display| {
                    src_str = display;
                })
                .is_some();
            if !source_from_annotation {
                src_str = self
                    .canonicalize_assignment_numeric_literal_union_display(source, target, src_str);
            }
            tgt_str =
                self.canonicalize_assignment_numeric_literal_union_display(target, source, tgt_str);
            // TS2719: when both types display identically but are different,
            // emit "Two different types with this name exist" instead of TS2322.
            let authoritative_src = self.authoritative_assignability_def_name(source);
            let authoritative_tgt = self.authoritative_assignability_def_name(target);
            let authoritative_names_differ = authoritative_src
                .as_ref()
                .zip(authoritative_tgt.as_ref())
                .is_some_and(|(src, tgt)| src != tgt);

            // Do not repaint literal displays as boxed/wrapper interfaces via
            // authoritative-name fallback.
            let display_is_literal_value = display_is_literal_value;

            // Literal-value display pairs are not distinct nominal types; use
            // the regular TS2322 path instead of TS2719.
            let pair_is_literal_value =
                display_is_literal_value(&src_str) && display_is_literal_value(&tgt_str);
            let (message, code) = if src_str == tgt_str
                && !authoritative_names_differ
                && !pair_is_literal_value
            {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                        &[&src_str, &tgt_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                )
            } else {
                let source_generic_base = src_str.split_once('<').map(|(base, _)| base);
                let target_generic_base = tgt_str.split_once('<').map(|(base, _)| base);
                let preserve_generic_nominal_pair = src_str.contains('<')
                    && tgt_str.contains('<')
                    && authoritative_src == authoritative_tgt
                    && source_generic_base == target_generic_base
                    && authoritative_src.as_deref() == source_generic_base;
                let source_name = if src_str.starts_with("typeof ")
                    || src_str.starts_with("import(")
                    || src_str.starts_with('{')
                    || src_str.contains('<')
                    || source_is_direct_type_query_primitive
                    || preserve_generic_nominal_pair
                    || display_is_literal_value(&src_str)
                {
                    src_str.as_str()
                } else {
                    authoritative_src.as_deref().unwrap_or(&src_str)
                };
                let target_name = if tgt_str.starts_with("typeof ")
                    || tgt_str.starts_with("import(")
                    || tgt_str.starts_with('{')
                    || tgt_str.contains('<')
                    || preserve_generic_nominal_pair
                    || display_is_literal_value(&tgt_str)
                {
                    tgt_str.as_str()
                } else {
                    authoritative_tgt.as_deref().unwrap_or(&tgt_str)
                };
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[source_name, target_name],
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
}
