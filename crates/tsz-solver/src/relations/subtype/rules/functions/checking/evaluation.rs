use crate::evaluation::session::{EvaluationSession, with_current_session};
use crate::relations::subtype::{RelationEvaluationResult, SubtypeChecker, TypeResolver};
use crate::types::TypeId;

impl<R: TypeResolver> SubtypeChecker<'_, R> {
    /// Evaluate a meta-type (conditional, index access, mapped, keyof, etc.) to its
    /// concrete form. Uses `TypeEvaluator` with the resolver to correctly resolve
    /// `Lazy(DefId)` types at all nesting levels (e.g., `KeyOf(Lazy(DefId))`).
    ///
    /// Always uses `TypeEvaluator` with the resolver instead of `query_db.evaluate_type()`
    /// because the checker populates `DefId` -> `TypeId` mappings in the
    /// `TypeEnvironment` that the `query_db`'s resolver-less evaluator cannot access.
    ///
    /// Results are cached in `eval_cache` to avoid re-evaluating the same type across
    /// multiple subtype checks. This turns O(n^2) evaluate calls into O(n).
    pub(crate) fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_stability(type_id).type_id()
    }

    /// Like [`Self::evaluate_type`], but also reports whether the evaluation is
    /// *stable*: it converged without tripping any recursion/depth/budget limit
    /// or cross-instance cycle bail.
    ///
    /// Callers that special-case collapsed `unknown` results must consult the
    /// flag so a genuine `unknown` is not treated like a bail artifact.
    pub(crate) fn evaluate_type_with_stability(
        &mut self,
        type_id: TypeId,
    ) -> RelationEvaluationResult {
        if type_id.is_intrinsic() {
            return RelationEvaluationResult::stable(type_id);
        }
        let cache_key = (type_id, self.no_unchecked_indexed_access);
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        if let Some(fuel) = self.explain_eval_fuel.as_mut() {
            if *fuel == 0 {
                return RelationEvaluationResult::unstable(type_id);
            }
            *fuel -= 1;
        }

        let no_unchecked_indexed_access = self.no_unchecked_indexed_access;
        let memo_result = if let Some(session) = self.eval_session {
            self.evaluate_type_with_session(type_id, no_unchecked_indexed_access, session)
        } else {
            with_current_session(|session| {
                self.evaluate_type_with_session(type_id, no_unchecked_indexed_access, session)
            })
        };
        let Some(memo_result) = memo_result else {
            return RelationEvaluationResult::unstable(type_id);
        };
        let entry = RelationEvaluationResult::from_depth_agnostic_memo(memo_result);
        self.eval_cache.insert(cache_key, entry);
        entry
    }

    fn evaluate_type_with_session(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
        session: &EvaluationSession,
    ) -> Option<crate::evaluation::result::EvaluationMemoResult> {
        use crate::evaluation::cross_eval_guard;
        use crate::evaluation::evaluate::TypeEvaluator;

        cross_eval_guard::memoized_eval_with_stability(
            session,
            type_id,
            no_unchecked_indexed_access,
            || {
                let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
                if let Some(db) = self.query_db {
                    evaluator = evaluator.with_query_db(db);
                }
                let request = crate::evaluation::request::EvaluationRequest::new(type_id)
                    .with_no_unchecked_indexed_access(no_unchecked_indexed_access);
                evaluator.evaluate_request_memo_result(request)
            },
        )
    }
}
