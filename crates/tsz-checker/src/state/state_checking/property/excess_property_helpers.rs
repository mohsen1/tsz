//! Excess-property diagnostic helpers for object-literal checking.
//!
//! Extracted from `state_checking/property.rs` as pure code motion to keep
//! that file under its size ratchet. All helpers are inherent methods on
//! `CheckerState`, so call sites are unchanged.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn target_index_signature_accepts_source_property_with_env(
        &self,
        target: TypeId,
        source_prop: &tsz_solver::PropertyInfo,
    ) -> bool {
        if crate::query_boundaries::assignability::target_index_signature_accepts_source_property(
            self.ctx.types,
            target,
            source_prop,
        ) {
            return true;
        }
        if !source_prop.is_symbol_named {
            return false;
        }

        if let Some(shape) = crate::query_boundaries::common::get_merged_object_shape_for_type(
            self.ctx.types,
            target,
        ) {
            return shape.string_index.as_ref().is_some_and(|idx| {
                let key_type = self.resolve_index_signature_key_type_via_env(idx.key_type);
                key_type != idx.key_type
                    && crate::query_boundaries::assignability::index_signature_key_type_accepts_symbol(
                        self.ctx.types,
                        key_type,
                    )
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            return members.iter().any(|&member| {
                self.target_index_signature_accepts_source_property_with_env(member, source_prop)
            });
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)
        {
            return members.iter().any(|&member| {
                self.target_index_signature_accepts_source_property_with_env(member, source_prop)
            });
        }

        false
    }

    pub(crate) fn resolve_index_signature_key_type_via_env(&self, key_type: TypeId) -> TypeId {
        let mut current = key_type;
        for _ in 0..8 {
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, current)
            else {
                break;
            };
            let resolved = self.ctx.type_env.borrow().get_def(def_id);
            match resolved {
                Some(next) if next != current => current = next,
                _ => break,
            }
        }
        current
    }

    pub(crate) fn report_excess_function_like_implicit_any(&mut self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::METHOD_DECLARATION
            && node.kind != syntax_kind_ext::GET_ACCESSOR
            && node.kind != syntax_kind_ext::SET_ACCESSOR
        {
            return false;
        }
        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };
        for &param_idx in &func.parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if param.type_annotation.is_some() {
                continue;
            }
            let param_name = self.parameter_name_for_error(param.name);
            if param_name.is_empty() {
                continue;
            }
            let Some((start, end)) = self.get_node_span(param.name) else {
                continue;
            };
            let template = tsz_common::diagnostics::get_message_template(
                crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
            )
            .unwrap_or("");
            let message = crate::diagnostics::format_message(template, &[&param_name, "any"]);
            self.ctx
                .deferred_excess_property_implicit_any_diagnostics
                .push(tsz_common::diagnostics::Diagnostic::error(
                    self.ctx.file_name.clone(),
                    start,
                    end.saturating_sub(start),
                    message,
                    crate::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE,
                ));
        }
        if func.body.is_some() {
            self.check_for_nested_function_ts7006(func.body);
        }
        true
    }

    pub(crate) fn check_excess_property_initializer_implicit_any(
        &mut self,
        elem_idx: NodeIndex,
        _target: TypeId,
    ) {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return;
        };

        match elem_node.kind {
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    self.invalidate_initializer_for_context_change(prop.initializer);
                    if !self.report_excess_function_like_implicit_any(prop.initializer) {
                        self.check_for_nested_function_ts7006(prop.initializer);
                    }
                }
            }
            syntax_kind_ext::METHOD_DECLARATION
                if self.ctx.arena.get_method_decl(elem_node).is_some() =>
            {
                self.invalidate_function_like_for_contextual_retry(elem_idx);
                self.report_excess_function_like_implicit_any(elem_idx);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
                if self.ctx.arena.get_accessor(elem_node).is_some() =>
            {
                self.invalidate_function_like_for_contextual_retry(elem_idx);
                self.report_excess_function_like_implicit_any(elem_idx);
            }
            _ => {}
        }
    }

    pub(crate) fn emit_tracked_excess_property(
        &mut self,
        tracked: Option<(Atom, NodeIndex, u32)>,
        target: TypeId,
    ) {
        if let Some((prop_atom, report_idx, _)) = tracked {
            let prop_name = self.object_literal_property_display_name(
                report_idx,
                self.ctx.types.resolve_atom(prop_atom).as_ref(),
            );
            self.error_excess_property_at(&prop_name, target, report_idx);
            self.check_excess_property_initializer_implicit_any(report_idx, target);
        }
    }

    pub(crate) fn track_earliest_excess_with_target(
        &self,
        current: &mut Option<(Atom, NodeIndex, u32, TypeId)>,
        name: Atom,
        report_idx: NodeIndex,
        target: TypeId,
    ) {
        let pos = self.ctx.arena.get(report_idx).map_or(u32::MAX, |n| n.pos);
        if current.is_none_or(|(_, _, best, _)| pos < best) {
            *current = Some((name, report_idx, pos, target));
        }
    }

    pub(crate) fn emit_tracked_excess_property_with_target(
        &mut self,
        tracked: Option<(Atom, NodeIndex, u32, TypeId)>,
    ) {
        if let Some((prop_atom, report_idx, _, target)) = tracked {
            let prop_name = self.object_literal_property_display_name(
                report_idx,
                self.ctx.types.resolve_atom(prop_atom).as_ref(),
            );
            self.error_excess_property_at(&prop_name, target, report_idx);
            self.check_excess_property_initializer_implicit_any(report_idx, target);
        }
    }

    pub(crate) fn union_member_has_type_parameter_for_excess_display(
        &self,
        member: TypeId,
    ) -> bool {
        query::is_type_parameter_like(self.ctx.types, member)
            || crate::query_boundaries::common::contains_generic_type_parameters(
                self.ctx.types,
                member,
            )
    }

    pub(crate) fn type_is_recursive_operation_application(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::type_predicates::is_recursive_operation_application(
            self.ctx.types,
            &self.ctx.definition_store,
            type_id,
        ) {
            return true;
        }

        if let Some(alias) = self.ctx.types.get_display_alias(type_id)
            && crate::query_boundaries::type_predicates::is_recursive_operation_application(
                self.ctx.types,
                &self.ctx.definition_store,
                alias,
            )
        {
            return true;
        }

        false
    }

    pub(crate) fn type_contains_recursive_operation_application(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::type_predicates::contains_recursive_operation_application(
            self.ctx.types,
            &self.ctx.definition_store,
            type_id,
        )
    }
}
