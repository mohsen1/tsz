//! Render `SubtypeFailureReason` values into diagnostics.
//! Split from `assignability.rs` for maintainability.
use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::type_checking_utilities as query_utils;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::assignability::{
    is_builtin_wrapper_name, is_object_prototype_method,
    is_object_prototype_method_for_array_target, is_primitive_type_name,
};
mod nested_application_property_mismatch;
mod type_mismatch;
impl<'a> CheckerState<'a> {
    /// Resolve the parameter name at `param_index` in the first call
    /// signature of `callable_ty` (if any). Used to render TS2328
    /// "Types of parameters '_' and '_' are incompatible." messages.
    fn callable_param_name_at(&self, callable_ty: TypeId, param_index: usize) -> Option<String> {
        let shape = crate::query_boundaries::common::get_callable_shape_for_type(
            self.ctx.types,
            callable_ty,
        )?;
        let atom = shape
            .call_signatures
            .first()
            .and_then(|sig| sig.params.get(param_index).and_then(|p| p.name))?;
        Some(self.ctx.types.resolve_atom(atom))
    }

    fn callable_type_after_display_evaluation(&mut self, ty: TypeId) -> Option<TypeId> {
        if crate::query_boundaries::common::is_callable_type(self.ctx.types, ty) {
            return Some(ty);
        }
        let evaluated = self.evaluate_type_with_resolution(ty);
        if evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated)
        {
            return Some(evaluated);
        }
        let evaluated = self.evaluate_type_for_assignability(ty);
        if evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated)
        {
            return Some(evaluated);
        }
        let evaluated = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
        (evaluated != TypeId::ERROR
            && crate::query_boundaries::common::is_callable_type(self.ctx.types, evaluated))
        .then_some(evaluated)
    }

    fn strict_callback_param_display_type(&mut self, ty: TypeId) -> TypeId {
        self.callable_type_after_display_evaluation(ty)
            .unwrap_or(ty)
    }

    fn strict_callback_outer_display_type(
        &mut self,
        ty: TypeId,
        param_index: usize,
    ) -> Option<TypeId> {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
            && param_index < shape.params.len()
        {
            let mut shape = (*shape).clone();
            shape.params[param_index].type_id =
                self.strict_callback_param_display_type(shape.params[param_index].type_id);
            return Some(self.ctx.types.factory().function(shape));
        }

        let shape = crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)?;
        if !shape.construct_signatures.is_empty()
            || shape.call_signatures.len() != 1
            || param_index >= shape.call_signatures[0].params.len()
        {
            return None;
        }
        let mut sig = shape.call_signatures[0].clone();
        sig.params[param_index].type_id =
            self.strict_callback_param_display_type(sig.params[param_index].type_id);
        Some(
            self.ctx
                .types
                .factory()
                .function(tsz_solver::FunctionShape {
                    type_params: sig.type_params,
                    params: sig.params,
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_predicate: sig.type_predicate,
                    is_constructor: false,
                    is_method: sig.is_method,
                }),
        )
    }

    fn strict_callback_assignment_display_pair(
        &mut self,
        source: TypeId,
        target: TypeId,
        param_index: usize,
    ) -> Option<(String, String)> {
        let source_display = self.strict_callback_outer_display_type(source, param_index)?;
        let target_display = self.strict_callback_outer_display_type(target, param_index)?;
        Some((
            self.format_assignability_type_for_message(source_display, target_display),
            self.format_assignability_type_for_message(target_display, source_display),
        ))
    }

    fn strict_callback_single_call_signature(
        &mut self,
        ty: TypeId,
    ) -> Option<tsz_solver::CallSignature> {
        let ty = self.callable_type_after_display_evaluation(ty)?;
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
        {
            return Some(tsz_solver::CallSignature {
                type_params: shape.type_params.clone(),
                params: shape.params.clone(),
                this_type: shape.this_type,
                return_type: shape.return_type,
                type_predicate: shape.type_predicate,
                is_method: shape.is_method,
            });
        }

        let shape = crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)?;
        if shape.construct_signatures.is_empty() && shape.call_signatures.len() == 1 {
            Some(shape.call_signatures[0].clone())
        } else {
            None
        }
    }

    fn strict_callback_inner_parameter_mismatch_exists(
        &mut self,
        source_param: TypeId,
        target_param: TypeId,
    ) -> bool {
        let Some(inner_source) = self.strict_callback_single_call_signature(target_param) else {
            return false;
        };
        let Some(inner_target) = self.strict_callback_single_call_signature(source_param) else {
            return false;
        };
        inner_source
            .params
            .iter()
            .zip(inner_target.params.iter())
            .any(|(source_param, target_param)| {
                !self.is_assignable_to(target_param.type_id, source_param.type_id)
            })
    }

    fn no_union_member_matches_switch_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        let expected_len = crate::query_boundaries::common::union_members(self.ctx.types, source)
            .map(|members| members.len())?;
        let target_members = crate::query_boundaries::common::union_members(self.ctx.types, target);
        if expected_len < 2 {
            return None;
        }

        let mut current = anchor_idx;
        let clause_idx = loop {
            let parent = self.ctx.arena.parent_of(current)?;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::CASE_CLAUSE {
                break parent;
            }
            current = parent;
        };

        let case_block_idx = self.ctx.arena.parent_of(clause_idx)?;
        let case_block_node = self.ctx.arena.get(case_block_idx)?;
        let case_block = self.ctx.arena.get_block(case_block_node)?;
        let clause_pos = case_block
            .statements
            .nodes
            .iter()
            .position(|&idx| idx == clause_idx)?;

        let mut start = clause_pos;
        while start > 0 {
            let prev_idx = case_block.statements.nodes[start - 1];
            let Some(prev_node) = self.ctx.arena.get(prev_idx) else {
                break;
            };
            let Some(prev_clause) = self.ctx.arena.get_case_clause(prev_node) else {
                break;
            };
            if !prev_clause.statements.nodes.is_empty() {
                break;
            }
            start -= 1;
        }

        let mut invalid = Vec::new();
        let mut valid = Vec::new();
        for &idx in &case_block.statements.nodes[start..=clause_pos] {
            let clause_node = self.ctx.arena.get(idx)?;
            let clause = self.ctx.arena.get_case_clause(clause_node)?;
            if clause.expression.is_none() {
                return None;
            }
            let case_type = self.literal_type_from_initializer(clause.expression)?;
            let display = self
                .literal_expression_display(clause.expression)
                .unwrap_or_else(|| self.format_assignability_type_for_message(case_type, target));
            let matches_target = case_type == target
                || target_members
                    .as_ref()
                    .is_some_and(|members| members.contains(&case_type));
            if matches_target {
                valid.push(display);
            } else {
                invalid.push(display);
            }
        }

        if invalid.len() + valid.len() != expected_len {
            return None;
        }

        invalid.extend(valid);
        Some(invalid.join(" | "))
    }

    fn format_tuple_shape_for_readonly_to_mutable(&mut self, type_id: TypeId) -> Option<String> {
        let elements = crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)?;
        let mut formatted = Vec::with_capacity(elements.len());
        for element in elements {
            let rest = if element.rest { "..." } else { "" };
            let optional = if element.optional && !element.rest {
                "?"
            } else {
                ""
            };
            let type_str = self.format_type_diagnostic(element.type_id);
            if let Some(name_atom) = element.name {
                let name = self.ctx.types.resolve_atom_ref(name_atom);
                formatted.push(format!("{rest}{name}{optional}: {type_str}"));
            } else {
                formatted.push(format!("{rest}{type_str}{optional}"));
            }
        }
        Some(format!("[{}]", formatted.join(", ")))
    }

    fn class_own_missing_properties_for_display(
        &self,
        source_candidates: &[TypeId],
        target_candidates: &[TypeId],
        missing_property_name: tsz_common::interner::Atom,
        fallback_target_type: TypeId,
    ) -> Option<(
        tsz_binder::SymbolId,
        TypeId,
        Vec<tsz_common::interner::Atom>,
    )> {
        let target_symbol = target_candidates
            .iter()
            .find_map(|&candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
                    .and_then(|shape| {
                        shape.properties.iter().find_map(|prop| {
                            (prop.name == missing_property_name)
                                .then_some(prop.parent_id)
                                .flatten()
                                .filter(|sym| {
                                    self.ctx.binder.get_symbol(*sym).is_some_and(|symbol| {
                                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                                    })
                                })
                        })
                    })
            })
            .or_else(|| {
                target_candidates.iter().find_map(|&candidate| {
                    crate::query_boundaries::common::get_object_symbol(self.ctx.types, candidate)
                        .or_else(|| {
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                candidate,
                            )
                            .and_then(|shape| {
                                shape.properties.iter().find_map(|prop| {
                                    prop.parent_id.filter(|sym| {
                                        self.ctx.binder.get_symbol(*sym).is_some_and(|symbol| {
                                            symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                                        })
                                    })
                                })
                            })
                        })
                })
            })?;

        let mut source_props = Vec::new();
        for &candidate in source_candidates {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            {
                for prop in &shape.properties {
                    if !source_props.contains(&prop.name) {
                        source_props.push(prop.name);
                    }
                }
            }
        }

        let mut class_own_missing = Vec::new();
        let mut target_display_type = None;
        for &candidate in target_candidates {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, candidate)
            {
                let mut saw_own = false;
                for prop in &shape.properties {
                    if prop.parent_id == Some(target_symbol) {
                        saw_own = true;
                        let name = self.ctx.types.resolve_atom_ref(prop.name);
                        if !tsz_solver::utils::is_synthetic_private_brand_name(&name)
                            && !is_object_prototype_method(&name)
                            && !source_props.contains(&prop.name)
                            && !class_own_missing.contains(&prop.name)
                        {
                            class_own_missing.push(prop.name);
                        }
                    }
                }
                if saw_own && target_display_type.is_none() {
                    target_display_type = Some(candidate);
                }
            }
        }

        (class_own_missing.len() > 1).then(|| {
            (
                target_symbol,
                target_display_type.unwrap_or(fallback_target_type),
                class_own_missing,
            )
        })
    }

    /// Recursively render a `SubtypeFailureReason` into a Diagnostic.
    pub(crate) fn render_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        use crate::query_boundaries::common::SubtypeFailureReason;

        let source = self.recover_unknown_array_source_type_for_display(source, idx, depth);
        let (start, length) = self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                // get_node_span returns (pos, end); convert to (start, length)
                // and apply the same span normalization as the primary path.
                let (pos, end) = self.get_node_span(idx).unwrap_or((0, 0));
                self.normalized_anchor_span(idx, pos, end.saturating_sub(pos))
            });
        let file_name = self.ctx.file_name.clone();
        // TS2696: property-only failures from the `Object` wrapper use the
        // specialized message unless the target is callable/constructable.
        if depth == 0 {
            let is_property_failure = matches!(
                reason,
                SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
                    | SubtypeFailureReason::PropertyTypeMismatch { .. }
                    | SubtypeFailureReason::OptionalPropertyRequired { .. }
                    | SubtypeFailureReason::NoCommonProperties { .. }
            );
            if is_property_failure {
                let src_str = self.format_type_diagnostic(source);
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
        }
        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => self.render_missing_property(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *property_name,
                *source_type,
                *target_type,
            ),
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => self.render_missing_properties(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                property_names,
                *source_type,
                *target_type,
            ),
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => self.render_property_type_mismatch(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *property_name,
                *source_property_type,
                *target_property_type,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::OptionalPropertyRequired { property_name } => self
                .render_optional_property_required(
                    reason,
                    source,
                    target,
                    idx,
                    depth,
                    start,
                    length,
                    file_name,
                    *property_name,
                ),
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
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let base = self.property_visibility_assignability_message(
                    &source_str,
                    &target_str,
                    &prop_name,
                    *source_visibility,
                    *target_visibility,
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
            SubtypeFailureReason::PropertyNominalMismatch { property_name } => self
                .render_property_nominal_mismatch(
                    reason,
                    source,
                    target,
                    idx,
                    depth,
                    start,
                    length,
                    file_name,
                    *property_name,
                ),
            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type: _,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let (code, message) =
                    self.excess_property_diagnostic_message(&prop_name, target, idx);
                let (excess_start, excess_length) = self
                    .find_excess_property_anchor(idx, *property_name)
                    .unwrap_or((start, length));
                Diagnostic::error(file_name, excess_start, excess_length, message, code)
            }
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => self.render_return_type_mismatch(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *source_return,
                *target_return,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::TooManyParameters {
                source_count,
                target_count,
            } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
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
                let elaboration = format_message(
                    diagnostic_messages::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT,
                    &[&source_count.to_string(), &target_count.to_string()],
                );
                diag.related_information.push(DiagnosticRelatedInformation {
                    file: file_name,
                    start,
                    length,
                    message_text: elaboration,
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT,
                });
                diag
            }
            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
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
                        self.format_top_level_assignability_message_types_at(source, target, idx);
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
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
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
                if depth == 0 {
                    let source_str = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
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

            SubtypeFailureReason::MissingIndexSignature { index_kind } => {
                if depth == 0 {
                    let source_str = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
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
                let source_str = self.format_type_diagnostic(source);
                let message = format_message(
                    diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                    &[index_kind, &source_str],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let display_source = if depth == 0 { source } else { *source_type };
                let (mut source_str, mut target_str) = if depth == 0 {
                    let use_structural_source_display =
                        crate::query_boundaries::common::enum_def_id(
                            self.ctx.types,
                            display_source,
                        )
                        .is_none();
                    (
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                display_source,
                                DiagnosticTypeDisplayRole::AssignmentSource {
                                    target,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_assignability_type_for_message(display_source, target)
                        },
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                target,
                                DiagnosticTypeDisplayRole::AssignmentTarget {
                                    source: display_source,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_assignability_type_for_message(target, display_source)
                        },
                    )
                } else {
                    (
                        self.format_type_diagnostic(display_source),
                        self.format_type_diagnostic(target),
                    )
                };
                if source_str == "unknown" && source != TypeId::UNKNOWN {
                    let fallback =
                        self.format_assignability_type_for_message(display_source, target);
                    if fallback != "unknown" {
                        source_str = fallback;
                    }
                }
                if depth == 0
                    && let Some(switch_display) =
                        self.no_union_member_matches_switch_source_display(source, target, idx)
                {
                    source_str = switch_display;
                }
                if let Some(display) = self
                    .object_literal_property_literal_union_alias_target_display(
                        target,
                        &target_str,
                        idx,
                    )
                {
                    target_str = display;
                }
                let evaluated_target_for_suggestion = self.evaluate_type_with_env(target);
                if let Some(suggestion) = self.find_string_literal_spelling_suggestion(
                    source,
                    evaluated_target_for_suggestion,
                ) {
                    let display_target_str = self.format_ts2820_target_display(
                        target,
                        evaluated_target_for_suggestion,
                        &target_str,
                    );
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                        &[&source_str, &display_target_str, &suggestion],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        msg,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                    );
                }
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
                // Use the unwidened source: tsc preserves literal spellings in
                // "has no properties in common" messages.
                let mut source_str =
                    if (crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        source,
                    ) || crate::query_boundaries::common::has_construct_signatures(
                        self.ctx.types,
                        source,
                    )) && depth == 0
                    {
                        let widened_source = self.widen_type_for_display(source);
                        let widened_source = self.widen_function_like_display_type(widened_source);
                        self.format_type_for_assignability_message(widened_source)
                    } else {
                        self.format_type_diagnostic(source)
                    };
                let target_str = self.format_type_for_assignability_message(target);

                // If calling the source would fix the mismatch, emit TS2560 instead.
                let (msg_template, code) = if self
                    .should_suggest_calling_for_weak_type(source, target)
                {
                    (
                            diagnostic_messages::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                            diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                        )
                } else {
                    (
                        diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                        diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    )
                };
                if code
                    == diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT
                {
                    source_str = Self::widen_member_literals_in_display_text(&source_str);
                }
                let (source_str, target_str) = self
                    .finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
                let message = format_message(msg_template, &[&source_str, &target_str]);
                Diagnostic::error(file_name, start, length, message, code)
            }

            SubtypeFailureReason::TypeMismatch {
                source_type: _,
                target_type: _,
            } => self
                .render_type_mismatch(reason, source, target, idx, depth, start, length, file_name),

            SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            } => {
                let source_str =
                    if let Some(inner) = crate::query_boundaries::common::readonly_inner_type(
                        self.ctx.types,
                        *source_type,
                    ) && let Some(tuple_display) =
                        self.format_tuple_shape_for_readonly_to_mutable(inner)
                    {
                        format!("readonly {tuple_display}")
                    } else {
                        self.format_type_diagnostic(*source_type)
                    };
                let target_str = self
                    .format_tuple_shape_for_readonly_to_mutable(*target_type)
                    .unwrap_or_else(|| self.format_type_diagnostic(*target_type));
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

            SubtypeFailureReason::ParameterTypeMismatch {
                param_index,
                source_param,
                target_param,
                inner_reason,
            } => {
                // For top-level direct-callable mismatches whose param types
                // are themselves callable and non-generic, tsc treats the
                // inner contravariant comparison as a callback. When that
                // inner check fails on the callback's RETURN type, tsc
                // suppresses the outer "Type X is not assignable to Y"
                // (TS2322) wrapper and reports the diagnostic directly with
                // code TS2328 ("Types of parameters '_' and '_' are
                // incompatible.") — see checker.ts `reportErrorResults`,
                // which honours `overrideNextErrorInfo` bumped by the
                // elided `Call_signature_return_types_0_and_1_are_incompatible`
                // (TS2202) report. When the inner failure is on a
                // PARAMETER, no elision happens and tsc keeps the TS2322
                // wrapper.
                let contains_type_params = |ty| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                };
                let strict_callback_case = if depth == 0 {
                    let source_callable = self.callable_type_after_display_evaluation(source);
                    let target_callable = self.callable_type_after_display_evaluation(target);
                    let source_param_callable =
                        self.callable_type_after_display_evaluation(*source_param);
                    let target_param_callable =
                        self.callable_type_after_display_evaluation(*target_param);
                    source_callable.is_some()
                        && target_callable.is_some()
                        && source_param_callable.is_some()
                        && target_param_callable.is_some()
                        && !contains_type_params(source_param_callable.unwrap_or(*source_param))
                        && !contains_type_params(target_param_callable.unwrap_or(*target_param))
                } else {
                    false
                };
                let inner_failed_on_return = matches!(
                    inner_reason.as_deref(),
                    Some(SubtypeFailureReason::ReturnTypeMismatch { .. })
                );
                let inner_param_mismatch_exists = inner_failed_on_return
                    && self.strict_callback_inner_parameter_mismatch_exists(
                        *source_param,
                        *target_param,
                    );

                if strict_callback_case && inner_failed_on_return && !inner_param_mismatch_exists {
                    let source_name = self
                        .callable_param_name_at(source, *param_index)
                        .unwrap_or_else(|| format!("arg{param_index}"));
                    let target_name = self
                        .callable_param_name_at(target, *param_index)
                        .unwrap_or_else(|| format!("arg{param_index}"));
                    let ts2328_message = format_message(
                        diagnostic_messages::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
                        &[&source_name, &target_name],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        ts2328_message,
                        diagnostic_codes::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
                    )
                } else {
                    let (source_str, target_str) = if strict_callback_case {
                        self.strict_callback_assignment_display_pair(source, target, *param_index)
                            .unwrap_or_else(|| {
                                (
                                    self.format_type_for_diagnostic_role(
                                        source,
                                        DiagnosticTypeDisplayRole::AssignmentSource {
                                            target,
                                            anchor_idx: idx,
                                        },
                                    ),
                                    self.format_assignability_type_for_message(target, source),
                                )
                            })
                    } else {
                        (
                            self.format_type_for_diagnostic_role(
                                source,
                                DiagnosticTypeDisplayRole::AssignmentSource {
                                    target,
                                    anchor_idx: idx,
                                },
                            ),
                            self.format_assignability_type_for_message(target, source),
                        )
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
            }

            _ => {
                let source_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
                let mut target_str = self.format_assignability_type_for_message(target, source);
                if let Some(display) = self
                    .object_literal_property_literal_union_alias_target_display(
                        target,
                        &target_str,
                        idx,
                    )
                {
                    target_str = display;
                }
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

    fn object_literal_property_literal_union_alias_target_display(
        &mut self,
        target: TypeId,
        current_display: &str,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if current_display.contains(" | ")
            || !self.anchor_is_within_object_literal_property(anchor_idx)
        {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(target);
        let display_target =
            if crate::query_boundaries::common::union_members(self.ctx.types, target).is_some() {
                target
            } else {
                evaluated
            };
        let members =
            crate::query_boundaries::common::union_members(self.ctx.types, display_target)?;
        if members.len() < 2
            || !members.iter().all(|&member| {
                crate::query_boundaries::common::literal_value(self.ctx.types, member).is_some()
                    || member == TypeId::BOOLEAN_TRUE
                    || member == TypeId::BOOLEAN_FALSE
            })
        {
            return None;
        }

        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        Some(
            members
                .iter()
                .map(|&member| formatter.format(member).into_owned())
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    fn anchor_is_within_object_literal_property(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        for _ in 0..12 {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            ) {
                return self
                    .ctx
                    .arena
                    .get_extended(current)
                    .and_then(|ext| self.ctx.arena.get(ext.parent))
                    .is_some_and(|parent| {
                        parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    });
            }
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
            ) {
                return false;
            }
            let Some(parent) = self.ctx.arena.get_extended(current).map(|ext| ext.parent) else {
                return false;
            };
            if parent.is_none() {
                return false;
            }
            current = parent;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn render_missing_property(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        let source_type_is_object = self.is_object_intrinsic_for_missing_properties(source_type);
        // Primitive sources use TS2322 rather than missing-property wording.
        let display_src_str = if depth == 0 && !source_type_is_object {
            self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            )
        } else {
            self.format_type_diagnostic(source_type)
        };
        // Distinguish "outer source is primitive" (e.g. `let y: Foo = 42`) from
        // "inner source_type is primitive" (e.g. assigning `{ one: number }` to
        // `{ [k: string]: Foo }`, where the solver reports `MissingProperty(foo,
        // src_ty=number, tgt_ty=Foo)` describing the failed nested check). In
        // the first case we want the primitive-vs-target message; in the second
        // we want the OUTER source/target shown, not the inner property types.
        let outer_source_is_primitive =
            crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
                || is_primitive_type_name(&display_src_str);
        let inner_source_type_is_primitive = !source_type_is_object
            && crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type);
        let is_source_primitive =
            outer_source_is_primitive || (depth > 0 && inner_source_type_is_primitive);
        if is_source_primitive {
            let tgt_str = self.recursive_non_generic_alias_body_name(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&display_src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Pure function sources against non-callable targets use TS2322; class
        // constructors still keep the missing-property path.
        if self.should_suppress_missing_property_for_callable_source(source, source_type, target) {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
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

        // TSC emits TS2322 instead of TS2741 when the target has an index signature but the
        // missing property is not a direct named property of the target. In this case, the
        // "missing" property comes from the index signature value type, not from a required
        // named property, so the generic assignability error is more appropriate.
        // Skip this check for array/tuple targets: their properties (like `length`) come
        // from the Array interface and ARE named properties even though the array also has
        // a numeric index signature.
        {
            use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
            let resolver = IndexSignatureResolver::new(self.ctx.types);
            let target_is_array_or_tuple =
                crate::query_boundaries::common::array_element_type(self.ctx.types, target)
                    .is_some()
                    || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target)
                        .is_some();
            let target_has_index = !target_is_array_or_tuple
                && (resolver.has_index_signature(target, IndexKind::String)
                    || resolver.has_index_signature(target, IndexKind::Number));
            if target_has_index {
                let prop_name_str = self.ctx.types.resolve_atom_ref(property_name);
                let target_has_named_prop = crate::query_boundaries::common::find_property_by_str(
                    self.ctx.types,
                    target,
                    &prop_name_str,
                )
                .is_some();
                if !target_has_named_prop {
                    let src_str = if depth == 0 {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource {
                                target,
                                anchor_idx: idx,
                            },
                        )
                    } else {
                        self.format_type_diagnostic(source_type)
                    };
                    let tgt_str = if depth == 0 {
                        self.format_assignability_type_for_message(target, source)
                    } else {
                        self.format_type_diagnostic(target_type)
                    };
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
            }
        }

        // TSC emits TS2322 instead of TS2741 when both source and target have index signatures.
        // For index signature to index signature assignments, the more general assignability error
        // is preferred over specific missing property errors.
        // Skip for array/tuple targets — their numeric index is implicit and missing named
        // properties (like `length`) should still produce TS2741.
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
        let resolver = IndexSignatureResolver::new(self.ctx.types);
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let target_is_array_or_tuple_for_idx =
            crate::query_boundaries::common::array_element_type(self.ctx.types, target).is_some()
                || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target).is_some();
        let source_has_index = [source, source_evaluated].iter().any(|t| {
            resolver.has_index_signature(*t, IndexKind::String)
                || resolver.has_index_signature(*t, IndexKind::Number)
        });
        let target_has_index = !target_is_array_or_tuple_for_idx
            && [target, target_evaluated].iter().any(|t| {
                resolver.has_index_signature(*t, IndexKind::String)
                    || resolver.has_index_signature(*t, IndexKind::Number)
            });
        if source_has_index && target_has_index {
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

        // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
        let tgt_str = self.format_type_diagnostic(target_type);
        let original_tgt_str = self.format_type_diagnostic(target);
        if is_builtin_wrapper_name(&tgt_str) || is_builtin_wrapper_name(&original_tgt_str) {
            let src_str = self.format_type_diagnostic(source_type);
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

        // TSC emits TS2322 instead of TS2741 when the target type is an intersection type.
        let target_evaluated_for_intersection = self.evaluate_type_with_env(target);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            )
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = if crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            ) {
                self.format_type_diagnostic(target_evaluated_for_intersection)
            } else if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            {
                self.format_type_diagnostic(target)
            } else {
                self.format_type_diagnostic(target_type)
            };
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

        // TSC emits TS2322 instead of TS2741 when the *source* type is an intersection.
        // This covers type aliases like `LinkedList<T> = T & { next: ... }` that may have
        // been evaluated to an intersection by the time we reach diagnostic rendering.
        // Check both the type data and the source's declaration annotation, since
        // intersections may be flattened into Object types by the solver.
        let source_evaluated_for_intersection = self.evaluate_type_with_env(source);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, source)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                source_evaluated_for_intersection,
            )
            || (depth == 0 && self.anchor_source_has_intersection_annotation(idx))
        {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
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

        // TSC emits TS2322 instead of TS2741 when the source is a type application
        // (generic type alias) whose base type resolves to an intersection. For example,
        // `LinkedList<Entity>` where `type LinkedList<T> = T & { next: LinkedList<T> }`.
        // Named type aliases expanding to intersections are reported as general
        // assignability failures, not property-level "missing" errors.
        if let Some((base, _args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, source)
        {
            let base_eval = self.evaluate_type_with_env(base);
            let base_is_intersection =
                crate::query_boundaries::common::is_intersection_type(self.ctx.types, base)
                    || crate::query_boundaries::common::is_intersection_type(
                        self.ctx.types,
                        base_eval,
                    );
            if base_is_intersection {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
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
        }

        // Private brand properties handling
        let prop_name = self.ctx.types.resolve_atom_ref(property_name).to_string();
        if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_for_assignability_message(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_for_assignability_message(target_type)
            };
            // Try to find the backing private/protected member for a detailed message.
            if depth == 0
                && let Some((member_name, owner_name, visibility)) =
                    self.private_or_protected_member_missing_display(source, target, None)
            {
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &member_name,
                    &owner_name,
                    visibility,
                    None,
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            // Source HAS the property but with wrong visibility/nominal identity.
            if depth == 0
                && let Some((display_prop, owner_name, visibility)) =
                    self.private_or_protected_brand_backing_member_display(target, None)
            {
                let message = self.private_or_protected_assignability_message(
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
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
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

        // TSC emits TS2322 instead of TS2741 when the target is an intersection type.
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
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

        // TSC emits TS2322 when the target's declared type annotation is an intersection type.
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

        // Object.prototype methods → emit TS2322 instead of TS2741.
        if is_object_prototype_method(&prop_name) {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
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

        // When the source has an index signature, upgrade TS2741 → TS2739 when needed.
        if depth == 0
            && let Some(all_missing) =
                self.missing_required_properties_from_index_signature_source(source, target)
            && all_missing.len() > 1
        {
            // For TS2739 source display, when the source is a non-generic
            // type alias whose body is a generic Application
            // (`type B = A<X1, X2, ...>`), tsc unfolds one level to display
            // the application form `A<X1, X2, ...>` rather than the wrapper
            // alias name `B`. See `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
            // line 91. Falls through to the role formatter for any other shape.
            let src_str = if let Some(display) =
                self.ts2739_alias_of_application_source_display_text(source)
            {
                display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            };
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let prop_list: Vec<String> = all_missing
                .iter()
                .take(4)
                .map(|name| self.missing_property_name_for_display(*name, target))
                .collect();
            let props_joined = prop_list.join(", ");
            let (message, code) = if all_missing.len() > 4 {
                let more_count = (all_missing.len() - 4).to_string();
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
            return Diagnostic::error(file_name, start, length, message, code);
        }

        if depth == 0 {
            let source_resolved = self.resolve_type_for_property_access(source_type);
            let source_evaluated = self.evaluate_type_for_assignability(source_type);
            let target_resolved = self.resolve_type_for_property_access(target_type);
            let target_evaluated = self.evaluate_type_for_assignability(target_type);
            let source_candidates = [source_type, source, source_resolved, source_evaluated];
            let target_candidates = [target_type, target, target_resolved, target_evaluated];
            if let Some((target_symbol, target_display_type, class_own_missing)) = self
                .class_own_missing_properties_for_display(
                    &source_candidates,
                    &target_candidates,
                    property_name,
                    target_type,
                )
            {
                let src_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
                let tgt_str = self
                    .ctx
                    .binder
                    .get_symbol(target_symbol)
                    .map(|symbol| symbol.escaped_name.to_string())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_display_type));
                let ordered_names = self.sort_missing_property_names_for_display(
                    target_display_type,
                    &class_own_missing,
                );
                let prop_list: Vec<String> = ordered_names
                    .iter()
                    .take(5)
                    .map(|name| self.missing_property_name_for_display(*name, target))
                    .collect();
                let props_joined = prop_list.join(", ");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                );
            }
        }

        // TS2741: Property 'x' is missing in type 'A' but required in type 'B'.
        let widened_source = self.widen_type_for_display(source_type);
        let (mut src_str, mut tgt_str_qualified) = if depth == 0 {
            let src = if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else if let Some(base_display) =
                self.private_identifier_missing_source_base_display(source, property_name)
            {
                base_display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            };
            let widened_target = self.widen_fresh_object_literal_properties_for_display(target);
            (
                src,
                self.format_type_for_diagnostic_role(
                    widened_target,
                    DiagnosticTypeDisplayRole::FlattenedDiagnostic,
                ),
            )
        } else if source_type == TypeId::OBJECT {
            ("{}".to_string(), tgt_str)
        } else {
            self.format_type_pair_diagnostic(widened_source, target)
        };
        // When source and target collapse to the same short name (e.g. two
        // same-named classes from different modules), re-qualify them so the
        // reader can tell them apart. The formatter's pair-disambiguation
        // path adds namespace or `import("<specifier>")` prefixes only when
        // the bare names collide.
        //
        // Two cases:
        //   1. `src_str == tgt_str_qualified`: both formatted to the same
        //      short name — disambiguate both sides.
        //   2. `src_str` was already qualified by expression text (e.g.
        //      `N.A` from `new N.A()`) but the underlying source and target
        //      types still share a bare formatted name (e.g. both "A").
        //      Keep the source text as-is and only qualify the target.
        if widened_source != target {
            if src_str == tgt_str_qualified {
                let (da, db) = self.format_type_pair_diagnostic(widened_source, target);
                src_str = da;
                tgt_str_qualified = db;
            } else {
                let fmt_src_bare = self.format_type_diagnostic(widened_source);
                let fmt_tgt_bare = self.format_type_diagnostic(target);
                if fmt_src_bare == fmt_tgt_bare {
                    let (_, db) = self.format_type_pair_diagnostic(widened_source, target);
                    if db != tgt_str_qualified {
                        tgt_str_qualified = db;
                    }
                }
            }
        }
        let prop_name_display = self.missing_property_name_for_display(property_name, target);
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[&prop_name_display, &src_str, &tgt_str_qualified],
        );
        Diagnostic::error(
            file_name,
            start,
            length,
            message,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// For TS2739 source display, unfold wrapper aliases like
    /// `type B = A<X>` to the body application `A<X>`. Other shapes keep
    /// normal formatting.
    pub(in crate::error_reporter) fn ts2739_alias_of_application_source_display(
        &self,
        source: TypeId,
    ) -> Option<TypeId> {
        // The source can reach this point either as:
        // - `Lazy(DefId)` when an unevaluated alias reference,
        // - the already-evaluated structural form (find_def_for_type points
        //   back at the alias's definition),
        // - or an `Application(Lazy(DefId), [args...])` when generic.
        let source_application =
            crate::query_boundaries::common::application_info(self.ctx.types, source).or_else(
                || {
                    let alias = self.ctx.types.get_display_alias(source)?;
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                },
            );

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source)
            .or_else(|| self.ctx.definition_store.find_def_for_type(source))
            .or_else(|| {
                // Application path: peek at the application's base to find
                // the alias's def_id.
                let (base, _) = source_application.as_ref()?;
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, *base)
            })?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        if def.type_params.is_empty() {
            // Recover the as-written application via display_alias for
            // evaluated sources, or via the alias body for lazy references.
            let app_origin = self
                .ctx
                .types
                .get_display_alias(source)
                .filter(|&alias| {
                    crate::query_boundaries::common::application_id(self.ctx.types, alias).is_some()
                })
                .or(def.body)?;
            let app_id =
                crate::query_boundaries::common::application_id(self.ctx.types, app_origin)?;
            let app = self.ctx.types.type_application(app_id);
            if app.args.is_empty() {
                return None;
            }
            let app_base_def_id =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            if !self
                .ctx
                .definition_store
                .get(app_base_def_id)
                .is_some_and(|def| {
                    matches!(
                        def.kind,
                        tsz_solver::def::DefKind::TypeAlias | tsz_solver::def::DefKind::Interface
                    )
                })
            {
                return None;
            }
            return Some(app_origin);
        }

        // Generic wrapper alias path: `type IndirectArrayish<U extends ...> =
        // Objectish<U>;` — when source is `IndirectArrayish<any>` and the
        // body is itself an `Application` of a different named alias, tsc
        // unfolds one level to display `Objectish<any>` (the body alias's
        // application form with the wrapper's type-args substituted into the
        // body's slots). See `compiler/mappedTypeWithAny.ts` line 47 — tsc
        // displays `Objectish<any>` for `arr = indirectArrayish` rather than
        // the wrapper name `IndirectArrayish<any>`.
        let body = def.body?;
        let body_app_id = crate::query_boundaries::common::application_id(self.ctx.types, body)?;
        let body_app = self.ctx.types.type_application(body_app_id);
        // Body alias must be different from the wrapper itself (avoid loops).
        let body_def_id =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, body_app.base)?;
        if body_def_id == def_id {
            return None;
        }
        // Substitute the wrapper's type-params with the source application's
        // args so the displayed application reflects the call-site instantiation.
        let (_, source_args) = source_application?;
        if source_args.len() != def.type_params.len() {
            return None;
        }
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &source_args,
        );
        let body_args: Vec<TypeId> = body_app
            .args
            .iter()
            .map(|&arg| {
                crate::query_boundaries::common::instantiate_type_preserving_meta(
                    self.ctx.types,
                    arg,
                    &subst,
                )
            })
            .collect();
        Some(
            self.ctx
                .types
                .factory()
                .application(body_app.base, body_args),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_missing_properties(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_names: &[tsz_common::interner::Atom],
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        let source_type_is_object = self.is_object_intrinsic_for_missing_properties(source_type);
        // TSC emits TS2322 instead of TS2739/TS2740 when the source is a primitive type.
        if !source_type_is_object
            && crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type)
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.recursive_non_generic_alias_body_name(target_type);
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

        // TSC emits TS2322 instead of TS2739/TS2740/TS2741 when the source has call
        // signatures (pure function type, NOT class constructor) and the target does NOT
        // have call signatures. Class constructors (with construct-only signatures) should
        // still produce TS2741 for missing properties.
        {
            if self.should_suppress_missing_property_for_callable_source(
                source,
                source_type,
                target,
            ) {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
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
        }

        // Note: TS2696 for `Object` source is handled at the top of render_failure_reason.

        // Emit TS2322 instead of TS2739/TS2740 when the SOURCE is a wrapper-like built-in.
        let src_str_check = self.format_type_diagnostic(source_type);
        let original_src_check = self.format_type_diagnostic(source);
        if is_builtin_wrapper_name(&src_str_check) || is_builtin_wrapper_name(&original_src_check) {
            let display_src = if is_builtin_wrapper_name(&original_src_check) {
                &original_src_check
            } else {
                &src_str_check
            };
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[display_src, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740 when the target is an intersection type.
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            || self.anchor_jsdoc_type_tag_targets_intersection_alias(idx)
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

        // TSC emits TS2322 instead of TS2739/TS2740 when both source and target have
        // string index signatures. For number index signatures, suppress only when the
        // target has no explicit named properties (i.e., it's purely an index-signature
        // type like `{ [x: number]: T }`). Named interfaces that happen to have number
        // index signatures (like String, Array) should still get TS2739/TS2740.
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
        let resolver = IndexSignatureResolver::new(self.ctx.types);
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let source_has_string_index = [source, source_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::String));
        let target_has_string_index = [target, target_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::String));
        let source_has_number_index = [source, source_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::Number));
        let target_has_number_index = [target, target_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::Number));
        // For number index signatures, only suppress when the missing properties are
        // NOT explicitly declared on the target (they came from index value type expansion).
        // We detect this by checking if none of the missing property names match a real
        // named member of the target type's object shape.
        let number_index_suppress =
            source_has_number_index && target_has_number_index && !property_names.is_empty() && {
                let target_shape = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    target_type,
                );
                property_names.iter().all(|name| {
                    // If none of the missing properties are real named members of the
                    // target type, the "missing properties" came from index value type
                    // comparison, not from actual missing named members.
                    match &target_shape {
                        Some(shape) => !shape.properties.iter().any(|p| p.name == *name),
                        None => true,
                    }
                })
            };
        // When the target is an array/tuple type, the missing properties (length, push,
        // pop, etc.) are real named members, not artifacts of index signature comparison.
        // Don't suppress TS2739/TS2740 in that case — tsc correctly emits them.
        let is_array_target = matches!(
            query_utils::classify_array_like(self.ctx.types, target_type),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        );
        if !is_array_target
            && ((source_has_string_index && target_has_string_index) || number_index_suppress)
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
        let _has_non_proto_missing = property_names.iter().any(|name| {
            let s = self.ctx.types.resolve_atom_ref(*name);
            !tsz_solver::utils::is_synthetic_private_brand_name(&s)
                && if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
        });
        let filtered_names: Vec<_> = property_names
            .iter()
            .filter(|name| {
                let s = self.ctx.types.resolve_atom_ref(**name);
                if tsz_solver::utils::is_synthetic_private_brand_name(&s) {
                    return false;
                }
                if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
            })
            .copied()
            .collect();

        // If all missing properties are numeric indices, emit TS2322.
        let all_numeric = !filtered_names.is_empty()
            && filtered_names.iter().all(|name| {
                let s = self.ctx.types.resolve_atom_ref(*name);
                s.parse::<usize>().is_ok()
            });

        if all_numeric {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
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

        // If all missing properties were private brands, emit TS2322 instead.
        if filtered_names.is_empty() {
            if let Some((prop_name, owner_name, visibility)) =
                self.private_or_protected_member_missing_display(source_type, target_type, None)
            {
                let widened_source = self.widen_type_for_display(source_type);
                let src_str = if source_type_is_object {
                    "{}".to_string()
                } else {
                    self.format_type_diagnostic(widened_source)
                };
                let tgt_str = self.format_type_diagnostic(target_type);
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &prop_name,
                    &owner_name,
                    visibility,
                    None,
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            let src_str = if depth == 0 {
                if source_type_is_object {
                    "{}".to_string()
                } else {
                    let source_display = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    self.rewrite_source_display_for_non_literal_target_assignability(
                        source,
                        target,
                        source_display,
                    )
                }
            } else if source_type_is_object {
                "{}".to_string()
            } else {
                let widened_source = self.widen_type_for_display(source_type);
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
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

        if filtered_names.len() == 1 {
            let source_resolved = self.resolve_type_for_property_access(source_type);
            let source_evaluated = self.evaluate_type_for_assignability(source_type);
            let target_resolved = self.resolve_type_for_property_access(target_type);
            let target_evaluated = self.evaluate_type_for_assignability(target_type);
            let source_candidates = [source_type, source, source_resolved, source_evaluated];
            let target_candidates = [target_type, target, target_resolved, target_evaluated];
            if let Some((target_symbol, target_display_type, class_own_missing)) = self
                .class_own_missing_properties_for_display(
                    &source_candidates,
                    &target_candidates,
                    filtered_names[0],
                    target_type,
                )
            {
                let src_str = if depth == 0 {
                    if source_type_is_object {
                        "{}".to_string()
                    } else {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource {
                                target,
                                anchor_idx: idx,
                            },
                        )
                    }
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = self
                    .ctx
                    .binder
                    .get_symbol(target_symbol)
                    .map(|symbol| symbol.escaped_name.to_string())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_display_type));
                let ordered_names = self.sort_missing_property_names_for_display(
                    target_display_type,
                    &class_own_missing,
                );
                let prop_list: Vec<String> = ordered_names
                    .iter()
                    .take(5)
                    .map(|name| self.missing_property_name_for_display(*name, target))
                    .collect();
                let props_joined = prop_list.join(", ");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                );
            }
        }

        // When filtering removed brand/prototype properties and only 1 remains, emit TS2741.
        if filtered_names.len() == 1 {
            let prop_name = self
                .ctx
                .types
                .resolve_atom_ref(filtered_names[0])
                .to_string();

            // When the source is a function/callable type and the remaining property is
            // private or protected, the function fundamentally can't satisfy the class's
            // nominal brand requirement. TSC emits TS2322 (general mismatch) here, not
            // TS2741 (missing property). For class-to-class assignments, TSC keeps TS2741.
            let source_is_function =
                crate::query_boundaries::common::is_function_type(self.ctx.types, source)
                    || crate::query_boundaries::common::is_function_type(
                        self.ctx.types,
                        source_type,
                    );
            if source_is_function
                && let Some(prop_info) =
                    self.property_info_for_display(target_type, filtered_names[0])
                && prop_info.visibility != tsz_solver::Visibility::Public
            {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
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

            let src_str = if depth == 0 {
                if source_type_is_object {
                    "{}".to_string()
                } else if let Some(base_display) =
                    self.private_identifier_missing_source_base_display(source, filtered_names[0])
                {
                    base_display
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                }
            } else if source_type_is_object {
                "{}".to_string()
            } else {
                let widened_source = self.widen_type_for_display(source_type);
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = self
                .property_declaring_type_name(target_type, filtered_names[0])
                .unwrap_or_else(|| {
                    if depth == 0 {
                        self.format_assignability_type_for_message(target, source)
                    } else {
                        self.format_type_diagnostic(target_type)
                    }
                });
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        // TS2739/TS2740: Type 'A' is missing the following properties from type 'B': x, y, z
        let display_source = if self
            .missing_required_properties_from_index_signature_source(source_type, target_type)
            .is_some()
        {
            self.evaluate_type_for_assignability(source_type)
        } else {
            source_type
        };
        let src_str = if depth == 0 {
            // For TS2739, when the source is a non-generic type alias whose
            // body is a generic Application (`type B = A<X1, X2, ...>`),
            // tsc unfolds one level to display the application form
            // `A<X1, X2, ...>` rather than the wrapper alias name `B`.
            // The application form names both the underlying generic and its
            // type arguments, which is the structural information the
            // "is missing the following properties" message is meant to
            // expose. tsc preserves alias names in TS2322 (target context)
            // and TS2339 (receiver), so this unfold is scoped to TS2739
            // source rendering. See
            // `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
            // line 91, where `type NumberToNumber = NumberTo<number>` is
            // displayed as `NumberTo<number>` in the missing-properties source.
            if source_type_is_object {
                "{}".to_string()
            } else if let Some(display) =
                self.ts2739_alias_of_application_source_display_text(source)
            {
                display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            }
        } else {
            self.format_type_diagnostic(self.widen_type_for_display(display_source))
        };
        let tgt_str = if depth == 0 {
            self.format_assignability_type_for_message(target, source)
        } else {
            self.format_type_diagnostic(target_type)
        };
        let ordered_names =
            self.sort_missing_property_names_for_display(target_type, &filtered_names);
        // tsc lists up to 5 properties inline (TS2739), and uses "and N more"
        // truncation (TS2740) when there are 6+. For TS2740, tsc lists the
        // first 4 properties then "and N more" (where N = total - 4).
        let is_truncated = ordered_names.len() > 5;
        let display_count = if is_truncated { 4 } else { 5 };
        let prop_list: Vec<String> = ordered_names
            .iter()
            .take(display_count)
            .map(|name| self.missing_property_name_for_display(*name, target))
            .collect();
        let props_joined = prop_list.join(", ");
        if is_truncated {
            let more_count = (ordered_names.len() - display_count).to_string();
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

    fn missing_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> String {
        if let Some(display) = self.enum_mapped_property_name_for_display(property_name, target) {
            return display;
        }
        self.ctx.types.resolve_atom_ref(property_name).to_string()
    }

    fn enum_mapped_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> Option<String> {
        let property_key = self.ctx.types.resolve_atom_ref(property_name).to_string();
        let (_, args) = crate::query_boundaries::common::application_info(self.ctx.types, target)?;

        args.into_iter()
            .find_map(|arg| self.enum_key_property_name_for_display(&property_key, arg))
    }

    fn enum_key_property_name_for_display(
        &mut self,
        property_key: &str,
        key_type: TypeId,
    ) -> Option<String> {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, key_type)
        {
            return members
                .iter()
                .find_map(|&member| self.enum_key_property_name_for_display(property_key, member));
        }

        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, key_type)
            .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, key_type))?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind == tsz_solver::def::DefKind::Enum && !def.enum_members.is_empty() {
            return self.enum_property_name_from_parent_def(property_key, &def);
        }

        self.enum_property_name_from_member_type(property_key, key_type, &def)
    }

    fn enum_property_name_from_parent_def(
        &mut self,
        property_key: &str,
        enum_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        let enum_name = self.ctx.types.resolve_atom_ref(enum_def.name).to_string();
        let enum_symbol_id = tsz_binder::SymbolId(enum_def.symbol_id?);
        let enum_symbol = self.ctx.binder.get_symbol(enum_symbol_id)?;
        let exports = enum_symbol.exports.as_ref()?;

        for (member_atom, _) in &enum_def.enum_members {
            let member_name = self.ctx.types.resolve_atom_ref(*member_atom).to_string();
            let Some(member_symbol_id) = exports.get(&member_name) else {
                continue;
            };
            let Some(member_type) = self.ctx.symbol_types.get(&member_symbol_id).copied() else {
                continue;
            };
            if self.enum_member_type_matches_property_key(member_type, property_key) {
                return Some(format!("[{enum_name}.{member_name}]"));
            }
        }

        None
    }

    fn enum_property_name_from_member_type(
        &mut self,
        property_key: &str,
        member_type: TypeId,
        member_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        if !self.enum_member_type_matches_property_key(member_type, property_key) {
            return None;
        }

        let member_symbol_id = tsz_binder::SymbolId(member_def.symbol_id?);
        let member_symbol = self.ctx.binder.get_symbol(member_symbol_id)?;
        if member_symbol.parent.is_none() {
            return None;
        }
        let enum_symbol = self.ctx.binder.get_symbol(member_symbol.parent)?;
        Some(format!(
            "[{}.{}]",
            enum_symbol.escaped_name, member_symbol.escaped_name
        ))
    }

    fn enum_member_type_matches_property_key(
        &self,
        member_type: TypeId,
        property_key: &str,
    ) -> bool {
        let value_type =
            crate::query_boundaries::common::enum_member_type(self.ctx.types, member_type)
                .unwrap_or(member_type);
        crate::query_boundaries::common::literal_value(self.ctx.types, value_type)
            .and_then(|literal| self.literal_property_key_text(literal))
            .is_some_and(|key| key == property_key)
    }

    fn literal_property_key_text(&self, literal: tsz_solver::LiteralValue) -> Option<String> {
        match literal {
            tsz_solver::LiteralValue::String(atom) | tsz_solver::LiteralValue::BigInt(atom) => {
                Some(self.ctx.types.resolve_atom_ref(atom).to_string())
            }
            tsz_solver::LiteralValue::Number(value) => {
                let value = value.0;
                if value == 0.0 {
                    Some("0".to_string())
                } else if value.is_finite() && value.fract() == 0.0 {
                    Some(format!("{value:.0}"))
                } else {
                    Some(value.to_string())
                }
            }
            tsz_solver::LiteralValue::Boolean(value) => Some(value.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_optional_property_required(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self
                .private_identifier_missing_source_base_display(source, property_name)
                .unwrap_or_else(|| self.format_type_diagnostic(source));
            let target_str = self.format_type_diagnostic(target);
            let detail = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name.clone(),
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            });
            diag
        } else {
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self
                .private_identifier_missing_source_base_display(source, property_name)
                .unwrap_or_else(|| self.format_type_diagnostic(source));
            let target_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            )
        }
    }

    fn private_identifier_missing_source_base_display(
        &mut self,
        source: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let property_name = self.ctx.types.resolve_atom_ref(property_name);
        if !property_name.starts_with('#') {
            return None;
        }

        let source_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source)?;
        let source_symbol = self.ctx.binder.get_symbol(source_shape.symbol?)?;
        let source_declarations = source_symbol.declarations.clone();

        for decl_idx in source_declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(heritage_clauses) = &interface.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else if type_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
                    {
                        self.ctx
                            .arena
                            .get_type_ref(type_node)
                            .map_or(type_idx, |type_ref| type_ref.type_name)
                    } else {
                        type_idx
                    };

                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };
                    let Some(base_symbol) = self
                        .get_cross_file_symbol(base_sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(base_sym_id))
                    else {
                        continue;
                    };
                    let base_declarations = base_symbol.declarations.clone();

                    for base_decl_idx in base_declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                            continue;
                        };
                        let base_type = self.get_class_instance_type(base_decl_idx, base_class);
                        return Some(self.format_type_diagnostic(base_type));
                    }
                }
            }
        }

        None
    }

    #[allow(clippy::too_many_arguments)]
    fn render_property_nominal_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        _depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        if let Some((prop_name, owner_name, visibility)) =
            self.private_or_protected_member_missing_display(source, target, Some(property_name))
        {
            let widened_source = self.widen_type_for_display(source);
            let src_str = if source == TypeId::OBJECT {
                "{}".to_string()
            } else {
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = self.format_type_diagnostic(target);
            let message = self.private_or_protected_assignability_message(
                &src_str,
                &tgt_str,
                &prop_name,
                &owner_name,
                visibility,
                None,
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(source, target, idx);
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
        if let Some(detail) = self.nominal_mismatch_detail(source, target, property_name) {
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
            });
        }
        diag
    }

    #[allow(clippy::too_many_arguments)]
    fn render_return_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        source_return: TypeId,
        target_return: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
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

            // tsc's elaboration shape for return-type mismatches goes
            // straight from the top-level message into the inner mismatch
            // (e.g. "Type 'Object' is not assignable to type 'string'.")
            // without an intermediate "Return type 'X' is not assignable
            // to 'Y'." line. Only emit the "Return type ..." fallback when
            // there is no nested reason that already carries the inner
            // mismatch — otherwise we'd double-elaborate the same gap.
            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
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
            } else {
                let ret_source_str = self.format_type_diagnostic(source_return);
                let ret_target_str = self.format_type_diagnostic(target_return);
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
            }

            diag
        } else {
            let source_str = self.format_type_diagnostic(source_return);
            let target_str = self.format_type_diagnostic(target_return);
            let message =
                format!("Return type '{source_str}' is not assignable to '{target_str}'.");
            let mut diag =
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
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

    /// Locate the span of an excess property name within a source expression.
    ///
    /// Walks any surrounding parenthesized expression, `||`/`??`/`,` combinator,
    /// or conditional `? :` to reach the object literal that declares the
    /// property and returns the span of that property's name token. tsc
    /// underlines the property (e.g. `b` in `{ a: '', b: 123 } || ...`) rather
    /// than the containing literal's `{`; preserving that anchor is required
    /// for TS2353 fingerprint parity.
    pub(crate) fn find_excess_property_anchor(
        &self,
        idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        const MAX_DEPTH: u32 = 8;
        // Stack holds (node, depth). Popping left-before-right requires pushing
        // right first (LIFO) so the leftmost operand is inspected first — matches
        // tsc's left-to-right property enumeration for `||` / `??` / `,`.
        let mut stack: Vec<(NodeIndex, u32)> = vec![(idx, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                if let Some(span) =
                    self.excess_property_name_span_in_literal(current, property_name)
                {
                    return Some(span);
                }
                continue;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                stack.push((paren.expression, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
            {
                stack.push((bin.right, depth + 1));
                stack.push((bin.left, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            {
                stack.push((cond.when_false, depth + 1));
                stack.push((cond.when_true, depth + 1));
                continue;
            }
        }
        None
    }

    fn excess_property_name_span_in_literal(
        &self,
        literal_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = self.ctx.arena.get(literal_idx)?;
        let literal = self.ctx.arena.get_literal_expr(node)?;
        for &elem in &literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem)?;
            if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
                && self.property_name_matches_atom(method.name, property_name)
            {
                return self.property_name_span(method.name);
            }
        }
        None
    }

    fn property_name_matches_atom(
        &self,
        name_idx: NodeIndex,
        target: tsz_common::interner::Atom,
    ) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let resolved = self.ctx.types.resolve_atom_ref(target);
        let target_str: &str = &resolved;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return ident.escaped_text.as_str() == target_str;
        }
        if let Some(literal) = self.ctx.arena.get_literal(name_node) {
            return literal.text.as_str() == target_str;
        }
        false
    }

    fn property_name_span(&self, name_idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(name_idx)?;
        Some((node.pos, node.end.saturating_sub(node.pos)))
    }
}
