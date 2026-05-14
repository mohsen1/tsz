use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeList;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn normalize_base_instance_type_for_merge(
        &mut self,
        base_instance_type: TypeId,
    ) -> TypeId {
        let evaluated = self.evaluate_application_type(base_instance_type);
        let resolved = self.resolve_lazy_type(evaluated);
        if resolved != base_instance_type {
            resolved
        } else {
            evaluated
        }
    }

    pub(super) fn array_base_instance_type_for_heritage(
        &mut self,
        base_sym_id: SymbolId,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if !self
            .ctx
            .binder
            .get_symbol(base_sym_id)
            .is_some_and(|symbol| symbol.escaped_name == "Array")
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(base_sym_id)
        {
            return None;
        }

        let array_base = tsz_solver::TypeResolver::get_array_base_type(self.ctx.types)?;
        let array_params =
            tsz_solver::TypeResolver::get_array_base_type_params(self.ctx.types).to_vec();
        Some(self.instantiate_base_instance_type_with_args(
            array_base,
            &array_params,
            type_arguments,
        ))
    }
}
