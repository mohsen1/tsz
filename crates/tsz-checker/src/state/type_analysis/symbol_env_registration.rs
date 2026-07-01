//! Type-environment publication for computed symbol results.

use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::{SymbolRef, TypeId, TypeParamInfo};

use crate::state::CheckerState;

impl CheckerState<'_> {
    pub(super) fn publish_symbol_result_to_type_envs(
        &mut self,
        sym_id: SymbolId,
        result: TypeId,
        type_params: &[TypeParamInfo],
    ) {
        // For class symbols, cache BOTH the constructor type (for value position)
        // and the instance type (for type position with typeof/TypeQuery resolution).
        let class_env_entry = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
            if symbol.has_any_flags(symbol_flags::CLASS) {
                self.class_instance_type_with_params_from_symbol(sym_id)
            } else {
                None
            }
        });

        // For non-class interface+value merges, register the VALUE-space side
        // that deferred `typeof X` queries should resolve to.
        let merged_interface_value_typeof = (class_env_entry.is_none())
            .then(|| self.merged_interface_value_typeof_type(sym_id))
            .flatten();

        let def_id = self.ctx.get_existing_def_id(sym_id);
        let symbol_ref = SymbolRef(sym_id.0);
        let env_params = if type_params.is_empty() {
            def_id
                .and_then(|d| self.ctx.get_def_type_params(d))
                .unwrap_or_default()
        } else {
            type_params.to_vec()
        };
        self.ctx
            .register_symbol_type_in_env(symbol_ref, result, env_params.clone());

        if let Some(def_id) = def_id {
            let def_params = if class_env_entry.is_some() {
                type_params.to_vec()
            } else {
                env_params
            };
            self.ctx
                .register_def_auto_params_in_env(def_id, result, def_params);

            if let Some((instance_type, _instance_params)) = &class_env_entry {
                self.ctx
                    .register_class_instance_in_env(def_id, *instance_type);

                let parents = self.ctx.inheritance_graph.get_parents(sym_id);
                if let Some(&parent_sym) = parents.first()
                    && let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym)
                {
                    self.ctx
                        .register_class_extends_in_env(def_id, parent_def_id);
                }
            }

            self.maybe_register_numeric_enum(sym_id, def_id);

            if let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
                && symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
            {
                let parent_sym_id = symbol.parent;
                if let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym_id) {
                    self.ctx.register_enum_parent_in_env(def_id, parent_def_id);
                }
            }
        }

        if let Some(value_type) = merged_interface_value_typeof {
            self.register_typeof_value_type_in_env(sym_id, value_type);
        }
        if class_env_entry.is_some()
            && let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
        {
            self.ctx.register_def_symbol_mapping_in_env(def_id, sym_id);
        }
    }
}
