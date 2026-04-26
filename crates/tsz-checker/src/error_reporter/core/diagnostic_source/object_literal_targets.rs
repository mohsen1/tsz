use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn object_literal_property_contextual_target_for_diagnostic(
        &mut self,
        anchor_idx: NodeIndex,
        current_target: TypeId,
    ) -> Option<TypeId> {
        let mut current = anchor_idx;
        let mut property_elem = None;
        let mut object_literal_idx = None;

        for _ in 0..12 {
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT =>
                {
                    let is_object_literal_property = self
                        .ctx
                        .arena
                        .get_extended(current)
                        .and_then(|ext| self.ctx.arena.get(ext.parent))
                        .is_some_and(|parent| {
                            parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    if is_object_literal_property {
                        object_literal_idx = self.ctx.arena.parent_of(current);
                        property_elem = Some(current);
                        break;
                    }
                }
                k if k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION =>
                {
                    return None;
                }
                _ => {}
            }

            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                break;
            }
            current = parent;
        }

        let property_elem = property_elem?;
        let elem_node = self.ctx.arena.get(property_elem)?;
        let (prop_name_idx, prop_name) = match elem_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                (
                    prop.name,
                    self.object_literal_property_name_text(prop.name)?,
                )
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let prop = self.ctx.arena.get_shorthand_property(elem_node)?;
                (
                    prop.name,
                    self.object_literal_property_name_text(prop.name)?,
                )
            }
            _ => return None,
        };

        let object_contextual_target = object_literal_idx.and_then(|object_literal_idx| {
            self.ctx
                .object_literal_contextual_targets
                .get(&object_literal_idx)
                .copied()
        });
        let object_property_target = object_contextual_target.and_then(|contextual_target| {
            self.object_literal_target_property_type(contextual_target, prop_name_idx, &prop_name)
                .map(|(_, diagnostic_target)| diagnostic_target)
        });
        let property_diag_target = self
            .ctx
            .object_literal_property_diag_targets
            .get(&property_elem)
            .copied();
        let mut raw_call_param_property_target = None;
        if let Some(object_literal_idx) = object_literal_idx
            && let Some(parent_idx) = self
                .ctx
                .arena
                .get_extended(object_literal_idx)
                .map(|ext| ext.parent)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION
            )
            && let Some(call) = self.ctx.arena.get_call_expr(parent_node)
            && let Some(args) = call.arguments.as_ref()
            && let Some(arg_index) = args
                .nodes
                .iter()
                .position(|&candidate| candidate == object_literal_idx)
        {
            let raw_callee_type = self
                .resolve_qualified_symbol(call.expression)
                .or_else(|| self.resolve_identifier_symbol(call.expression))
                .map(|sym| self.get_type_of_symbol(sym))
                .unwrap_or_else(|| self.get_type_of_node(call.expression));
            if let Some(raw_sig) = crate::query_boundaries::checkers::call::get_call_signature(
                self.ctx.types,
                raw_callee_type,
                args.nodes.len(),
            )
                && let Some(param_type) = raw_sig
                    .params
                    .get(arg_index)
                    .map(|param| param.type_id)
                    .or_else(|| {
                        let last = raw_sig.params.last()?;
                        last.rest.then_some(last.type_id)
                    })
                {
                    raw_call_param_property_target =
                        self.mapped_target_property_display_type(param_type, &prop_name);
                    if self.ctx.strict_null_checks()
                        && crate::query_boundaries::class_type::type_includes_undefined(
                            self.ctx.types,
                            current_target,
                        )
                        && let Some(target) = raw_call_param_property_target
                        && !crate::query_boundaries::class_type::type_includes_undefined(
                            self.ctx.types,
                            target,
                        )
                    {
                        raw_call_param_property_target =
                            Some(self.ctx.types.union2(target, TypeId::UNDEFINED));
                    }
                }
        }
        let contextual_target = raw_call_param_property_target
            .or(object_property_target)
            .or(property_diag_target)?;
        if contextual_target.is_any_unknown_or_error() {
            return None;
        }

        let has_generic_surface = crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            contextual_target,
        ) || crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            self.evaluate_type_for_assignability(contextual_target),
        ) || crate::query_boundaries::common::is_index_access_type(
            self.ctx.types,
            contextual_target,
        ) || crate::query_boundaries::common::is_keyof_type(
            self.ctx.types,
            contextual_target,
        ) || crate::query_boundaries::common::is_conditional_type(
            self.ctx.types,
            contextual_target,
        ) || crate::query_boundaries::common::type_application(
            self.ctx.types,
            contextual_target,
        )
        .is_some();
        let same_target_but_recoverable = contextual_target == current_target
            && self
                .collapsed_index_access_intersection_display(contextual_target)
                .is_some();
        if !has_generic_surface
            || (contextual_target == current_target && !same_target_but_recoverable)
        {
            return None;
        }

        Some(contextual_target)
    }

    pub(in crate::error_reporter) fn format_object_literal_property_diag_target(
        &mut self,
        target: TypeId,
    ) -> String {
        self.collapsed_index_access_intersection_display(target)
            .unwrap_or_else(|| self.format_type_for_assignability_message(target))
    }

    fn collapsed_index_access_intersection_display(&mut self, target: TypeId) -> Option<String> {
        let members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target)?;
        if members.len() < 2 {
            return None;
        }

        let mut object_members = Vec::with_capacity(members.len());
        let mut shared_index: Option<TypeId> = None;
        let mut has_optional_property = false;

        for &member in &members {
            let (object_type, index_type) =
                crate::query_boundaries::common::index_access_types(self.ctx.types, member)?;
            match shared_index {
                Some(existing) if existing != index_type => return None,
                Some(_) => {}
                None => shared_index = Some(index_type),
            }

            let prop_atom =
                crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)?;
            has_optional_property |= crate::query_boundaries::common::find_property_in_object(
                self.ctx.types,
                object_type,
                prop_atom,
            )
            .is_some_and(|prop| prop.optional)
                || crate::query_boundaries::class_type::type_includes_undefined(
                    self.ctx.types,
                    self.evaluate_type_with_env(member),
                );
            object_members.push(object_type);
        }

        let shared_index = shared_index?;
        let combined_object =
            tsz_solver::utils::intersection_or_single(self.ctx.types, object_members);
        let mut combined = self
            .ctx
            .types
            .factory()
            .index_access(combined_object, shared_index);
        if has_optional_property && self.ctx.strict_null_checks() {
            combined = self.ctx.types.union2(combined, TypeId::UNDEFINED);
        }

        Some(self.format_type_for_assignability_message(combined))
    }
}
