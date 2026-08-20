use std::sync::Arc;

use rustc_hash::FxHashMap;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::TypeId;

use crate::state::CheckerState;

use super::lib_resolution::{
    augmentation_def_id_from_node, no_value_resolver, resolve_augmentation_node,
};

/// Mutation-isolation campaign: freeze each lib def's shared-store body at
/// its *finalized* publication point (heritage-merged + augmented), so later
/// checkers' re-finalizations cannot republish a different (checker-relative)
/// form. Default-on; `TSZ_DISABLE_LIB_DEF_FREEZE=1` is the kill switch for
/// A/B parity measurement.
pub(crate) fn lib_def_finalize_freeze_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| !std::env::var("TSZ_DISABLE_LIB_DEF_FREEZE").is_ok_and(|v| v == "1"))
}

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
        // Sibling-lib declarations reach this path when a user global
        // augmentation forces re-merging a lib interface (e.g. `interface
        // Error { ... }` re-merges `Array`, whose es2019 `flat()` references
        // `FlatArray`). Their identifiers name GLOBAL lib types, but the
        // node resolver is arena-local: under a merged global the shifted
        // `SymbolId` can alias an unrelated lib symbol and mint a
        // wrong-identity def. Resolve lib-arena references name-first;
        // user-file augmentation decls keep node-first so local bindings
        // stay exact (same per-decl policy as
        // `lower_merged_interface_declarations`).
        let lowering =
            if crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena(
                arena_ref,
            ) {
                lowering.prefer_name_def_id_resolution()
            } else {
                lowering
            };
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

        // Fold the cross-file groups in deterministic program order, not the
        // address order a raw `cross_file_groups.values()` walk would produce
        // (`combine_augmentation_with_lib` -> `merge_interface_types` is
        // order-sensitive, so an address-ordered fold splits one merged lib
        // interface into distinct identities across runs — see
        // `order_cross_arena_augmentation_groups`).
        for (arena, decls) in self.order_cross_arena_augmentation_groups(cross_file_groups) {
            let aug_type = self.lower_augmentation_for_arena(arena.as_ref(), &decls, lib_contexts);
            result = Some(self.combine_augmentation_with_lib(result, aug_type));
        }

        result
    }

    /// Order cross-file global-augmentation groups by their owning program file
    /// index (source file name as a stable tiebreaker) so a downstream
    /// order-sensitive fold produces a deterministic merged-interface identity.
    ///
    /// The groups arrive keyed on each arena's raw pointer address
    /// (`Arc::as_ptr(arena) as usize`), so iterating the map directly folds in
    /// memory-layout (ASLR/allocator) order. `combine_augmentation_with_lib`
    /// (via `merge_interface_types`) is order-sensitive: the merged interface's
    /// member and overload-signature order — and therefore its interned
    /// `TypeId` identity — depends on the fold order. An address-ordered fold
    /// splits one globally-augmented lib interface (e.g. `Document`,
    /// `HTMLElement`) into distinct identities across otherwise-identical runs,
    /// which later meet in a relation and mis-fire TS2345/TS2430. Sorting by the
    /// owning file index makes the fold follow tsc's program-declaration order
    /// regardless of memory layout.
    ///
    /// Shared by every global-augmentation fold site — `merge_global_augmentations`,
    /// `apply_self_global_augmentations` (`declare global`),
    /// `resolve_array_global_augmentation_property`, and
    /// `resolve_augmentation_property_by_name` — so they all order groups
    /// identically.
    pub(crate) fn order_cross_arena_augmentation_groups(
        &self,
        groups: FxHashMap<usize, (Arc<NodeArena>, Vec<NodeIndex>)>,
    ) -> Vec<(Arc<NodeArena>, Vec<NodeIndex>)> {
        let mut ordered: Vec<(Arc<NodeArena>, Vec<NodeIndex>)> = groups.into_values().collect();
        ordered.sort_by_cached_key(|(arena, _)| {
            (
                self.ctx
                    .get_file_idx_for_arena(arena.as_ref())
                    .unwrap_or(usize::MAX),
                arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_default(),
            )
        });
        ordered
    }

    pub(crate) fn combine_augmentation_with_lib(
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
        self.register_finalized_lib_body_for_def(name, ty, None);
    }

    /// Finalize against the exact lib definition selected while lowering.
    /// Falling back to the name index preserves callers that do not carry a
    /// symbol-owned `DefId` (notably post-resolution global augmentations).
    pub(crate) fn register_finalized_lib_body_for_def(
        &mut self,
        name: &str,
        ty: TypeId,
        selected_def_id: Option<tsz_solver::DefId>,
    ) {
        // A selected per-arena definition is authoritative only for deciding
        // whether lowering returned that definition's public `Lazy(DefId)`
        // identity. Final structural bodies still publish through the stable
        // canonical name entry: parallel source-file checkers can select
        // equivalent lib declarations from different arenas, and letting that
        // selection choose the finalized body makes cross-file consumers depend
        // on worker completion order.
        if selected_def_id.is_some_and(|def_id| {
            crate::query_boundaries::lib_augmentations::is_lazy_def_identity(
                self.ctx.types,
                ty,
                def_id,
            )
        }) {
            return;
        }
        let name_atom = self.ctx.types.intern_string(name);
        let def_id = self
            .ctx
            .definition_store
            .find_defs_by_name(name_atom)
            .and_then(|defs| defs.first().copied());
        let Some(def_id) = def_id else {
            return;
        };
        if crate::query_boundaries::lib_augmentations::is_lazy_def_identity(
            self.ctx.types,
            ty,
            def_id,
        ) {
            return;
        }
        self.ctx.definition_store.register_type_to_def(ty, def_id);
        let existing_body = self.ctx.definition_store.get_body(def_id);
        if existing_body == Some(ty) {
            return;
        }
        // Monotone-membership publication for lib interface bodies.
        //
        // Lib interface bodies publish to the program-shared `DefinitionStore`
        // and the per-file `type_env` last-writer-wins (the publish-once freeze
        // at `resolve_lib_type_by_name`'s tail is opt-in/default-off because it
        // regresses `declare global` augmentation cases). A later checker can
        // re-derive a HERITAGE-THIN body for the same def: an iterator interface
        // momentarily resolved with a heritage-thin base drops the inherited
        // members (e.g. `SetIterator`/`MapIterator` lose `Iterator.next` reached
        // through `IteratorObject`). Once that thin form clobbers the complete
        // one, a consumer materializes against it — `set.values()` becomes a
        // `next`-less `SetIterator`, drawing false TS2741/TS2322 against
        // `IterableIterator` (#13942), and the DOM `Node`/`Element` diamond
        // oscillates the same way (#13862/#12299).
        //
        // The published body must only ever GAIN members for a given def, never
        // lose them: a body whose property set is a STRICT SUBSET of the body
        // already published is a heritage-thinning re-derivation and is rejected
        // (we keep and re-mirror the existing, more-complete body). Heritage
        // completion (thin already published, complete arriving) is a superset
        // and still wins; augmentation only adds members and still wins; same
        // member set with refined member types still replaces. The check is
        // order-independent — the membership-maximal body wins regardless of
        // which checker finalizes first.
        //
        // This guards the FINALIZE entry point, which intentionally bypasses the
        // store's deferred-publish drop (`set_body_with_params_impl`); the two
        // guards are complementary — the solver's deferred flag drops thin
        // NON-finalize re-publications, this drops thin finalize re-publications.
        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        let keep_existing = existing_body.is_some_and(|prev| {
            crate::query_boundaries::lib_augmentations::lib_body_strictly_loses_members(
                self.ctx.types,
                prev,
                ty,
            )
        });
        let published = if keep_existing {
            existing_body.unwrap_or(ty)
        } else {
            self.ctx.publish_finalized_definition_body(
                def_id,
                ty,
                (!type_params.is_empty()).then(|| type_params.clone()),
            )
        };
        self.ctx
            .register_def_auto_params_in_envs(def_id, published, type_params);
    }

    /// Mutation-isolation campaign: freeze `name`'s lib def body in the
    /// shared store after a **cleanly completed** resolution (not
    /// heritage-incomplete, not locally augmented), so later checkers'
    /// re-finalizations cannot republish a different (checker-relative) form.
    ///
    /// Freezing must only happen at clean completion: the incomplete-heritage
    /// recovery path (#12299) intentionally re-resolves the name and
    /// *overwrites* the def body with the flattened form — freezing the
    /// incomplete body would suppress that recovery and drop inherited
    /// members (false `TS2339`).
    pub(crate) fn freeze_finalized_lib_def(&mut self, name: &str) {
        if !lib_def_finalize_freeze_enabled() {
            return;
        }
        // Declaration emit is out of scope for the freeze: emit nameability
        // analysis (TS7056 and friends) is sensitive to which checker's lib
        // body TypeId the shared store carries, and freezing the first
        // checker's form regresses tsc parity there. The campaign target is
        // the checking pipeline (parallel gate-lift); the emit lane keeps
        // last-writer-wins semantics until emit type identity is owned.
        if self.ctx.emit_declarations() {
            return;
        }
        let name_atom = self.ctx.types.intern_string(name);
        if let Some(defs) = self.ctx.definition_store.find_defs_by_name(name_atom)
            && let Some(&def_id) = defs.first()
        {
            self.ctx.definition_store.mark_publish_once(def_id);
        }
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
        if !crate::query_boundaries::lib_augmentations::type_id_is_known_to_db(
            self.ctx.types,
            type_id,
        ) {
            return false;
        }
        for &def_id in self.ctx.collect_lazy_def_ids_cached(type_id).iter() {
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

        crate::query_boundaries::lib_augmentations::has_construct_signatures(
            self.ctx.types,
            type_id,
        )
    }
}

#[cfg(test)]
mod finalized_lib_body_identity_tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_solver::def::DefinitionInfo;

    #[test]
    fn selected_same_name_lazy_identity_does_not_rewrite_or_bump_generation() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "identity.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let canonical = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let sibling = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));

        let sibling_ref = types.lazy(sibling);
        let generation = checker.ctx.definition_store.generation();
        checker.register_finalized_lib_body_for_def("AliasToken", sibling_ref, Some(sibling));

        assert_eq!(checker.ctx.definition_store.generation(), generation);
        assert_eq!(
            checker.ctx.definition_store.get_body(canonical),
            Some(TypeId::STRING),
            "a same-name sibling wrapper is its own public identity, not a body for the first def",
        );
        assert_eq!(
            checker.ctx.definition_store.get_body(sibling),
            Some(TypeId::NUMBER),
            "finalization must preserve the sibling's already-published structural body",
        );
    }

    #[test]
    fn selected_same_name_structural_body_still_finalizes_canonical_definition() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "structural.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let canonical = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let sibling = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));

        checker.register_finalized_lib_body_for_def("AliasToken", TypeId::BOOLEAN, Some(sibling));

        assert_eq!(
            checker.ctx.definition_store.get_body(canonical),
            Some(TypeId::BOOLEAN),
            "structural lib bodies must publish through the stable canonical name entry",
        );
        assert_eq!(
            checker.ctx.definition_store.get_body(sibling),
            Some(TypeId::NUMBER),
            "a worker-local selected sibling must not become the structural finalization target",
        );
    }

    #[test]
    fn same_basename_distinct_lazy_target_remains_a_real_alias_chain() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "same-basename-chain.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let source = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let target = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));
        let target_ref = types.lazy(target);

        checker.register_finalized_lib_body_for_def("AliasToken", target_ref, Some(source));

        assert_eq!(
            checker.ctx.definition_store.get_body(source),
            Some(target_ref),
            "a same-basename but distinct lazy target is a valid alias chain",
        );
    }
}
