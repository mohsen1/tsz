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

        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            let def_id = self.ctx.get_existing_def_id(sym_id);
            let symbol_ref = SymbolRef(sym_id.0);

            if let Some((instance_type, _instance_params)) = &class_env_entry {
                if type_params.is_empty() {
                    self.ctx.insert_symbol_type_and_mirror(
                        &mut env,
                        symbol_ref,
                        result,
                        Vec::new(),
                    );
                    if let Some(def_id) = def_id {
                        env.insert_def(def_id, result);
                        env.insert_class_instance_type(def_id, *instance_type);
                    }
                } else {
                    let params = type_params.to_vec();
                    self.ctx.insert_symbol_type_and_mirror(
                        &mut env,
                        symbol_ref,
                        result,
                        params.clone(),
                    );
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, params);
                        env.insert_class_instance_type(def_id, *instance_type);
                    }
                }

                if let Some(def_id) = def_id {
                    let parents = self.ctx.inheritance_graph.get_parents(sym_id);
                    if let Some(&parent_sym) = parents.first()
                        && let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym)
                    {
                        self.ctx
                            .register_class_extends_in_envs(def_id, parent_def_id);
                    }
                }
            } else if type_params.is_empty() {
                let lib_params = def_id.and_then(|d| self.ctx.get_def_type_params(d));
                if let Some(params) = lib_params {
                    self.ctx.insert_symbol_type_and_mirror(
                        &mut env,
                        symbol_ref,
                        result,
                        params.clone(),
                    );
                    if let Some(def_id) = def_id {
                        env.insert_def_with_params(def_id, result, params);
                    }
                } else {
                    self.ctx.insert_symbol_type_and_mirror(
                        &mut env,
                        symbol_ref,
                        result,
                        Vec::new(),
                    );
                    if let Some(def_id) = def_id {
                        env.insert_def(def_id, result);
                    }
                }
            } else {
                let params = type_params.to_vec();
                self.ctx.insert_symbol_type_and_mirror(
                    &mut env,
                    symbol_ref,
                    result,
                    params.clone(),
                );
                if let Some(def_id) = def_id {
                    env.insert_def_with_params(def_id, result, params);
                }
            }

            if let Some(def_id) = def_id {
                self.maybe_register_numeric_enum(&mut env, sym_id, def_id);
            }

            if let Some(def_id) = def_id
                && let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
                && symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
            {
                let parent_sym_id = symbol.parent;
                if let Some(parent_def_id) = self.ctx.get_existing_def_id(parent_sym_id) {
                    env.register_enum_parent(def_id, parent_def_id);
                }
            }
        } else {
            let sym_name = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map_or("<unknown>", |s| s.escaped_name.as_str());
            tracing::warn!(
                sym_id = sym_id.0,
                sym_name = sym_name,
                type_id = result.0,
                type_params_count = type_params.len(),
                "type_env try_borrow_mut FAILED - skipping insertion"
            );
        }

        if let Some(def_id) = self.ctx.get_existing_def_id(sym_id) {
            if let Some((instance_type, _)) = &class_env_entry {
                self.ctx
                    .mirror_def_in_type_environment(def_id, result, type_params);
                self.ctx
                    .mirror_class_instance_in_type_environment(def_id, *instance_type);
            } else {
                let lib_params = type_params
                    .is_empty()
                    .then(|| self.ctx.get_def_type_params(def_id))
                    .flatten();
                let params = lib_params.as_deref().unwrap_or(type_params);
                self.ctx
                    .mirror_def_in_type_environment(def_id, result, params);
            }
        }

        if let Some(value_type) = merged_interface_value_typeof {
            self.register_typeof_value_type_in_envs(sym_id, value_type);
        }
        if class_env_entry.is_some()
            && let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
        {
            self.ctx.register_def_symbol_mapping_in_envs(def_id, sym_id);
        }
    }
}
