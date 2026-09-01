//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate/closed_eval.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 437715bfd96a74e8214e5c61f70da5e23b3623ff5289edfcdc42c7a133cebaba 467 limited_resolver_does_not_read_cached_meta_operations
    /// A limited-resolver evaluator (the checker's intentionally-partial
    /// first-pass `TypeEnvironment` evaluation) must *recompute* a cacheable
    /// `IndexAccess`/`KeyOf` rather than consume the authoritative pass's stored,
    /// fully-materialized result — consuming it across the partial/complete
    /// boundary poisons its in-flight inference (a `propTypeValidatorInference`
    /// style false `TS2322`). A non-limited (authoritative / plain query-backed)
    /// evaluator keeps reading the cache, so the meta-operation reuse is
    /// preserved. The cache key (`no_unchecked_indexed_access`/`exact_optional`)
    /// is supplied by the same `QueryCache` for both store and read, so a hit is
    /// exactly the stored value.
    #[test]
    fn limited_resolver_does_not_read_cached_meta_operations() {
        let interner = TypeInterner::new();
        // Two cacheable meta-operations over a plain (non-index-signature) object.
        let idx = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        let keyof = interner.keyof(TypeId::OBJECT);
        let cache = QueryCache::new(&interner);
        cache.insert_closed_eval_cache(idx, false, TypeId::NUMBER);
        cache.insert_closed_eval_cache(keyof, false, TypeId::BOOLEAN);

        // Non-limited evaluator: reads the authoritative stored result.
        let authoritative = TypeEvaluator::new(&cache);
        assert_eq!(
            authoritative.try_closed_eval_read(idx),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            authoritative.try_closed_eval_read(keyof),
            Some(TypeId::BOOLEAN)
        );

        // Limited-resolver evaluator: must not consume the materialized result.
        let limited = TypeEvaluator::new(&cache).with_limited_resolver();
        assert_eq!(limited.try_closed_eval_read(idx), None);
        assert_eq!(limited.try_closed_eval_read(keyof), None);
    }
// TSZ_INLINE_TEST_END 437715bfd96a74e8214e5c61f70da5e23b3623ff5289edfcdc42c7a133cebaba

// TSZ_INLINE_TEST_BEGIN 5476ced566d1e6fb47b2a4a928fd7537ce9d4fc2a3715093c7be073526e8dcfa 496 request_state_stability_gate_blocks_closed_eval_write
    /// A closed-eval write is publishable only when the shared request-state
    /// stability gate reports the result is complete and untainted.
    #[test]
    fn request_state_stability_gate_blocks_closed_eval_write() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);

        let complete_node = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        let mut complete = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        complete.cache.insert(complete_node, TypeId::NUMBER);
        let complete_snapshot = EvaluationCacheLimitSnapshot::capture(&cache);
        complete.commit_closed_eval_writes(complete_snapshot);
        assert_eq!(
            cache.lookup_closed_eval_cache(complete_node, false),
            Some(TypeId::NUMBER)
        );

        let incomplete_node = interner.keyof(TypeId::OBJECT);
        let mut incomplete = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        incomplete.cache.insert(incomplete_node, TypeId::BOOLEAN);
        incomplete.simulate_incomplete_request_verdict_for_test(TerminationKind::DepthExceeded);
        let incomplete_snapshot = EvaluationCacheLimitSnapshot::capture(&cache);
        incomplete.commit_closed_eval_writes(incomplete_snapshot);
        assert_eq!(cache.lookup_closed_eval_cache(incomplete_node, false), None);

        let legacy_tainted_node = interner.index_access(TypeId::STRING, TypeId::NUMBER);
        let mut legacy_tainted = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        legacy_tainted
            .cache
            .insert(legacy_tainted_node, TypeId::STRING);
        legacy_tainted.simulate_unrelated_recursion_bail_for_test();
        let legacy_tainted_snapshot = EvaluationCacheLimitSnapshot::capture(&cache);
        legacy_tainted.commit_closed_eval_writes(legacy_tainted_snapshot);
        assert_eq!(
            cache.lookup_closed_eval_cache(legacy_tainted_node, false),
            None
        );

        let unresolved_node = interner.index_access(TypeId::BOOLEAN, TypeId::STRING);
        let mut unresolved = TypeEvaluator::new(&cache)
            .with_query_db(&cache)
            .with_closed_eval_writes();
        unresolved.cache.insert(unresolved_node, TypeId::BOOLEAN);
        unresolved.mark_unresolved_def_seen();
        let unresolved_snapshot = EvaluationCacheLimitSnapshot::capture(&cache);
        unresolved.commit_closed_eval_writes(unresolved_snapshot);
        assert_eq!(cache.lookup_closed_eval_cache(unresolved_node, false), None);
    }
// TSZ_INLINE_TEST_END 5476ced566d1e6fb47b2a4a928fd7537ce9d4fc2a3715093c7be073526e8dcfa

// TSZ_INLINE_TEST_BEGIN a6125340af506b9ab75c45c6c955ad0e2c3b18176174c98cdbb17dc726f88233 552 unresolved_def_seen_is_reported_by_request_state_stability_gate
    /// An unresolved semantic body is a registration-window artifact, so the
    /// shared request-state stability verdict should name it before closed-eval
    /// decides whether to publish any cache entries.
    #[test]
    fn unresolved_def_seen_is_reported_by_request_state_stability_gate() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);

        assert_eq!(
            evaluator.request_state_cache_stability(),
            EvaluationRequestStability::Stable
        );

        evaluator.mark_unresolved_def_seen();

        assert_eq!(
            evaluator.request_state_cache_stability(),
            EvaluationRequestStability::UnresolvedDef
        );
        assert!(!evaluator.request_state_is_depth_agnostic_cache_stable());
    }
// TSZ_INLINE_TEST_END a6125340af506b9ab75c45c6c955ad0e2c3b18176174c98cdbb17dc726f88233

// TSZ_INLINE_TEST_BEGIN c3452350fdfc65adc64068318100fca888108d583ff30f27ba161fa054f334e3 576 request_stability_does_not_inherit_prior_unresolved_def_hit
    /// A run-wide unresolved-def hit blocks closed-eval writes for the whole
    /// top-level evaluation, but it must not poison later independent request
    /// memos. Otherwise a clean indexed-access request after an unrelated
    /// registration-window artifact can lose its stable cross-evaluator memo and
    /// re-resolve to a different method shape.
    #[test]
    fn request_stability_does_not_inherit_prior_unresolved_def_hit() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);

        evaluator.mark_unresolved_def_seen();
        assert_eq!(
            evaluator.run_state_cache_stability(),
            EvaluationRequestStability::UnresolvedDef
        );

        let result = evaluator.evaluate_request_result(
            crate::evaluation::request::EvaluationRequest::new(TypeId::STRING),
        );

        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert_eq!(
            evaluator.request_state_cache_stability(),
            EvaluationRequestStability::Stable
        );
        assert_eq!(
            evaluator.run_state_cache_stability(),
            EvaluationRequestStability::UnresolvedDef
        );
    }
// TSZ_INLINE_TEST_END c3452350fdfc65adc64068318100fca888108d583ff30f27ba161fa054f334e3

// TSZ_INLINE_TEST_BEGIN 91d71b216d1702a3e50ffd5fb94cdad6117e09fd87822669f36ffa73f8025d7b 605 cacheable_kinds_exclude_union_and_intersection
    /// The substitution-independent cache is eligible for `IndexAccess`/`KeyOf`
    /// meta-operations but never for `Union`/`Intersection` node inputs (caching
    /// a normalized cross-product could suppress `TS2590`).
    #[test]
    fn cacheable_kinds_exclude_union_and_intersection() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // IndexAccess over a plain concrete object operand is eligible.
        let idx = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        assert!(ev.is_closed_cacheable_kind(idx));

        // keyof over a plain concrete operand is eligible.
        let keyof = interner.keyof(TypeId::OBJECT);
        assert!(ev.is_closed_cacheable_kind(keyof));

        // Union / Intersection node inputs are never eligible.
        let union = interner.union2(TypeId::STRING, TypeId::NUMBER);
        let inter = interner.intersection(vec![TypeId::OBJECT, TypeId::STRING]);
        assert!(!ev.is_closed_cacheable_kind(union));
        assert!(!ev.is_closed_cacheable_kind(inter));

        // A primitive / plain object is not a meta-operation, so not eligible.
        assert!(!ev.is_closed_cacheable_kind(TypeId::STRING));
        assert!(!ev.is_closed_cacheable_kind(TypeId::OBJECT));
    }
// TSZ_INLINE_TEST_END 91d71b216d1702a3e50ffd5fb94cdad6117e09fd87822669f36ffa73f8025d7b

// TSZ_INLINE_TEST_BEGIN 2812f367ecbeb149cd67e07b5df6fd847c53c640d01492a3805090255af82ef4 633 cacheable_kinds_exclude_conditional_bearing_index_access
    /// An `IndexAccess`/`KeyOf` whose structure contains a `Conditional` is
    /// excluded — the conditional can bind `infer` against context the cache key
    /// does not capture. The check is name-agnostic (uses structure, not
    /// spellings).
    #[test]
    fn cacheable_kinds_exclude_conditional_bearing_index_access() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // A conditional `string extends number ? 1 : 2` interned as the index.
        let cond = interner.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::ANY,
            false_type: TypeId::UNKNOWN,
            is_distributive: false,
        });
        // IndexAccess whose index operand is a conditional → structure contains
        // a conditional → excluded.
        let idx_with_cond = interner.index_access(TypeId::OBJECT, cond);
        assert!(ev.body_has_conditional(idx_with_cond));
        assert!(!ev.is_closed_cacheable_kind(idx_with_cond));

        // The same shape without the conditional stays eligible.
        let idx_plain = interner.index_access(TypeId::OBJECT, TypeId::STRING);
        assert!(!ev.body_has_conditional(idx_plain));
        assert!(ev.is_closed_cacheable_kind(idx_plain));
    }
// TSZ_INLINE_TEST_END 2812f367ecbeb149cd67e07b5df6fd847c53c640d01492a3805090255af82ef4

// TSZ_INLINE_TEST_BEGIN 8fa060cb3f88a9af81718b6db6109685eb93c8c6f5b2468a029dc4cd942454f7 665 cacheable_kinds_terminate_on_cyclic_alias_index_access
    /// A `Lazy` alias whose resolved body re-references the same alias through an
    /// `IndexAccess` forms a multi-step cycle (`A -> A[K] -> A -> …`). Each step
    /// alternates `Lazy -> body` and `IndexAccess -> operand`, so the single-step
    /// `body != obj` check is satisfied at every hop and cannot break it. Such
    /// shapes arise from cross-file import cycles. The cache-eligibility walk must
    /// terminate (returning the conservative "not cacheable") instead of
    /// overflowing the stack.
    #[test]
    fn cacheable_kinds_terminate_on_cyclic_alias_index_access() {
        /// Resolver whose single alias body is supplied after construction so it
        /// can reference its own `Lazy` node (`A = A[string]`).
        struct CyclicAliasResolver {
            def_id: DefId,
            body: TypeId,
        }
        impl TypeResolver for CyclicAliasResolver {
            fn resolve_ref(
                &self,
                _symbol: crate::types::SymbolRef,
                _interner: &dyn crate::caches::db::TypeDatabase,
            ) -> Option<TypeId> {
                None
            }
            fn resolve_lazy(
                &self,
                def_id: DefId,
                _interner: &dyn crate::caches::db::TypeDatabase,
            ) -> Option<TypeId> {
                (def_id == self.def_id).then_some(self.body)
            }
        }

        let interner = TypeInterner::new();
        let def_id = DefId(7);
        let lazy = interner.lazy(def_id);
        // `A = A[string]`: an index access whose operand is the alias itself.
        let body = interner.index_access(lazy, TypeId::STRING);
        let resolver = CyclicAliasResolver { def_id, body };
        let ev = TypeEvaluator::with_resolver(&interner, &resolver);

        // Must terminate (no stack overflow) and conservatively exclude the
        // cyclic shape from the substitution-independent cache.
        assert!(!ev.is_closed_cacheable_kind(body));
    }
// TSZ_INLINE_TEST_END 8fa060cb3f88a9af81718b6db6109685eb93c8c6f5b2468a029dc4cd942454f7

// TSZ_INLINE_TEST_BEGIN 50d85112bc3644f99c8551cc321e4f8daedf047aa097da72f679d428551dce86 706 cacheable_kinds_exclude_index_signature_operand
    /// An `IndexAccess`/`KeyOf` over an index-signature-bearing operand (a bare
    /// mapped type, or one reached through an alias) is excluded, because the
    /// checker derives element-access diagnostics from the structural form.
    #[test]
    fn cacheable_kinds_exclude_index_signature_operand() {
        let interner = TypeInterner::new();
        let ev = evaluator(&interner);

        // A `NoopResolver` cannot resolve a `Lazy` alias's body, so an index
        // access over a `Lazy` operand is conservatively excluded.
        let lazy = interner.lazy(DefId(123));
        let idx_over_lazy = interner.index_access(lazy, TypeId::STRING);
        assert!(!ev.is_closed_cacheable_kind(idx_over_lazy));

        // An application node with an unresolvable base is also excluded
        // (conservative: the body cannot be proven safe).
        let app = interner.application(lazy, vec![TypeId::STRING]);
        assert!(!ev.is_closed_cacheable_kind(app));
    }
// TSZ_INLINE_TEST_END 50d85112bc3644f99c8551cc321e4f8daedf047aa097da72f679d428551dce86
