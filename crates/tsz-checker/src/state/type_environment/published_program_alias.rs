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
        let body = self.ctx.definition_store.get_body(def_id)?;
        (body != TypeId::ERROR && lazy_def_id(self.ctx.types, body) != Some(def_id)).then_some(body)
    }
}
