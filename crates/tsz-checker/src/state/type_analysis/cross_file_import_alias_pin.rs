//! Import-alias pinning for cross-file symbol lookup.
//!
//! Per-file binders mint raw `SymbolId`s starting from zero, so the same
//! integer id can name a local `import ... from "./m"` alias in one file and
//! an unrelated export in `./m`. When the consuming file looks up that id via
//! `get_symbol_globally` / `get_cross_file_symbol`, the
//! `cross_file_symbol_targets` overlay routes the request to the source
//! file's binder. If we look there blindly we may pick up *whatever decl*
//! happens to share the raw id rather than the alias itself, and downstream
//! type computation collapses (for example, `import { instance }` can collapse
//! to `typeof instance` and drop the imported class's heritage).
//!
//! The alias's actual target is resolved through the import chain in
//! `compute_type_of_symbol_type_alias_variable_alias`, so the alias symbol
//! must stay anchored to the current binder when one is present. This applies
//! to named, default, namespace, and type-only imports because all are local
//! alias symbols; the imported target belongs to the source module.
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};

impl<'a> CheckerState<'a> {
    /// Local import alias (`ALIAS` + `import_module`) at `sym_id`, if any.
    pub(crate) fn local_import_alias(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        let local = self.ctx.binder.get_symbol(sym_id)?;
        (local.has_any_flags(symbol_flags::ALIAS) && local.import_module().is_some())
            .then_some(local)
    }

    /// `true` when a symbol read out of the *declaring* file's binder at the
    /// raw id `sym_id` is genuinely the entity a local import alias at that
    /// same id refers to.
    ///
    /// `get_symbol_from_registered_file_target` answers the right *file* — the
    /// overlay records where the alias's target lives — but then indexes that
    /// file's binder with the **consuming** file's raw `SymbolId`. Raw ids are
    /// minted per binder from zero with no `base_offset`, so the read lands on
    /// whichever declaration of the declaring file happens to sit at that
    /// ordinal. It is right only by coincidence, when the imported entity is
    /// also that file's Nth declaration.
    ///
    /// The imported name is what ties the two ends together, so require it to
    /// match: `import { Shape }` may only accept a declaring-file symbol named
    /// `Shape`, and `import { Shape as S }` likewise (the alias's
    /// `import_name` is the module-side name, `escaped_name` the local one).
    /// When `sym_id` is not a local import alias there is no alias for the
    /// read to disagree with and it stands unchanged.
    pub(crate) fn registered_file_target_matches_import_alias(
        &self,
        sym_id: SymbolId,
        candidate: &tsz_binder::Symbol,
    ) -> bool {
        let Some(alias) = self.local_import_alias(sym_id) else {
            return true;
        };
        let imported_name = alias.import_name().unwrap_or(alias.escaped_name.as_str());
        candidate.escaped_name == imported_name
    }
}
