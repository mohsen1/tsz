//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/eval_materialization_probe.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c25c3f8a5ff18efd0812bcf28dc06a850d36ef69d885912ce8ff11c238f3ae4e 1249 record_compute_counts_distinct_and_recompute_deltas
    /// `record_compute` distinguishes distinct inputs from recomputes and
    /// distinct results from collapsed ones, and classifies deferred-vs-eager.
    /// Asserts on deltas because the probe state is process-wide.
    #[test]
    fn record_compute_counts_distinct_and_recompute_deltas() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let idx = ProbeKind::Conditional as usize;
        let before = kind_snapshot_for_tests(idx);

        let cond = TypeData::Conditional(ConditionalTypeId(7));
        // Eager: input conditional resolves to a non-conditional concrete
        // result. Same input twice => one distinct input, two computes
        // (the second is a recompute). Two distinct concrete results.
        let concrete_a = TypeData::Intrinsic(crate::types::IntrinsicKind::Number);
        let concrete_b = TypeData::Intrinsic(crate::types::IntrinsicKind::String);
        record_compute(TypeId(100), &cond, TypeId(200), Some(&concrete_a));
        record_compute(TypeId(100), &cond, TypeId(201), Some(&concrete_b));
        // Deferred: result stays a conditional (re-interned, not resolved).
        let deferred_cond = TypeData::Conditional(ConditionalTypeId(9));
        record_compute(TypeId(101), &cond, TypeId(202), Some(&deferred_cond));

        let after = kind_snapshot_for_tests(idx);
        let d_computes = after.0 - before.0;
        let d_inputs = after.1 - before.1;
        let d_results = after.2 - before.2;
        let d_deferred = after.3 - before.3;

        assert_eq!(d_computes, 3, "three computes recorded");
        assert_eq!(d_inputs, 2, "two distinct input TypeIds (100, 101)");
        assert_eq!(
            d_results, 3,
            "three distinct result TypeIds (200, 201, 202)"
        );
        assert_eq!(d_deferred, 1, "one result stayed a conditional (deferred)");
        // recompute headroom = computes - distinct_inputs = 3 - 2 = 1.
        assert_eq!(
            d_computes - d_inputs,
            1,
            "one recompute of an existing input"
        );
    }
// TSZ_INLINE_TEST_END c25c3f8a5ff18efd0812bcf28dc06a850d36ef69d885912ce8ff11c238f3ae4e

// TSZ_INLINE_TEST_BEGIN dbf66a238e7b0673b0fbb520660f038b421c1889584bf04235f0187c9994680d 1290 record_compute_ignores_non_lever_kinds_and_routes_per_kind
    /// Non eval-engine kinds are ignored; mapped/application route to their
    /// own buckets.
    #[test]
    fn record_compute_ignores_non_lever_kinds_and_routes_per_kind() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let m_idx = ProbeKind::Mapped as usize;
        let a_idx = ProbeKind::Application as usize;
        let m_before = kind_snapshot_for_tests(m_idx);
        let a_before = kind_snapshot_for_tests(a_idx);

        // An object input is not a lever kind: must be ignored entirely.
        let object = TypeData::Array(TypeId(1));
        record_compute(TypeId(300), &object, TypeId(301), None);

        let mapped = TypeData::Mapped(MappedTypeId(3));
        let app = TypeData::Application(TypeApplicationId(4));
        record_compute(TypeId(400), &mapped, TypeId(401), None);
        record_compute(TypeId(500), &app, TypeId(501), None);

        let m_after = kind_snapshot_for_tests(m_idx);
        let a_after = kind_snapshot_for_tests(a_idx);
        // `>` (not `==`): under a shared-process test runner a sibling test
        // may record into the application bucket concurrently. The object
        // input must contribute nothing to either lever bucket, which the
        // mapped-bucket delta (no sibling writes mapped) pins exactly.
        assert_eq!(
            m_after.0 - m_before.0,
            1,
            "one mapped compute, object ignored"
        );
        assert!(a_after.0 > a_before.0, "at least our application compute");
    }
// TSZ_INLINE_TEST_END dbf66a238e7b0673b0fbb520660f038b421c1889584bf04235f0187c9994680d

// TSZ_INLINE_TEST_BEGIN 0369da16ad33a2f8cad5facf14a93fda9e0b3f000a003b38f2a19cb13b876ebf 1323 dump_report_nonempty_only_under_gate
    /// The report is empty when counters are disabled and the gate is the
    /// only check (default-behavior-unchanged contract).
    #[test]
    fn dump_report_nonempty_only_under_gate() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let app = TypeData::Application(TypeApplicationId(11));
        record_compute(TypeId(900), &app, TypeId(901), None);
        record_application_entry(true, true);
        record_application_cache_lookup(ApplicationLookupSite::RawArgs, false);
        record_application_cache_lookup(ApplicationLookupSite::ExpandedArgs, true);
        record_application_body_path(ApplicationBodyPath::KnownParams);
        record_application_cache_insert(true, true);
        let report = dump_report();
        assert!(
            report.contains("eval-materialization probe"),
            "report should render under the gate"
        );
        assert!(
            report.contains("recompute headroom"),
            "report should expose recompute headroom"
        );
        assert!(
            report.contains("application cache eligibility"),
            "report should expose the application cache eligibility split"
        );
    }
// TSZ_INLINE_TEST_END 0369da16ad33a2f8cad5facf14a93fda9e0b3f000a003b38f2a19cb13b876ebf

// TSZ_INLINE_TEST_BEGIN 55b948c4cba6269853a6cf60bf7cb9e7f9d3c03ea29fa9dfcea6ebeefbcbb8bd 1351 application_cache_eligibility_counters_record_deltas
    /// Application cache counters split `(DefId,args)` eligibility from opaque
    /// and tainted paths so #13250 follow-ups can tell whether the existing
    /// application-eval cache key is the right reuse layer.
    #[test]
    fn application_cache_eligibility_counters_record_deltas() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let before = application_cache_snapshot_for_tests();

        record_application_entry(true, false);
        record_application_entry(false, false);
        record_application_cache_lookup(ApplicationLookupSite::RawArgs, false);
        record_application_cache_lookup(ApplicationLookupSite::ExpandedArgs, true);
        record_application_body_path(ApplicationBodyPath::OpaqueResolvedUnknown);
        record_application_body_path(ApplicationBodyPath::ExtractedParams);
        record_application_cache_insert(false, true);
        record_application_cache_insert(true, false);
        record_application_cache_insert(true, true);

        let after = application_cache_snapshot_for_tests();
        assert_eq!(after.entries_with_def_id - before.entries_with_def_id, 1);
        assert_eq!(
            after.entries_without_def_id - before.entries_without_def_id,
            1
        );
        assert_eq!(
            after.entries_without_query_db - before.entries_without_query_db,
            1
        );
        assert_eq!(after.raw_lookup_misses - before.raw_lookup_misses, 1);
        assert_eq!(after.expanded_lookup_hits - before.expanded_lookup_hits, 1);
        assert_eq!(
            after.body_opaque_resolved_unknown - before.body_opaque_resolved_unknown,
            1
        );
        assert_eq!(
            after.body_extracted_params - before.body_extracted_params,
            1
        );
        assert_eq!(
            after.cache_insert_skipped_limit - before.cache_insert_skipped_limit,
            1
        );
        assert_eq!(
            after.cache_insert_skipped_no_query_db - before.cache_insert_skipped_no_query_db,
            1
        );
        assert_eq!(
            after.cache_insert_eligible - before.cache_insert_eligible,
            1
        );
    }
// TSZ_INLINE_TEST_END 55b948c4cba6269853a6cf60bf7cb9e7f9d3c03ea29fa9dfcea6ebeefbcbb8bd

// TSZ_INLINE_TEST_BEGIN 89746684b703d3d6f88a4603cad4e0de81cd58ac05f1e784c9f18e0f53ab01f3 1420 symbol_stripped_fingerprint_ignores_object_symbol
    /// The fingerprint ignores the nominal `symbol` brand: two objects with
    /// identical structure but different `ObjectShape.symbol` (and different
    /// per-property `parent_id`) must hash EQUAL.
    #[test]
    fn symbol_stripped_fingerprint_ignores_object_symbol() {
        let interner = TypeInterner::new();
        let o1 = branded_object(&interner, Some(SymbolId(11)));
        let o2 = branded_object(&interner, Some(SymbolId(22)));
        // Different brands => distinct interned TypeIds (nominal identity).
        assert_ne!(o1, o2, "branded objects must intern distinctly");
        let db: &dyn TypeDatabase = &interner;
        assert_eq!(
            symbol_stripped_fingerprint(db, o1),
            symbol_stripped_fingerprint(db, o2),
            "symbol/parent_id brand must not affect the fingerprint"
        );
    }
// TSZ_INLINE_TEST_END 89746684b703d3d6f88a4603cad4e0de81cd58ac05f1e784c9f18e0f53ab01f3

// TSZ_INLINE_TEST_BEGIN ba21c5e45527e831a709dc619feb045ca61183f63239d51ed8cb242b0a0f563c 1437 symbol_stripped_fingerprint_distinguishes_structure
    /// The fingerprint distinguishes a real structural difference (a property
    /// type change) even when the brand is identical.
    #[test]
    fn symbol_stripped_fingerprint_distinguishes_structure() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("a");
        let b = interner.intern_string("b");
        let brand = Some(SymbolId(7));
        let o1 = {
            let mut p_a = PropertyInfo::new(a, TypeId::STRING);
            p_a.parent_id = brand;
            let mut p_b = PropertyInfo::new(b, TypeId::NUMBER);
            p_b.parent_id = brand;
            interner.object_with_flags_and_symbol(vec![p_a, p_b], ObjectFlags::empty(), brand)
        };
        // Same brand, but `b: boolean` instead of `b: number`.
        let o2 = {
            let mut p_a = PropertyInfo::new(a, TypeId::STRING);
            p_a.parent_id = brand;
            let mut p_b = PropertyInfo::new(b, TypeId::BOOLEAN);
            p_b.parent_id = brand;
            interner.object_with_flags_and_symbol(vec![p_a, p_b], ObjectFlags::empty(), brand)
        };
        let db: &dyn TypeDatabase = &interner;
        assert_ne!(
            symbol_stripped_fingerprint(db, o1),
            symbol_stripped_fingerprint(db, o2),
            "a property type difference must change the fingerprint"
        );
    }
// TSZ_INLINE_TEST_END ba21c5e45527e831a709dc619feb045ca61183f63239d51ed8cb242b0a0f563c

// TSZ_INLINE_TEST_BEGIN dbed59233ca00883abf250ac91739f9227a46e4564d26d198d294d9e75ca2ffd 1468 symbol_stripped_fingerprint_strips_symbol_at_depth
    /// Recursion strips the symbol brand at depth: wrapping each of two
    /// symbol-distinct objects in an `Array` produces EQUAL fingerprints.
    #[test]
    fn symbol_stripped_fingerprint_strips_symbol_at_depth() {
        let interner = TypeInterner::new();
        let o1 = branded_object(&interner, Some(SymbolId(33)));
        let o2 = branded_object(&interner, Some(SymbolId(44)));
        let a1 = interner.array(o1);
        let a2 = interner.array(o2);
        assert_ne!(a1, a2, "arrays of branded objects intern distinctly");
        let db: &dyn TypeDatabase = &interner;
        assert_eq!(
            symbol_stripped_fingerprint(db, a1),
            symbol_stripped_fingerprint(db, a2),
            "symbol brand must be stripped through the array element"
        );
    }
// TSZ_INLINE_TEST_END dbed59233ca00883abf250ac91739f9227a46e4564d26d198d294d9e75ca2ffd

// TSZ_INLINE_TEST_BEGIN bc7ad2ffff574d56f86f23f7b80c4640c2d3c4ec1d0a925240b35d039743ed66 1487 record_canon_headroom_dedups_and_counts
    /// `record_canon_headroom` is first-sight per distinct result id: a repeat
    /// of the same result does not re-sample, and a distinct structural result
    /// adds one raw form. `query_db = None` is exercised.
    #[test]
    fn record_canon_headroom_dedups_and_counts() {
        FORCE_PROBE_FOR_TESTS.store(true, Ordering::Relaxed);
        let interner = TypeInterner::new();
        let db: &dyn TypeDatabase = &interner;
        let idx = ProbeKind::Application as usize;
        let key = TypeData::Application(TypeApplicationId(101));

        let before = canon_headroom_snapshot_for_tests(idx);
        let o1 = branded_object(&interner, Some(SymbolId(101)));
        // First sight of o1 => one new sampled result and one raw form.
        record_canon_headroom(&key, o1, db, None);
        // Repeat of o1 => first-sight gate rejects, no new sample.
        record_canon_headroom(&key, o1, db, None);
        // A structurally distinct result => one more sample + one more raw form.
        let o3 = {
            let c = interner.intern_string("c");
            interner.object(vec![PropertyInfo::new(c, TypeId::STRING)])
        };
        record_canon_headroom(&key, o3, db, None);
        let after = canon_headroom_snapshot_for_tests(idx);

        assert_eq!(
            after.0 - before.0,
            2,
            "two distinct result ids sampled (o1, o3); the repeat of o1 is gated"
        );
        // o1 and o3 are structurally distinct => two distinct raw fingerprints.
        assert_eq!(
            after.1 - before.1,
            2,
            "two distinct raw symbol-stripped forms"
        );
        assert_eq!(after.2 - before.2, 2, "two first-sight samples counted");
    }
// TSZ_INLINE_TEST_END bc7ad2ffff574d56f86f23f7b80c4640c2d3c4ec1d0a925240b35d039743ed66
