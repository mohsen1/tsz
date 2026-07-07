use crate::evaluation::request::EvaluationRequest;
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
        let request = EvaluationRequest::new(type_id)
            .with_no_unchecked_indexed_access(self.no_unchecked_indexed_access)
            .with_exact_optional_property_types(self.exact_optional_property_types)
            .with_type_database_identity(self.interner.type_database_identity())
            .with_resolver_identity(self.resolver.resolver_identity())
            .with_resolver_generation(self.resolver.resolver_generation());
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

        let memo_result = if let Some(session) = self.eval_session {
            self.evaluate_type_with_session(request, session)
        } else {
            with_current_session(|session| self.evaluate_type_with_session(request, session))
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::relations::subtype::TypeResolver;
    use crate::types::{PropertyInfo, TypeData};
    use std::cell::Cell;

    struct GenerationBodyResolver {
        generation: u64,
        body: TypeId,
        calls: Cell<u32>,
    }

    struct MutableGenerationBodyResolver {
        generation: Cell<u64>,
        body: Cell<TypeId>,
        calls: Cell<u32>,
    }

    impl TypeResolver for GenerationBodyResolver {
        fn resolver_generation(&self) -> u64 {
            self.generation
        }

        fn resolve_ref(
            &self,
            _symbol: crate::types::SymbolRef,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(
            &self,
            _def_id: DefId,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            self.calls.set(self.calls.get() + 1);
            Some(self.body)
        }
    }

    impl TypeResolver for MutableGenerationBodyResolver {
        fn resolver_generation(&self) -> u64 {
            self.generation.get()
        }

        fn resolve_ref(
            &self,
            _symbol: crate::types::SymbolRef,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            None
        }

        fn resolve_lazy(
            &self,
            _def_id: DefId,
            _interner: &dyn crate::construction::TypeDatabase,
        ) -> Option<TypeId> {
            self.calls.set(self.calls.get() + 1);
            Some(self.body.get())
        }
    }

    #[test]
    fn relation_evaluation_session_memo_partitions_by_resolver_generation() {
        let interner = TypeInterner::new();
        let lazy = interner.intern(TypeData::Lazy(DefId(701)));
        let session = EvaluationSession::new();

        let resolver_one = GenerationBodyResolver {
            generation: 1,
            body: TypeId::STRING,
            calls: Cell::new(0),
        };
        let resolver_two = GenerationBodyResolver {
            generation: 2,
            body: TypeId::NUMBER,
            calls: Cell::new(0),
        };

        let mut first = SubtypeChecker::with_resolver(&interner, &resolver_one)
            .with_evaluation_session(&session);
        assert_eq!(
            first.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(resolver_one.calls.get(), 1);

        let mut first_again = SubtypeChecker::with_resolver(&interner, &resolver_one)
            .with_evaluation_session(&session);
        assert_eq!(
            first_again.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(
            resolver_one.calls.get(),
            1,
            "same resolver generation should hit the session memo"
        );

        let mut second = SubtypeChecker::with_resolver(&interner, &resolver_two)
            .with_evaluation_session(&session);
        assert_eq!(
            second.evaluate_type_with_stability(lazy).type_id(),
            TypeId::NUMBER
        );
        assert_eq!(
            resolver_two.calls.get(),
            1,
            "different resolver generation must not reuse the first resolver's memo"
        );
    }

    #[test]
    fn relation_evaluation_session_memo_partitions_by_resolver_identity() {
        let interner = TypeInterner::new();
        let lazy = interner.intern(TypeData::Lazy(DefId(703)));
        let session = EvaluationSession::new();

        let resolver_one = GenerationBodyResolver {
            generation: 1,
            body: TypeId::STRING,
            calls: Cell::new(0),
        };
        let resolver_two = GenerationBodyResolver {
            generation: 1,
            body: TypeId::NUMBER,
            calls: Cell::new(0),
        };

        let mut first = SubtypeChecker::with_resolver(&interner, &resolver_one)
            .with_evaluation_session(&session);
        assert_eq!(
            first.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(resolver_one.calls.get(), 1);

        let mut first_again = SubtypeChecker::with_resolver(&interner, &resolver_one)
            .with_evaluation_session(&session);
        assert_eq!(
            first_again.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(
            resolver_one.calls.get(),
            1,
            "same resolver identity and generation should hit the session memo"
        );

        let mut second = SubtypeChecker::with_resolver(&interner, &resolver_two)
            .with_evaluation_session(&session);
        assert_eq!(
            second.evaluate_type_with_stability(lazy).type_id(),
            TypeId::NUMBER
        );
        assert_eq!(
            resolver_two.calls.get(),
            1,
            "same generation on a different resolver must not reuse the first resolver's memo"
        );
    }

    #[test]
    fn relation_evaluation_session_memo_partitions_by_type_arena() {
        let interner_one = TypeInterner::new();
        let interner_two = TypeInterner::new();
        let session = EvaluationSession::new();

        let name_one = interner_one.intern_string("alpha");
        let object_one = interner_one.object(vec![PropertyInfo::new(name_one, TypeId::STRING)]);
        let key_one = interner_one.literal_string("alpha");

        let name_two = interner_two.intern_string("alpha");
        let object_two = interner_two.object(vec![PropertyInfo::new(name_two, TypeId::NUMBER)]);
        let key_two = interner_two.literal_string("alpha");
        assert_eq!(
            object_one, object_two,
            "same numeric object TypeId in two arenas should still be keyed separately"
        );
        assert_eq!(
            key_one, key_two,
            "same string literal key TypeId in two arenas should still be keyed separately"
        );

        let indexed_one = interner_one.index_access(object_one, key_one);
        let indexed_two = interner_two.index_access(object_two, key_two);
        assert_eq!(
            indexed_one, indexed_two,
            "same numeric TypeId in two arenas should still be keyed separately"
        );

        let mut first = SubtypeChecker::new(&interner_one).with_evaluation_session(&session);
        assert_eq!(
            first.evaluate_type_with_stability(indexed_one).type_id(),
            TypeId::STRING
        );

        let mut second = SubtypeChecker::new(&interner_two).with_evaluation_session(&session);
        assert_eq!(
            second.evaluate_type_with_stability(indexed_two).type_id(),
            TypeId::NUMBER,
            "same numeric TypeId in a different arena must not reuse the first arena's memo"
        );
    }

    #[test]
    fn relation_local_eval_cache_partitions_by_resolver_generation() {
        let interner = TypeInterner::new();
        let lazy = interner.intern(TypeData::Lazy(DefId(702)));
        let session = EvaluationSession::new();
        let resolver = MutableGenerationBodyResolver {
            generation: Cell::new(1),
            body: Cell::new(TypeId::STRING),
            calls: Cell::new(0),
        };

        let mut checker =
            SubtypeChecker::with_resolver(&interner, &resolver).with_evaluation_session(&session);
        assert_eq!(
            checker.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(resolver.calls.get(), 1);
        assert_eq!(checker.eval_cache.len(), 1);

        assert_eq!(
            checker.evaluate_type_with_stability(lazy).type_id(),
            TypeId::STRING
        );
        assert_eq!(
            resolver.calls.get(),
            1,
            "same checker and generation should hit the relation-local eval cache"
        );
        assert_eq!(checker.eval_cache.len(), 1);

        resolver.generation.set(2);
        resolver.body.set(TypeId::NUMBER);
        assert_eq!(
            checker.evaluate_type_with_stability(lazy).type_id(),
            TypeId::NUMBER
        );
        assert_eq!(
            resolver.calls.get(),
            2,
            "changed resolver generation must miss the relation-local eval cache"
        );
        assert_eq!(checker.eval_cache.len(), 2);
    }
}
