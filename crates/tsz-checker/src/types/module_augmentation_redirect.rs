//! Home-symbol redirect publication for cross-file augmented HKT registries
//! (issue #14344 / #14345).
//!
//! Sibling of `module_augmentation`: the augmentation merge site
//! (`apply_module_augmentations`) calls
//! [`CheckerState::publish_augmented_base_body_redirect`] once the merged body
//! is assembled for an EMPTY pre-merge base snapshot (the fp-ts `URItoKindN`
//! registry pattern). This module publishes that merged body under the home
//! interface's own `DefId` and records the home-symbol -> home-`DefId` redirect
//! edge so the solver's index-reduction consumer can map a frozen empty
//! `shape.symbol` back to the populated home def.
//!
//! Lives in a sibling file purely for the 2000-LOC checker boundary; the logic
//! belongs to the augmentation flow.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// #14344 / #14345: publish the merged augmented body under the home
    /// interface's `DefId` and record the home-symbol -> home-`DefId` redirect
    /// edge (`DefinitionStore::augmented_base_body_def_for_symbol`).
    ///
    /// Mirrors the self-global augmentation publication
    /// (`apply_self_global_augmentations`): writes the merged shape into the
    /// home def's body/instance shape so `get_body(home_def)` returns the merged
    /// members cross-arena, then records the redirect edge keyed on the raw home
    /// `SymbolId`. All writes are no-ops when
    /// `TSZ_AUGMENTED_BODY_SYMBOL_REDIRECT` is OFF (this function early-returns
    /// on the flag, and the store's `_if_enabled` guard skips the edge), so
    /// flag-OFF stays byte-identical.
    pub(crate) fn publish_augmented_base_body_redirect(
        &mut self,
        home_symbol: tsz_binder::SymbolId,
        merged_type: TypeId,
    ) {
        use crate::query_boundaries::state::type_environment;

        if !tsz_solver::def::augmented_body_symbol_redirect_enabled() {
            return;
        }

        let home_def_id = self.ctx.get_or_create_def_id(home_symbol);

        // Publish the merged body under the home def so `get_body(home_def_id)`
        // surfaces the merged members for the index-reduction redirect.
        self.ctx
            .definition_store
            .set_body_with_params(home_def_id, merged_type, None);
        if let Some(shape) = type_environment::object_shape(self.ctx.types, merged_type) {
            self.ctx
                .definition_store
                .set_instance_shape(home_def_id, shape);
        }
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            env.insert_def(home_def_id, merged_type);
        }

        // Record the redirect edge keyed on the raw home `SymbolId` (the same
        // value the frozen empty snapshot carries on `shape.symbol`).
        self.ctx
            .definition_store
            .register_augmented_base_body_def_if_enabled(home_symbol.0, home_def_id);
    }
}
