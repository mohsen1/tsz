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
    ///
    /// Combines lib and user-side `interface X { ... }` declarations via
    /// `merge_interface_types` (a single merged object shape), with a
    /// fallback to structural intersection when the lib type is not
    /// interface-like — see `combine_augmentation_with_lib`.
    ///
    /// A plain `intersection2(lib, augment)` produces TS2536 false
    /// positives: `(lib & augment)[K]` distributes to `lib[K] &
    /// augment[K]`, and when `augment` only contributes an index signature
    /// (`{ [n: number]: HTMLElement }`), `augment["div"]` is invalid even
    /// though `"div"` is a real key of the merged interface.
    pub(crate) fn merge_global_augmentations(
        &mut self,
        name: &str,
        lib_type_id: Option<TypeId>,
        lib_contexts: &[crate::context::LibContext],
    ) -> Option<TypeId> {
        let augmentation_decls = self.ctx.binder.global_augmentations.get(name)?;
        if augmentation_decls.is_empty() {
            return lib_type_id;
        }

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
            result = Some(self.combine_augmentation_with_lib(result, aug_type));
        }

        for (arena, decls) in cross_file_groups.values() {
            let aug_type = self.lower_augmentation_for_arena(arena.as_ref(), decls, lib_contexts);
            result = Some(self.combine_augmentation_with_lib(result, aug_type));
        }

        result
    }

    fn combine_augmentation_with_lib(
        &mut self,
        lib_type: Option<TypeId>,
        aug_type: TypeId,
    ) -> TypeId {
        let Some(lib) = lib_type else {
            return aug_type;
        };

        // `merge_interface_types` matches tsc's declaration-merging
        // semantics for `interface X { ... }` augmenting a lib `interface X`.
        // When the lib type is not interface-like (value-only namespace,
        // intrinsic, etc.) the function's catch-all returns the `derived`
        // arm unchanged, which would silently drop the lib members. Detect
        // that case (the only way `merged` equals `aug_type` here is the
        // unchanged-`derived` catch-all, since `merge_interface_types_impl`
        // constructs a fresh shape in every other branch) and preserve the
        // lib members via intersection.
        let merged = self.merge_interface_types(aug_type, lib);
        if merged == aug_type && merged != lib {
            return self.ctx.types.factory().intersection2(lib, aug_type);
        }
        merged
    }

    /// Finalize a lib type's `DefId` registration after post-resolution
    /// transforms (heritage merge, global augmentation).
    ///
    /// Updates the type→def mapping so the formatter renders the named
    /// form (e.g. `"Date"` instead of expanding members), then updates the
    /// `DefId` body so subsequent `resolve_lazy(def_id)` consumers see the
    /// merged shape. Without the body update, `Lazy(DefId)` evaluates to
    /// the pre-merge body and the annotation path disagrees with the
    /// literal path (e.g. regex literals resolving via
    /// `resolve_lib_type_by_name` return the post-merge TypeId directly),
    /// producing false TS2322/TS2719.
    ///
    /// Two guards on the body update:
    ///   1. Skip when `ty` is `Lazy(def_id)` itself — registering
    ///      `DefId -> Lazy(DefId)` creates a self-cycle that blocks
    ///      application instantiation (e.g. `FlatArray<T, D>`).
    ///   2. Skip when the body already equals `ty` — `set_body`
    ///      unconditionally clears type evaluation caches for the def,
    ///      which is wasted work on repeated lookups.
    pub(crate) fn register_finalized_lib_body(&mut self, name: &str, ty: TypeId) {
        let name_atom = self.ctx.types.intern_string(name);
        let Some(defs) = self.ctx.definition_store.find_defs_by_name(name_atom) else {
            return;
        };
        let Some(&def_id) = defs.first() else {
            return;
        };
        self.ctx.definition_store.register_type_to_def(ty, def_id);
        if crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty) == Some(def_id) {
            return;
        }
        if self.ctx.definition_store.get_body(def_id) == Some(ty) {
            return;
        }
        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        self.ctx
            .register_def_auto_params_in_envs(def_id, ty, type_params);
    }

    /// Wrapper around `register_finalized_lib_body` that no-ops unless the
    /// name actually has user-side global augmentations. Used after
    /// `merge_global_augmentations` to register the augmented body so
    /// type-position accesses (`keyof X`, `X[K]`) see the merged shape.
    pub(crate) fn register_augmented_lib_body(&mut self, name: &str, ty: TypeId) {
        if self.ctx.binder.global_augmentations.contains_key(name) {
            self.register_finalized_lib_body(name, ty);
        }
    }

    pub(crate) fn cached_lib_type_is_usable(&self, name: &str, cached: Option<TypeId>) -> bool {
        let Some(type_id) = cached else {
            return true;
        };
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
