use crate::context::CheckerContext;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerContext<'_> {
    pub(crate) fn publish_definition_body(&self, def_id: DefId, body: TypeId) -> bool {
        self.definition_store.set_body(def_id, body);
        self.record_definition_body_dependencies_if_published(def_id, body)
    }

    pub(crate) fn publish_definition_body_with_params(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> bool {
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
