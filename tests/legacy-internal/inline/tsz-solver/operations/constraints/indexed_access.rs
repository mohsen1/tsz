//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/constraints/indexed_access.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f8acd89a4a70f1f1e4a692692a02dd34dee9ba591431e9bd942ff2cc6de6e5e6 195 candidate_keyed_index_access_infers_selected_member_template
    #[test]
    fn candidate_keyed_index_access_infers_selected_member_template() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        ctx.add_candidate(
            key_var,
            interner.literal_string("Boxed"),
            InferencePriority::NakedTypeVariable,
        );

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.resolve_with_constraints(payload_var)
                .expect("keyed index access with key evidence must resolve the payload var"),
            TypeId::NUMBER
        );
    }
// TSZ_INLINE_TEST_END f8acd89a4a70f1f1e4a692692a02dd34dee9ba591431e9bd942ff2cc6de6e5e6

// TSZ_INLINE_TEST_BEGIN 973d41046a1fbb53fc8a3f46df52b9d091797e2bdefc19d4d224405557f9ab1d 225 candidate_keyed_index_access_waits_for_key_evidence
    #[test]
    fn candidate_keyed_index_access_waits_for_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, _key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(!ctx.var_has_candidates(payload_var));
    }
// TSZ_INLINE_TEST_END 973d41046a1fbb53fc8a3f46df52b9d091797e2bdefc19d4d224405557f9ab1d

// TSZ_INLINE_TEST_BEGIN 6a688abf4d646bdea3d003c9abafe2d7456d07b7678e74ff100d570782bad11a 245 candidate_keyed_index_access_accepts_union_key_evidence
    #[test]
    fn candidate_keyed_index_access_accepts_union_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        let key_union = interner.union(vec![
            interner.literal_string("Boxed"),
            interner.literal_string("Missing"),
        ]);
        ctx.add_candidate(key_var, key_union, InferencePriority::NakedTypeVariable);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.resolve_with_constraints(payload_var)
                .expect("keyed index access with key evidence must resolve the payload var"),
            TypeId::NUMBER
        );
    }
// TSZ_INLINE_TEST_END 6a688abf4d646bdea3d003c9abafe2d7456d07b7678e74ff100d570782bad11a

// TSZ_INLINE_TEST_BEGIN ad53e62579db5d088b2320f39e9809930daeb02dbf8680526bc88dc29cab4e75 275 candidate_keyed_index_access_ignores_missing_key_evidence
    #[test]
    fn candidate_keyed_index_access_ignores_missing_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        ctx.add_candidate(
            key_var,
            interner.literal_string("Missing"),
            InferencePriority::NakedTypeVariable,
        );

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(!ctx.var_has_candidates(payload_var));
    }
// TSZ_INLINE_TEST_END ad53e62579db5d088b2320f39e9809930daeb02dbf8680526bc88dc29cab4e75
