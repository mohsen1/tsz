//! Decide whether a property-access receiver must be materialized through the
//! env evaluator before member lookup.
//!
//! The env evaluator substitutes a generic application's type arguments into the
//! interface members; the lighter `evaluate_application_type` leaves a
//! cross-arena (re-exported) generic interface application opaque, so a member
//! read resolved to the unsubstituted declared member (a free `T`, false
//! TS2322). Split out of `resolve.rs` to keep that file under its size ratchet.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Whether the receiver `original_object_type` of a property access whose
    /// property name node is `name_idx` should be evaluated through the env
    /// evaluator (materializing generic applications, conditionals, and indexed
    /// accesses) rather than the lighter application evaluator.
    ///
    /// Fires for a *generic interface application* over a non-lib (program) def
    /// — e.g. `b: Box<number>` where `Box` is user-declared, including when
    /// reached through a barrel re-export — so its type arguments substitute
    /// into the members before lookup. Restricted to `DefKind::Interface`:
    /// mapped types and other type aliases lower to `TypeAlias` defs whose
    /// application already resolves through the lighter path, and routing them
    /// through the env evaluator over-normalizes them (regressing
    /// optional-method / mapped-type member resolution). This generalizes a
    /// prior property-name-gated special case (which only fired for `.select`)
    /// to any non-lib generic application receiver; the `.select` arm is kept
    /// for its conditional / indexed-access receiver coverage. Refs #13212 /
    /// #10663.
    pub(crate) fn receiver_needs_env_materialization(
        &self,
        original_object_type: TypeId,
        name_idx: NodeIndex,
    ) -> bool {
        let is_builder_select_access = self
            .ctx
            .arena
            .get(name_idx)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some_and(|prop_ident| prop_ident.escaped_text == "select");
        let receiver_is_nonlib_generic_application =
            crate::query_boundaries::common::get_application_lazy_def_id(
                self.ctx.types,
                original_object_type,
            )
            .filter(|&def_id| {
                matches!(
                    self.ctx.definition_store.get_kind(def_id),
                    Some(tsz_solver::def::DefKind::Interface)
                )
            })
            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
            .is_some_and(|sym_id| !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id));
        if !is_builder_select_access && !receiver_is_nonlib_generic_application {
            return false;
        }
        let receiver_fallback_def = crate::query_boundaries::common::get_application_lazy_def_id(
            self.ctx.types,
            original_object_type,
        )
        .or_else(|| {
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, original_object_type)
        });
        receiver_fallback_def
            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
            .is_some_and(|sym_id| !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id))
            || crate::query_boundaries::common::is_conditional_type(
                self.ctx.types,
                original_object_type,
            )
            || crate::query_boundaries::common::index_access_types(
                self.ctx.types,
                original_object_type,
            )
            .is_some()
    }
}
