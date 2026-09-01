//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/functions/checking/evaluation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4d70a97ddfc842fa981bcf7ebd7714e6c5bf58bff590223a9aa8e90fc396a599 185 relation_evaluation_session_memo_partitions_by_resolver_generation
    #[test]
    fn relation_evaluation_session_memo_partitions_by_resolver_generation() {
        let interner = TypeInterner::new();
        let lazy = interner.lazy(DefId(701));
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
// TSZ_INLINE_TEST_END 4d70a97ddfc842fa981bcf7ebd7714e6c5bf58bff590223a9aa8e90fc396a599

// TSZ_INLINE_TEST_BEGIN 33e2b30bb9800a7bc29e3dbb1bc0fa88b5f3dbcade4bb418c59ccf600c91461f 235 relation_evaluation_session_memo_partitions_by_resolver_identity
    #[test]
    fn relation_evaluation_session_memo_partitions_by_resolver_identity() {
        let interner = TypeInterner::new();
        let lazy = interner.lazy(DefId(703));
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
// TSZ_INLINE_TEST_END 33e2b30bb9800a7bc29e3dbb1bc0fa88b5f3dbcade4bb418c59ccf600c91461f

// TSZ_INLINE_TEST_BEGIN c3b43026d7e517d0376f7997355d61435972ba1af915a7fb637b276f289de07e 285 relation_evaluation_session_memo_partitions_by_type_arena
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
// TSZ_INLINE_TEST_END c3b43026d7e517d0376f7997355d61435972ba1af915a7fb637b276f289de07e

// TSZ_INLINE_TEST_BEGIN e3fd7b6e33766e6bee5bd3ab7fdb85ae68036853ed51a94863e6acda43eef4b0 328 relation_local_eval_cache_partitions_by_resolver_generation
    #[test]
    fn relation_local_eval_cache_partitions_by_resolver_generation() {
        let interner = TypeInterner::new();
        let lazy = interner.lazy(DefId(702));
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
// TSZ_INLINE_TEST_END e3fd7b6e33766e6bee5bd3ab7fdb85ae68036853ed51a94863e6acda43eef4b0
