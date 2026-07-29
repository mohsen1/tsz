//! Authoritative published bodies for owner-qualified program aliases.

use crate::query_boundaries::common::lazy_def_id;
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::def::{DefId, DefKind, DefinitionStore};

impl CheckerState<'_> {
    /// Return the concrete body of an owner-qualified program type alias.
    ///
    /// Once such a body is published under an exact `DefId`, relation
    /// readiness must not demote that identity back to a raw `SymbolId`.
    /// Per-file binders reuse raw ids, so the demoted lookup can resolve an
    /// unrelated declaration and publish its body over the exact def.
    /// Lib/synthetic definitions retain their dedicated materialization paths.
    pub(super) fn published_program_alias_body(&self, def_id: DefId) -> Option<TypeId> {
        let (file_id, kind, _) = self.ctx.definition_store.get_classification(def_id)?;
        if kind != DefKind::TypeAlias
            || file_id.is_none_or(|file_id| file_id == DefinitionStore::NON_PROGRAM_FILE_SENTINEL)
        {
            return None;
        }
        // `DefKind::TypeAlias` is also the placeholder kind minted for symbols
        // whose flags match no known declaration form (import aliases, type
        // parameters — see `get_or_create_def_id`'s default arm). A defaulted
        // placeholder's published body can be a value-side type (e.g. a class
        // constructor), and returning it skips the class-instance resolution
        // the demoted path performs. Refuse only on positive evidence: when
        // the def's symbol resolves and provably lacks `TYPE_ALIAS` flags.
        // Owner-qualified defs whose symbol identity is unavailable keep the
        // fast path — that exactness is what this publication exists for.
        if let Some((sym_id, owner_file_idx)) = self.ctx.def_symbol_identity(def_id)
            && let Some(symbol) = owner_file_idx
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                .and_then(|binder| binder.get_symbol(sym_id))
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
            && !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
        {
            return None;
        }
        let body = self.ctx.definition_store.get_body(def_id)?;
        (body != TypeId::ERROR && lazy_def_id(self.ctx.types, body) != Some(def_id)).then_some(body)
    }
}
