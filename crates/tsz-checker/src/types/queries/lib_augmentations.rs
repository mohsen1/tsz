use std::sync::Arc;

use rustc_hash::FxHashMap;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

use crate::state::CheckerState;

use super::lib_resolution::{
    augmentation_def_id_from_node, no_value_resolver, resolve_augmentation_node,
};

impl<'a> CheckerState<'a> {
    /// Lower augmentation declarations from a given arena and return the resulting `TypeId`.
    ///
    /// This is the shared implementation for global-augmentation lowering used by both
    /// `resolve_lib_type_by_name` and `resolve_lib_type_with_params`.
    pub(crate) fn lower_augmentation_for_arena(
        &self,
        arena_ref: &NodeArena,
        decls: &[NodeIndex],
        lib_contexts: &[crate::context::LibContext],
    ) -> TypeId {
        let binder_ref = self.ctx.binder;
        let decl_binder = self
            .ctx
            .get_binder_for_arena(arena_ref)
            .unwrap_or(binder_ref);
        let global_idx = self.ctx.global_file_locals_index.as_deref();
        let all_binders_slice = self.ctx.all_binders.as_ref().map(|v| v.as_slice());
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_augmentation_node(
                decl_binder,
                arena_ref,
                node_idx,
                global_idx,
                all_binders_slice,
                lib_contexts,
            )
            .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            augmentation_def_id_from_node(
                &self.ctx,
                decl_binder,
                arena_ref,
                node_idx,
                global_idx,
                all_binders_slice,
                lib_contexts,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
        };
        let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &no_value_resolver,
        )
        .with_name_def_id_resolver(&name_resolver);
        lowering.lower_interface_declarations(decls)
    }

    /// Merge global augmentations for `name` into `lib_type_id`.
    pub(crate) fn merge_global_augmentations(
        &self,
        name: &str,
        lib_type_id: Option<TypeId>,
        lib_contexts: &[crate::context::LibContext],
    ) -> Option<TypeId> {
        let augmentation_decls = self.ctx.binder.global_augmentations.get(name)?;
        if augmentation_decls.is_empty() {
            return lib_type_id;
        }

        let factory = self.ctx.types.factory();
        let current_arena: &NodeArena = self.ctx.arena;
        let mut result = lib_type_id;
        let mut current_file_decls: Vec<NodeIndex> = Vec::new();
        let mut cross_file_groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)> =
            FxHashMap::default();

        for aug in augmentation_decls {
            if let Some(ref arena) = aug.arena {
                let key = Arc::as_ptr(arena) as usize;
                cross_file_groups
                    .entry(key)
                    .or_insert_with(|| (Arc::clone(arena), Vec::new()))
                    .1
                    .push(aug.node);
            } else {
                current_file_decls.push(aug.node);
            }
        }

        if !current_file_decls.is_empty() {
            let aug_type =
                self.lower_augmentation_for_arena(current_arena, &current_file_decls, lib_contexts);
            result = Some(if let Some(lib_type) = result {
                factory.intersection2(lib_type, aug_type)
            } else {
                aug_type
            });
        }

        for (arena, decls) in cross_file_groups.values() {
            let aug_type = self.lower_augmentation_for_arena(arena.as_ref(), decls, lib_contexts);
            result = Some(if let Some(lib_type) = result {
                factory.intersection2(lib_type, aug_type)
            } else {
                aug_type
            });
        }

        result
    }

    pub(crate) fn cached_lib_type_is_usable(&self, name: &str, cached: Option<TypeId>) -> bool {
        let Some(type_id) = cached else {
            return true;
        };
        if self.ctx.file_local_type_shadow_for_lib_name(name) {
            return false;
        }
        if !crate::query_boundaries::common::type_id_is_known_to_db(self.ctx.types, type_id) {
            return false;
        }
        for def_id in crate::query_boundaries::common::collect_lazy_def_ids(self.ctx.types, type_id)
        {
            let has_body_in_env = self
                .ctx
                .type_env
                .try_borrow()
                .is_ok_and(|env| env.get_def(def_id).is_some());
            if !has_body_in_env && self.ctx.definition_store.get_body(def_id).is_none() {
                return false;
            }
        }
        let Some(global_name) = name.strip_suffix("Constructor") else {
            return true;
        };
        if !self.is_known_global_value_name(global_name)
            || matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
        {
            return true;
        }

        crate::query_boundaries::common::has_construct_signatures(self.ctx.types, type_id)
    }
}
