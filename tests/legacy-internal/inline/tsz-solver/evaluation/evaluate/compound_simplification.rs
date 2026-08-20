//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate/compound_simplification.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 96eb8ebf808117aadbf945778e94813ffcfbdece10d2f4e9d18d219f443ef8a1 830 union_simplification_threads_exact_optional_mode_into_local_subtype_checker
    #[test]
    fn union_simplification_threads_exact_optional_mode_into_local_subtype_checker() {
        let interner = TypeInterner::new();
        let (present_undefined, optional_number) = exact_optional_probe_pair(&interner);

        let mut legacy_members = vec![present_undefined, optional_number];
        let mut legacy_evaluator = TypeEvaluator::new(&interner);
        legacy_evaluator.set_exact_optional_property_types(false);
        legacy_evaluator.simplify_union_members(&mut legacy_members);
        assert_eq!(
            legacy_members,
            vec![optional_number],
            "legacy optional mode treats an optional number property as accepting present undefined",
        );

        let mut exact_members = vec![present_undefined, optional_number];
        let mut exact_evaluator = TypeEvaluator::new(&interner);
        exact_evaluator.set_exact_optional_property_types(true);
        exact_evaluator.simplify_union_members(&mut exact_members);
        assert_eq!(
            exact_members,
            vec![present_undefined, optional_number],
            "exact optional mode must not reuse legacy optional-property subtyping",
        );
    }
// TSZ_INLINE_TEST_END 96eb8ebf808117aadbf945778e94813ffcfbdece10d2f4e9d18d219f443ef8a1

// TSZ_INLINE_TEST_BEGIN 88e0e1e133a405bfc03363a7a5259789090f9a1382d0e6619e7a41afc9006bb9 856 compound_member_facts_extract_object_array_opaque_and_brand_facts
    #[test]
    fn compound_member_facts_extract_object_array_opaque_and_brand_facts() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let indexed = object_with_string_index(&interner, value);
        let tuple = interner.tuple(vec![tuple_elem(TypeId::STRING)]);
        let array = interner.array(TypeId::NUMBER);
        let lazy = interner.lazy(DefId(1001));
        let application = interner.application(lazy, vec![TypeId::STRING]);
        let branded = interner.intersect_types_raw2(TypeId::STRING, interner.object(Vec::new()));
        let evaluator = TypeEvaluator::new(&interner);

        let indexed_facts = evaluator.compound_member_facts(indexed, true);
        assert_eq!(indexed_facts.index_signature_kinds, INDEX_KIND_STRING);
        assert!(
            indexed_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&value.0)),
            "object facts include declared property names",
        );

        for member in [tuple, array] {
            let facts = evaluator.compound_member_facts(member, false);
            assert!(
                facts
                    .property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&u32::MAX)),
                "array-like members carry the property-name sentinel used by uniqueness vetoes",
            );
        }

        let application_facts = evaluator.compound_member_facts(application, false);
        assert!(application_facts.is_opaque_under_bypass_eval);

        let branded_facts = evaluator.compound_member_facts(branded, false);
        assert!(branded_facts.is_branded_primitive_intersection);

        let literal_facts =
            evaluator.compound_member_facts(interner.literal_string("token"), false);
        assert!(TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
            &literal_facts,
            false,
        ));

        let primitive_facts = evaluator.compound_member_facts(TypeId::STRING, false);
        assert!(
            !TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
                &primitive_facts,
                false,
            ),
            "bare primitive keywords stay protected without an empty-object member",
        );
        assert!(
            TypeEvaluator::<crate::relations::subtype::NoopResolver>::union_member_removable_as_subtype(
                &primitive_facts,
                true,
            ),
            "the empty-object union case keeps tsc's primitive absorption exception",
        );
    }
// TSZ_INLINE_TEST_END 88e0e1e133a405bfc03363a7a5259789090f9a1382d0e6619e7a41afc9006bb9

// TSZ_INLINE_TEST_BEGIN 6b7c7964853e26429ed57b5e41e8d426db7c3c0bb1d6cf26b78f2d77521630ce 919 compound_member_facts_merge_modifiers_without_evaluator_cache_state
    #[test]
    fn compound_member_facts_merge_modifiers_without_evaluator_cache_state() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let mut optional_readonly = PropertyInfo::opt(value, TypeId::STRING);
        optional_readonly.readonly = true;
        let required_readonly =
            interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);
        let optional_readonly = interner.object(vec![optional_readonly]);
        let intersection = interner.intersect_types_raw2(required_readonly, optional_readonly);
        let evaluator = TypeEvaluator::new(&interner);
        let before = evaluator.cache_statistics();

        let facts = evaluator.compound_member_facts(intersection, true);

        assert_eq!(
            evaluator.cache_statistics(),
            before,
            "per-call member facts must not mutate evaluator cache statistics",
        );
        assert_eq!(
            facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&value.0).copied()),
            Some((false, true)),
            "intersection modifier facts use tsc's AND-merge semantics",
        );

        let union_facts = evaluator.compound_member_facts(intersection, false);
        assert!(union_facts.property_modifiers.is_none());
    }
// TSZ_INLINE_TEST_END 6b7c7964853e26429ed57b5e41e8d426db7c3c0bb1d6cf26b78f2d77521630ce

// TSZ_INLINE_TEST_BEGIN 87c0c94fd28388834f50f84f65eb38558cd53755c1d0cd354f620779a3ab02e1 952 compound_member_facts_memo_partitions_property_modifier_mode
    #[test]
    fn compound_member_facts_memo_partitions_property_modifier_mode() {
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let mut optional_readonly = PropertyInfo::opt(value, TypeId::STRING);
        optional_readonly.readonly = true;
        let required = interner.object(vec![PropertyInfo::readonly(value, TypeId::STRING)]);
        let optional_readonly = interner.object(vec![optional_readonly]);
        let intersection = interner.intersect_types_raw2(required, optional_readonly);
        let evaluator = TypeEvaluator::new(&interner);
        let mut memo = FxHashMap::default();

        let union_facts = evaluator.compound_member_facts_memoized(intersection, false, &mut memo);

        assert!(union_facts.property_modifiers.is_none());
        assert!(memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: false,
        }));
        assert!(!memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: true,
        }));

        let intersection_facts =
            evaluator.compound_member_facts_memoized(intersection, true, &mut memo);

        assert_eq!(
            intersection_facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&value.0).copied()),
            Some((false, true)),
            "modifier-sensitive facts must not reuse the union-mode memo entry",
        );
        assert!(memo.contains_key(&CompoundMemberFactsKey {
            type_id: intersection,
            include_property_modifiers: true,
        }));

        let entries_after_both_modes = memo.len();
        assert_eq!(
            evaluator.compound_member_facts_memoized(intersection, false, &mut memo),
            union_facts,
        );
        assert_eq!(
            memo.len(),
            entries_after_both_modes,
            "re-reading the same mode should hit the local memo",
        );
    }
// TSZ_INLINE_TEST_END 87c0c94fd28388834f50f84f65eb38558cd53755c1d0cd354f620779a3ab02e1

// TSZ_INLINE_TEST_BEGIN a1f16f46a7628f20a81f2076c853f0f9a061b36d8fa9544a7c4bbc0ddce5d896 1004 compound_member_facts_memo_reuses_shared_intersection_children
    #[test]
    fn compound_member_facts_memo_reuses_shared_intersection_children() {
        let interner = TypeInterner::new();
        let shared_name = interner.intern_string("shared");
        let left_name = interner.intern_string("left");
        let shared = interner.object(vec![PropertyInfo::readonly(shared_name, TypeId::STRING)]);
        let left_only = interner.object(vec![PropertyInfo::new(left_name, TypeId::NUMBER)]);
        let tuple = interner.tuple(vec![tuple_elem(TypeId::BOOLEAN)]);
        let left = interner.intersect_types_raw2(shared, left_only);
        let right = interner.intersect_types_raw2(shared, tuple);
        let evaluator = TypeEvaluator::new(&interner);
        let mut memo = FxHashMap::default();

        let left_facts = evaluator.compound_member_facts_memoized(left, true, &mut memo);
        let entries_after_left = memo.len();
        let right_facts = evaluator.compound_member_facts_memoized(right, true, &mut memo);

        assert!(
            left_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&shared_name.0) && names.contains(&left_name.0)),
            "intersection facts merge object property names",
        );
        assert!(
            right_facts
                .property_names
                .as_ref()
                .is_some_and(|names| names.contains(&shared_name.0) && names.contains(&u32::MAX)),
            "array-like sentinel facts are preserved while sharing object children",
        );
        assert_eq!(
            right_facts
                .property_modifiers
                .as_ref()
                .and_then(|mods| mods.get(&shared_name.0).copied()),
            Some((false, true)),
            "shared child modifier facts stay available in intersection mode",
        );
        assert_eq!(
            memo.len(),
            entries_after_left + 3,
            "the second intersection adds only its unique child facts and top-level facts",
        );

        let entries_after_right = memo.len();
        assert_eq!(
            evaluator.compound_member_facts_memoized(right, true, &mut memo),
            right_facts,
        );
        assert_eq!(
            memo.len(),
            entries_after_right,
            "re-reading a top-level intersection should hit the local memo",
        );
    }
// TSZ_INLINE_TEST_END a1f16f46a7628f20a81f2076c853f0f9a061b36d8fa9544a7c4bbc0ddce5d896

// TSZ_INLINE_TEST_BEGIN e5a0d31e50ae248b1fd13ff2054a648891ffb2ae479f8746ac130906563c796c 1061 compound_member_facts_keep_index_signature_veto_after_session_subtype_hit
    #[test]
    fn compound_member_facts_keep_index_signature_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("value");
        let with_index = object_with_string_index(&interner, prop);
        let without_index = interner.object(vec![PropertyInfo::new(prop, TypeId::STRING)]);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        seed_session_compound_probe(&session, &checker, with_index, without_index, true);
        seed_session_compound_probe(&session, &checker, without_index, with_index, true);

        let mut members = vec![with_index, without_index];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![with_index],
            "a session raw-subtype hit must still rerun the index-signature removal veto",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed subtype probes should not duplicate evaluator-local relation entries",
        );
    }
// TSZ_INLINE_TEST_END e5a0d31e50ae248b1fd13ff2054a648891ffb2ae479f8746ac130906563c796c

// TSZ_INLINE_TEST_BEGIN a93a09f627dcbf962d7da5b081c2ac469cd84fc0e9161ac1831fdf5d5adb27aa 1088 compound_member_facts_keep_branded_literal_veto_after_session_subtype_hit
    #[test]
    fn compound_member_facts_keep_branded_literal_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("token");
        let empty = interner.object(Vec::new());
        let branded_string = interner.intersect_types_raw2(TypeId::STRING, empty);
        let branded_number = interner.intersect_types_raw2(TypeId::NUMBER, empty);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        for &brand in &[branded_string, branded_number] {
            seed_session_compound_probe(&session, &checker, literal, brand, true);
            seed_session_compound_probe(&session, &checker, brand, literal, false);
        }
        seed_session_compound_probe(&session, &checker, branded_string, branded_number, false);
        seed_session_compound_probe(&session, &checker, branded_number, branded_string, false);

        let mut members = vec![literal, branded_string, branded_number];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![literal, branded_string, branded_number],
            "cached raw subtype hits must not let branded-primitive vetoes drop literal members",
        );
    }
// TSZ_INLINE_TEST_END a93a09f627dcbf962d7da5b081c2ac469cd84fc0e9161ac1831fdf5d5adb27aa

// TSZ_INLINE_TEST_BEGIN 285cc893b4959b417122bb2e33d7e9abf83046c29dab9663ef52ef6d7734c851 1115 compound_member_facts_keep_opaque_intersection_veto_after_session_subtype_hit
    #[test]
    fn compound_member_facts_keep_opaque_intersection_veto_after_session_subtype_hit() {
        let interner = TypeInterner::new();
        let prop = interner.intern_string("path");
        let opaque = interner.application(interner.lazy(DefId(2002)), vec![TypeId::STRING]);
        let concrete = interner.object(vec![PropertyInfo::opt(prop, TypeId::STRING)]);
        let checker = compound_probe_checker(&interner, false);
        let session = EvaluationSession::new();
        seed_session_compound_probe(&session, &checker, concrete, opaque, true);
        seed_session_compound_probe(&session, &checker, opaque, concrete, false);

        let mut members = vec![opaque, concrete];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_intersection_members(&mut members);

        assert_eq!(
            members,
            vec![opaque, concrete],
            "a cached raw-subtype hit must still let opaque Application/Lazy members veto removal",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed subtype probes should not duplicate evaluator-local relation entries",
        );
    }
// TSZ_INLINE_TEST_END 285cc893b4959b417122bb2e33d7e9abf83046c29dab9663ef52ef6d7734c851

// TSZ_INLINE_TEST_BEGIN 6e92589a13b68633d37b9ad440f945bfe386690c8abfa5dbef70d7d82507c0bc 1142 compound_subtype_cache_partitions_seeded_probe_by_exact_optional_mode
    #[test]
    fn compound_subtype_cache_partitions_seeded_probe_by_exact_optional_mode() {
        let interner = TypeInterner::new();
        let (present_undefined, optional_number) = exact_optional_probe_pair(&interner);

        let mut evaluator = TypeEvaluator::new(&interner);
        evaluator.set_exact_optional_property_types(false);
        let legacy_checker = compound_probe_checker(&interner, false);
        evaluator.seed_compound_subtype_cache_for_test(
            &legacy_checker,
            present_undefined,
            optional_number,
            true,
        );

        // Flip the mode without clearing the memo so this test proves the key
        // partition itself, not only `set_exact_optional_property_types` reset.
        evaluator.exact_optional_property_types = true;
        let mut exact_members = vec![present_undefined, optional_number];
        evaluator.simplify_union_members(&mut exact_members);

        assert_eq!(
            exact_members,
            vec![present_undefined, optional_number],
            "a legacy-mode seeded verdict must not be read by an exact-mode compound probe",
        );
    }
// TSZ_INLINE_TEST_END 6e92589a13b68633d37b9ad440f945bfe386690c8abfa5dbef70d7d82507c0bc

// TSZ_INLINE_TEST_BEGIN 2c564d58e0d24df7701632e729de98e10370c1a9229fbe9dd5c05c18a50d8e8c 1170 compound_subtype_cache_skips_shared_budget_failure
    #[test]
    fn compound_subtype_cache_skips_shared_budget_failure() {
        crate::limits::reset_subtype_thread_local_state();
        let interner = TypeInterner::new();
        let value = interner.intern_string("value");
        let extra = interner.intern_string("extra");
        let source = interner.object(vec![
            PropertyInfo::new(value, TypeId::STRING),
            PropertyInfo::new(extra, TypeId::NUMBER),
        ]);
        let target = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);
        let mut checker = compound_probe_checker(&interner, false);
        let mut evaluator = TypeEvaluator::new(&interner);

        let mut held_frames = Vec::new();
        for _ in 0..MAX_SOLVER_STACK_FRAMES {
            held_frames.push(try_enter_solver_frame().expect("solver frame budget has headroom"));
        }
        assert!(!evaluator.compound_subtype_cached(&mut checker, source, target));
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "a strict shared-budget failure must not enter the compound memo",
        );

        drop(held_frames);
        crate::limits::reset_subtype_thread_local_state();
        checker.reset();
        assert!(evaluator.compound_subtype_cached(&mut checker, source, target));
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            1,
            "a fresh-budget structural proof should be memoized",
        );
        crate::limits::reset_subtype_thread_local_state();
    }
// TSZ_INLINE_TEST_END 2c564d58e0d24df7701632e729de98e10370c1a9229fbe9dd5c05c18a50d8e8c

// TSZ_INLINE_TEST_BEGIN ffcfd8ebcc0424394fc816362d324fda6da499b13050185add2a0ea376cb95b4 1207 compound_simplification_reads_session_probe_cache
    #[test]
    fn compound_simplification_reads_session_probe_cache() {
        let interner = TypeInterner::new();
        let lit_a = interner.literal_string("a");
        let narrow = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            lit_a,
        )]);
        let wide = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            TypeId::STRING,
        )]);
        let checker = compound_probe_checker(&interner, false);
        let key = CompoundSubtypePairKey::from_checker(&checker, narrow, wide);
        let session = EvaluationSession::new();
        session.compound_subtype_probe_put(key, false);

        let mut members = vec![narrow, wide];
        let mut evaluator = TypeEvaluator::new(&interner).with_evaluation_session(&session);
        evaluator.simplify_union_members(&mut members);

        assert_eq!(
            members,
            vec![narrow, wide],
            "a fresh evaluator should read raw subtype probes from the owning session",
        );
        assert_eq!(
            session.compound_subtype_probe_cache_entries(),
            2,
            "the seeded decisive probe and the reverse miss should live in the session",
        );
        assert_eq!(
            evaluator.cache_statistics().compound_subtype_entries,
            0,
            "session-backed probes should not duplicate entries in the evaluator-local fallback",
        );
    }
// TSZ_INLINE_TEST_END ffcfd8ebcc0424394fc816362d324fda6da499b13050185add2a0ea376cb95b4

// TSZ_INLINE_TEST_BEGIN ee2d73f9411e0c81ff1a6a54f2755bc822ff859f655833b0985514047c7c279d 1245 compound_subtype_probe_key_tracks_relation_and_simplifier_modes
    #[test]
    fn compound_subtype_probe_key_tracks_relation_and_simplifier_modes() {
        let interner = TypeInterner::new();
        let (source, target) = exact_optional_probe_pair(&interner);

        let legacy_checker = compound_probe_checker(&interner, false);
        let mut exact_checker = compound_probe_checker(&interner, true);
        let mut unchecked_checker = compound_probe_checker(&interner, false);
        unchecked_checker.no_unchecked_indexed_access = true;
        let mut normal_eval_checker = compound_probe_checker(&interner, false);
        normal_eval_checker.bypass_evaluation = false;
        let mut shallow_checker = compound_probe_checker(&interner, false);
        shallow_checker.max_depth = MAX_SUBTYPE_DEPTH - 1;

        let legacy_key = CompoundSubtypePairKey::from_checker(&legacy_checker, source, target);
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&exact_checker, source, target),
            "exactOptionalPropertyTypes is part of compound subtype probe identity",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&unchecked_checker, source, target),
            "noUncheckedIndexedAccess is part of the underlying relation identity",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&normal_eval_checker, source, target),
            "bypass-evaluation mode is specific to compound simplification probes",
        );
        assert_ne!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&shallow_checker, source, target),
            "compound subtype probe depth participates in the local memo key",
        );

        exact_checker.exact_optional_property_types = false;
        assert_eq!(
            legacy_key,
            CompoundSubtypePairKey::from_checker(&exact_checker, source, target),
            "matching relation and simplifier modes should address the same local memo slot",
        );
    }
// TSZ_INLINE_TEST_END ee2d73f9411e0c81ff1a6a54f2755bc822ff859f655833b0985514047c7c279d
