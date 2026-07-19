//! Checker boundary for stable exact-identity graph-rewrite sessions.

use tsz_solver::TypeId;
use tsz_solver::construction::QueryDatabase;
#[cfg(test)]
use tsz_solver::construction::TypeDatabase;

/// Completed exact-identity rewrite session retained for structural reuse.
///
/// The solver-owned memo stays opaque to checker orchestration. One session is
/// reused for every root in the same binder scope, preserving shared subgraphs
/// and nested fresh-binder identities while refreshing late provenance.
#[derive(Clone)]
pub(crate) struct ExactTypeRewriteSession {
    memo: tsz_solver::computation::ExactRewriteMemo,
}

impl ExactTypeRewriteSession {
    /// Rewrite another root through the retained structural map. An aborted
    /// attempt leaves the session reusable and does not cache that root.
    pub(crate) fn rewrite_root(
        &mut self,
        db: &dyn QueryDatabase,
        type_id: TypeId,
    ) -> Option<TypeId> {
        self.memo.rewrite_root(db, type_id).ok()
    }
}

/// Start a rewrite session. An aborted first walk is reported as `None` and
/// must not be cached by checker orchestration.
pub(crate) fn start_session(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    from: &[TypeId],
    to: &[TypeId],
) -> Option<(TypeId, ExactTypeRewriteSession)> {
    let (result, memo) =
        tsz_solver::computation::substitute_exact_types_with_memo(db, type_id, from, to).ok()?;
    Some((result, ExactTypeRewriteSession { memo }))
}

#[cfg(test)]
pub(crate) fn function_parameter_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    parameter_index: usize,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_function_shape(db, type_id).and_then(|shape| {
        shape
            .params
            .get(parameter_index)
            .map(|parameter| parameter.type_id)
    })
}

#[cfg(test)]
pub(crate) fn array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}
