use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::session::EvaluationSession;
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
        let request = EvaluationRequest::new(type_id)
            .with_no_unchecked_indexed_access(self.no_unchecked_indexed_access)
            .with_exact_optional_property_types(self.exact_optional_property_types);
        let cache_key = request.cache_key();
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        if let Some(fuel) = self.explain_eval_fuel.as_mut() {
            if *fuel == 0 {
                return RelationEvaluationResult::unstable(type_id);
            }
            *fuel -= 1;
        }

        let memo_result =
            crate::evaluation::session::with_session_or_current(self.eval_session, |session| {
                self.evaluate_type_with_session(request, session)
            });
        let Some(memo_result) = memo_result else {
            return RelationEvaluationResult::unstable(type_id);
        };
        // #14346 verdict consumption: a guard-truncated evaluation makes any
        // relation verdict computed from it a budget artifact. Note the event
        // on the checker-local taint counter so `record_definitive_verdict`
        // and maybe-key promotion keep the enclosing frames out of the
        // relation caches.
        if memo_result.is_incomplete_termination() {
            self.note_incomplete_evaluation_relation_event();
        }
        let entry = RelationEvaluationResult::from_depth_agnostic_memo(memo_result);
        if entry.is_stable_for_depth_agnostic_cache() {
            self.eval_cache.insert(cache_key, entry);
        }
        entry
    }

    /// Resolver-backed evaluation with a resolver-less raw fallback, used by
    /// function-shape recovery: when [`Self::evaluate_type`] leaves the type
    /// unchanged, retry without the resolver and keep whichever form moved.
    pub(crate) fn evaluate_type_or_raw_fallback(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_type(type_id);
        if evaluated != type_id {
            return evaluated;
        }
        self.raw_fallback_evaluate(type_id)
    }

    /// Resolver-less raw evaluation fallback. Applies the same #14346 taint
    /// discipline as the primary seat: a guard-truncated walk notes the
    /// checker-local event before collapsing to a `TypeId`.
    fn raw_fallback_evaluate(&self, type_id: TypeId) -> TypeId {
        let result = crate::evaluation::evaluate::evaluate_type_result_with_request(
            self.interner,
            EvaluationRequest::new(type_id)
                .with_exact_optional_property_types(self.interner.exact_optional_property_types()),
        );
        if result.is_incomplete() {
            self.note_incomplete_evaluation_relation_event();
        }
        result.into_type_id()
    }

    fn evaluate_type_with_session(
        &self,
        request: EvaluationRequest,
        session: &EvaluationSession,
    ) -> Option<crate::evaluation::result::EvaluationMemoResult> {
        use crate::evaluation::cross_eval_guard;
        use crate::evaluation::evaluate::TypeEvaluator;

        cross_eval_guard::memoized_eval_with_stability(session, request, || {
            let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver)
                .with_evaluation_session(session);
            if let Some(db) = self.query_db {
                evaluator = evaluator.with_query_db(db);
            }
            evaluator.evaluate_request_memo_result(request)
        })
    }
}
