use crate::context::CheckerContext;
use tsz_solver::TypeId;
use tsz_solver::def::{DefId, DefKind};

impl CheckerContext<'_> {
    /// Whether `body` is only a registration-window placeholder for a
    /// non-program type alias, rather than progress beyond that alias's public
    /// identity.
    ///
    /// Standard-library alias lowering returns `Lazy(DefId)` while publishing
    /// the structural body as a side effect. A later cross-file lookup can
    /// recover `UNKNOWN` or that self-lazy identity from its local symbol
    /// state. Neither candidate may be mirrored back into the shared
    /// `DefId -> body` slot: doing so makes publication non-monotone and turns
    /// already-materialized utility aliases opaque again.
    ///
    /// The candidate, kind, and origin gates are O(1). Ordinary program aliases
    /// (including program `.d.ts` inputs whose body genuinely is `unknown`) are
    /// deliberately excluded, and no name lookup or cache is added.
    pub(crate) fn is_non_progress_non_program_alias_body(
        &self,
        def_id: DefId,
        body: TypeId,
    ) -> bool {
        if body != TypeId::UNKNOWN
            && !crate::query_boundaries::definition_identity::is_lazy_def_identity(
                self.types, body, def_id,
            )
        {
            return false;
        }
        self.definition_store.get_kind(def_id) == Some(DefKind::TypeAlias)
            && self.definition_store.def_is_non_program(def_id)
    }

    /// Select the body that a checker-owned environment bridge may register.
    ///
    /// A cross-file symbol lookup can still return a non-progress placeholder
    /// after the canonical alias body was materialized. Keep the symbol result
    /// intact, but make its `DefId` cache monotone by reusing the published body.
    /// If materialization has not completed yet, defer the `DefId` write.
    pub(crate) fn definition_body_for_env_registration(
        &self,
        def_id: DefId,
        candidate: TypeId,
    ) -> Option<TypeId> {
        if !self.is_non_progress_non_program_alias_body(def_id, candidate) {
            return Some(candidate);
        }

        self.definition_store.get_body(def_id).filter(|&body| {
            body != TypeId::ERROR && !self.is_non_progress_non_program_alias_body(def_id, body)
        })
    }

    /// Publish `body` and its dependency set when it is canonical progress.
    /// Returns `false` when the candidate is a rejected registration-window
    /// placeholder or the store does not retain that exact body.
    pub(crate) fn publish_definition_body(&self, def_id: DefId, body: TypeId) -> bool {
        if self.is_non_progress_non_program_alias_body(def_id, body) {
            return false;
        }
        self.definition_store.set_body(def_id, body);
        self.record_definition_body_dependencies_if_published(def_id, body)
    }

    /// Parameterized counterpart of [`Self::publish_definition_body`], with
    /// the same exact-publication return contract.
    pub(crate) fn publish_definition_body_with_params(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> bool {
        if self.is_non_progress_non_program_alias_body(def_id, body) {
            return false;
        }
        self.definition_store
            .set_body_with_params(def_id, body, Some(params));
        self.record_definition_body_dependencies_if_published(def_id, body)
    }

    pub(crate) fn publish_finalized_definition_body(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Option<Vec<tsz_solver::TypeParamInfo>>,
    ) -> TypeId {
        self.definition_store
            .set_body_finalized(def_id, body, params);
        let published = self.definition_store.get_body(def_id).unwrap_or(body);
        self.record_definition_body_dependencies_if_published(def_id, body);
        published
    }

    fn record_definition_body_dependencies_if_published(
        &self,
        def_id: DefId,
        body: TypeId,
    ) -> bool {
        if self.definition_store.get_body(def_id) != Some(body) {
            return false;
        }
        self.definition_store.set_body_dependency_defs(
            def_id,
            self.collect_lazy_def_ids_cached(body).iter().copied(),
        );
        true
    }
}
