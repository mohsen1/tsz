use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    fn property_info_for_missing_property_satisfaction(
        &mut self,
        ty: TypeId,
        name: Atom,
    ) -> Option<PropertyInfo> {
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
        name: Atom,
    ) -> Option<PropertyInfo> {
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
        name: Atom,
    ) -> Option<PropertyInfo> {
        types
            .iter()
            .copied()
            .find_map(|ty| self.property_info_for_missing_property_satisfaction(ty, name))
    }

    pub(super) fn missing_property_is_satisfied_by_source(
        &mut self,
        source_types: &[TypeId],
        target_types: &[TypeId],
        property_name: Atom,
    ) -> bool {
        let Some(source_prop) = self
            .property_info_for_any_missing_property_satisfaction_type(source_types, property_name)
        else {
            return false;
        };
        if source_prop.optional || source_prop.visibility != Visibility::Public {
            return false;
        }
        let Some(target_prop) = self
            .property_info_for_any_missing_property_satisfaction_type(target_types, property_name)
        else {
            return false;
        };
        if target_prop.visibility != Visibility::Public {
            return false;
        }
        let read_ok = if source_prop.is_method || target_prop.is_method {
            self.bivariant_callbacks_relation_outcome(source_prop.type_id, target_prop.type_id)
                .related
        } else {
            self.assign_relation_outcome(source_prop.type_id, target_prop.type_id)
                .related
        };
        let write_ok = target_prop.readonly
            || self
                .assign_relation_outcome(target_prop.write_type, source_prop.write_type)
                .related;

        read_ok && write_ok
    }
}
