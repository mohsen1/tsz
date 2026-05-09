use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn emit_polymorphic_this_call_assignment_error(
        &mut self,
        source_idx: NodeIndex,
        target: TypeId,
        diag_idx: NodeIndex,
    ) -> bool {
        if let Some(display_source) =
            self.polymorphic_this_call_assignment_source(source_idx, target)
        {
            self.error_type_not_assignable_at_with_display_types(display_source, target, diag_idx);
            return true;
        }
        false
    }

    pub(crate) fn emit_polymorphic_this_property_assignment_error(
        &mut self,
        source: TypeId,
        target: TypeId,
        diag_idx: NodeIndex,
    ) -> bool {
        if source == target {
            return false;
        }

        let Some((base, target_args)) = self.application_info_or_display_alias(target) else {
            return false;
        };
        if !target_args.contains(&TypeId::ANY) {
            return false;
        }

        let target_with_unbound_this =
            crate::query_boundaries::state::type_environment::evaluate_type_suppressing_this(
                self.ctx.types,
                &self.ctx,
                target,
            );
        let Some(target_shape) = crate::query_boundaries::common::get_merged_object_shape_for_type(
            self.ctx.types,
            target_with_unbound_this,
        ) else {
            return false;
        };
        let mut has_this_property = target_shape.properties.iter().any(|prop| {
            !prop.is_method
                && (crate::query_boundaries::common::contains_this_type(
                    self.ctx.types,
                    prop.type_id,
                ) || (prop.write_type != TypeId::NONE
                    && crate::query_boundaries::common::contains_this_type(
                        self.ctx.types,
                        prop.write_type,
                    )))
        });

        if !has_this_property {
            let base_body = self.resolve_lazy_type(base);
            if let Some(base_shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(
                    self.ctx.types,
                    base_body,
                )
            {
                has_this_property = base_shape.properties.iter().any(|prop| {
                    !prop.is_method
                        && (crate::query_boundaries::common::contains_this_type(
                            self.ctx.types,
                            prop.type_id,
                        ) || (prop.write_type != TypeId::NONE
                            && crate::query_boundaries::common::contains_this_type(
                                self.ctx.types,
                                prop.write_type,
                            )))
                });
            }
        }
        if !has_this_property {
            return false;
        }

        self.error_type_not_assignable_at_with_display_types(source, target, diag_idx);
        true
    }

    pub(super) fn polymorphic_this_call_assignment_source(
        &mut self,
        source_idx: NodeIndex,
        target: TypeId,
    ) -> Option<TypeId> {
        let source_node = self.ctx.arena.get(source_idx)?;
        if source_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.ctx.arena.get_call_expr(source_node)?;
        let callee_node = self.ctx.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(callee_node)?;
        let name = self
            .ctx
            .arena
            .get(access.name_or_argument)
            .and_then(|node| self.ctx.arena.get_identifier(node))?
            .escaped_text
            .as_str();
        let name_atom = self.ctx.types.intern_string(name);

        let target_with_unbound_this =
            crate::query_boundaries::state::type_environment::evaluate_type_suppressing_this(
                self.ctx.types,
                &self.ctx,
                target,
            );
        let target_shape = crate::query_boundaries::common::get_merged_object_shape_for_type(
            self.ctx.types,
            target_with_unbound_this,
        )?;
        let target_prop =
            tsz_solver::PropertyInfo::find_in_slice(target_shape.properties.as_slice(), name_atom)?;
        if !target_prop.is_method
            || !crate::query_boundaries::common::contains_this_type(
                self.ctx.types,
                target_prop.type_id,
            )
        {
            return None;
        }

        let receiver_type = self.get_type_of_node(access.expression);
        if receiver_type == target || !self.is_assignable_to(receiver_type, target) {
            return None;
        }
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, receiver_type)
            && let Some(member) = members
                .into_iter()
                .find(|&member| member != target && self.is_assignable_to(member, target))
        {
            return Some(member);
        }
        let default_atom = self.ctx.types.intern_string("default");
        if let Some(receiver_shape) =
            crate::query_boundaries::common::get_merged_object_shape_for_type(
                self.ctx.types,
                receiver_type,
            )
            && let Some(default_prop) = tsz_solver::PropertyInfo::find_in_slice(
                receiver_shape.properties.as_slice(),
                default_atom,
            )
        {
            return Some(default_prop.type_id);
        }
        // A plain subtype receiver (no intersection, no default export property) means
        // the polymorphic-this call returns a valid subtype of the target. This is
        // ordinary subtype assignability and must not produce TS2322.
        None
    }
}
