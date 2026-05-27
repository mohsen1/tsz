use crate::query_boundaries::class_type::callable_shape_for_type;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{CallableShape, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn record_heritage_extends(
        &mut self,
        current_sym: Option<SymbolId>,
        expr_idx: NodeIndex,
        base_instance_type: TypeId,
    ) {
        let Some(current_sym) = current_sym else {
            return;
        };
        let parent_def_id = self
            .ctx
            .definition_store
            .find_def_for_type(base_instance_type)
            .or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(base_instance_type)
                    .and_then(|alias| self.ctx.definition_store.find_def_for_type(alias))
            })
            .or_else(|| {
                self.heritage_name_text(expr_idx)
                    .and_then(|base_name| self.ctx.actual_lib_def_id_for_bare_name(&base_name))
            });
        let Some(parent_def_id) = parent_def_id else {
            return;
        };

        let child_def_id = self.ctx.get_or_create_def_id(current_sym);
        if child_def_id != parent_def_id {
            self.ctx
                .register_class_extends_in_envs(child_def_id, parent_def_id);
        }
    }

    pub(crate) fn refresh_constructor_instance_return_if_stale(
        &mut self,
        class_idx: NodeIndex,
        sym_id: SymbolId,
        final_instance: TypeId,
    ) {
        let Some(stale_instance) = self
            .ctx
            .class_instance_type_cache
            .get(&class_idx)
            .copied()
            .filter(|&cached| cached != final_instance && cached != TypeId::ERROR)
        else {
            return;
        };
        self.refresh_cached_constructor_instance_return(
            class_idx,
            sym_id,
            stale_instance,
            final_instance,
        );
    }

    fn refresh_cached_constructor_instance_return(
        &mut self,
        class_idx: NodeIndex,
        sym_id: SymbolId,
        stale_instance: TypeId,
        final_instance: TypeId,
    ) {
        let Some(&constructor_type) = self.ctx.class_constructor_type_cache.get(&class_idx) else {
            return;
        };
        let Some(shape) = callable_shape_for_type(self.ctx.types, constructor_type) else {
            return;
        };

        let mut construct_signatures = shape.construct_signatures.clone();
        let mut changed = false;
        for signature in &mut construct_signatures {
            if signature.return_type == stale_instance {
                signature.return_type = final_instance;
                changed = true;
            }
        }
        if !changed {
            return;
        }

        let refreshed = self.ctx.types.factory().callable(CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures,
            properties: shape.properties.clone(),
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: shape.symbol,
            is_abstract: shape.is_abstract,
        });
        self.ctx
            .class_constructor_type_cache
            .insert(class_idx, refreshed);
        if self
            .ctx
            .symbol_types
            .get(&sym_id)
            .is_some_and(|&cached| cached == constructor_type)
        {
            self.ctx.symbol_types.insert(sym_id, refreshed);
        }
    }
}
