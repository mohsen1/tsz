//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8cf08f470e660e2a522da35e3a7d21b2f9482d751f81ab0436ff1749f619e457 1718 symbol_def_cycle_state_ignores_same_def_pair
    #[test]
    fn symbol_def_cycle_state_ignores_same_def_pair() {
        let state = SymbolDefCycleState::from_symbol_pairs(
            (DefId(1), DefId(2)),
            (DefId(1), DefId(2)),
            (Some(SymbolId(10)), Some(SymbolId(20))),
            (SymbolId(10), SymbolId(20)),
        );
        assert_eq!(state, SymbolDefCycleState::NoCycle);
    }
// TSZ_INLINE_TEST_END 8cf08f470e660e2a522da35e3a7d21b2f9482d751f81ab0436ff1749f619e457

// TSZ_INLINE_TEST_BEGIN 8a08f91fef53ba14c3144ae780eca97da04fb795f6eb8e9e2b4062216b4f9c8f 1729 symbol_def_cycle_state_detects_forward_alias_pair
    #[test]
    fn symbol_def_cycle_state_detects_forward_alias_pair() {
        let state = SymbolDefCycleState::from_symbol_pairs(
            (DefId(1), DefId(2)),
            (DefId(3), DefId(4)),
            (Some(SymbolId(10)), Some(SymbolId(20))),
            (SymbolId(10), SymbolId(20)),
        );
        assert_eq!(state, SymbolDefCycleState::CycleDetected);
    }
// TSZ_INLINE_TEST_END 8a08f91fef53ba14c3144ae780eca97da04fb795f6eb8e9e2b4062216b4f9c8f

// TSZ_INLINE_TEST_BEGIN 39ed66c9852578596c53b75b06cbf3504b63bdc36f6dc2d007d4a8856fd505b4 1740 symbol_def_cycle_state_detects_reversed_alias_pair
    #[test]
    fn symbol_def_cycle_state_detects_reversed_alias_pair() {
        let state = SymbolDefCycleState::from_symbol_pairs(
            (DefId(1), DefId(2)),
            (DefId(3), DefId(4)),
            (Some(SymbolId(20)), Some(SymbolId(10))),
            (SymbolId(10), SymbolId(20)),
        );
        assert_eq!(state, SymbolDefCycleState::CycleDetected);
    }
// TSZ_INLINE_TEST_END 39ed66c9852578596c53b75b06cbf3504b63bdc36f6dc2d007d4a8856fd505b4

// TSZ_INLINE_TEST_BEGIN fba7dbf747fde2ef37f65852a6b796d5663b0b02707e2b2088186b3b10cd44c4 1751 symbol_def_cycle_state_rejects_partial_or_different_symbols
    #[test]
    fn symbol_def_cycle_state_rejects_partial_or_different_symbols() {
        let missing_symbol = SymbolDefCycleState::from_symbol_pairs(
            (DefId(1), DefId(2)),
            (DefId(3), DefId(4)),
            (Some(SymbolId(10)), None),
            (SymbolId(10), SymbolId(20)),
        );
        assert_eq!(missing_symbol, SymbolDefCycleState::NoCycle);

        let different_symbol = SymbolDefCycleState::from_symbol_pairs(
            (DefId(1), DefId(2)),
            (DefId(3), DefId(4)),
            (Some(SymbolId(10)), Some(SymbolId(30))),
            (SymbolId(10), SymbolId(20)),
        );
        assert_eq!(different_symbol, SymbolDefCycleState::NoCycle);
    }
// TSZ_INLINE_TEST_END fba7dbf747fde2ef37f65852a6b796d5663b0b02707e2b2088186b3b10cd44c4

// TSZ_INLINE_TEST_BEGIN ce059cb84d0f074647052a6f79d9da0e9614b680bb5fd672355d713959fed704 1803 relation_cache_write_ignores_sibling_checker_lazy_event
    #[test]
    fn relation_cache_write_ignores_sibling_checker_lazy_event() {
        assert_definitive_write_gate(
            |_, sibling| sibling.note_unresolved_lazy_relation_event(),
            Some(false),
            "sibling checker lazy misses must not poison this relation frame",
        );
    }
// TSZ_INLINE_TEST_END ce059cb84d0f074647052a6f79d9da0e9614b680bb5fd672355d713959fed704

// TSZ_INLINE_TEST_BEGIN 8ce773bb3b58480eeafc85964b1641c74335564c9c03ef6985aeacc426bf1244 1812 relation_cache_write_skips_same_checker_lazy_event
    #[test]
    fn relation_cache_write_skips_same_checker_lazy_event() {
        assert_definitive_write_gate(
            |checker, _| checker.note_unresolved_lazy_relation_event(),
            None,
            "this checker's unresolved Lazy event must keep the frame non-cacheable",
        );
    }
// TSZ_INLINE_TEST_END 8ce773bb3b58480eeafc85964b1641c74335564c9c03ef6985aeacc426bf1244

// TSZ_INLINE_TEST_BEGIN d27ded85feee33c6cd2a0c7547cb741def786ad976a805a37b019b38a8232acd 1821 relation_cache_write_skips_contributing_subchecker_lazy_event
    #[test]
    fn relation_cache_write_skips_contributing_subchecker_lazy_event() {
        assert_definitive_write_gate(
            |checker, subchecker| {
                let entry = subchecker.unresolved_lazy_relation_event_count();
                subchecker.note_unresolved_lazy_relation_event();
                checker.absorb_unresolved_lazy_relation_events_from(subchecker, entry);
            },
            None,
            "a contributing subchecker Lazy miss must keep the outer frame non-cacheable",
        );
    }
// TSZ_INLINE_TEST_END d27ded85feee33c6cd2a0c7547cb741def786ad976a805a37b019b38a8232acd

// TSZ_INLINE_TEST_BEGIN 2c8c2241654ace07823692747932c89e587ca83aa1b00550bffeed44a88b5c95 1834 relation_cache_write_skips_same_checker_incomplete_evaluation_event
    #[test]
    fn relation_cache_write_skips_same_checker_incomplete_evaluation_event() {
        assert_definitive_write_gate(
            |checker, _| checker.note_incomplete_evaluation_relation_event(),
            None,
            "a guard-truncated evaluation must keep the frame's verdict non-cacheable",
        );
    }
// TSZ_INLINE_TEST_END 2c8c2241654ace07823692747932c89e587ca83aa1b00550bffeed44a88b5c95

// TSZ_INLINE_TEST_BEGIN 0faef8c33685251376306e4aa3b20cf41db3e7340cb4210edc5c41967a76f5f2 1843 relation_cache_write_ignores_sibling_checker_incomplete_evaluation_event
    #[test]
    fn relation_cache_write_ignores_sibling_checker_incomplete_evaluation_event() {
        assert_definitive_write_gate(
            |_, sibling| sibling.note_incomplete_evaluation_relation_event(),
            Some(false),
            "sibling checker truncation events must not poison this relation frame",
        );
    }
// TSZ_INLINE_TEST_END 0faef8c33685251376306e4aa3b20cf41db3e7340cb4210edc5c41967a76f5f2

// TSZ_INLINE_TEST_BEGIN 9090fa53496545206d36acb327b1d4cdea0dc8e501b5cb426f514849db00ef15 1852 relation_cache_write_skips_contributing_subchecker_incomplete_evaluation_event
    #[test]
    fn relation_cache_write_skips_contributing_subchecker_incomplete_evaluation_event() {
        assert_definitive_write_gate(
            |checker, subchecker| {
                let entry = subchecker.incomplete_evaluation_relation_event_count();
                subchecker.note_incomplete_evaluation_relation_event();
                checker.absorb_incomplete_evaluation_relation_events_from(subchecker, entry);
            },
            None,
            "a contributing subchecker truncation must keep the outer frame non-cacheable",
        );
    }
// TSZ_INLINE_TEST_END 9090fa53496545206d36acb327b1d4cdea0dc8e501b5cb426f514849db00ef15

// TSZ_INLINE_TEST_BEGIN cfea88540608a9f0a3ad5e94ea5b0d846db2f066605bc29116d88ec7ff3dc5af 1865 relation_cache_write_skips_consumed_subchecker_limit_event
    #[test]
    fn relation_cache_write_skips_consumed_subchecker_limit_event() {
        assert_definitive_write_gate(
            |checker, subchecker| {
                let entry = subchecker.relation_limit_event_count();
                let _ = subchecker.depth_result();
                checker.absorb_relation_limit_events_from(subchecker, entry);
            },
            None,
            "a consumed subchecker Maybe verdict must keep the outer frame non-cacheable",
        );
    }
// TSZ_INLINE_TEST_END cfea88540608a9f0a3ad5e94ea5b0d846db2f066605bc29116d88ec7ff3dc5af

// TSZ_INLINE_TEST_BEGIN 7f6929c44dff3319f40e8c15f5e9d741db28f795ac69b80f331f8a861dae3c6c 1878 active_type_param_equivalence_bypasses_relation_cache
    #[test]
    fn active_type_param_equivalence_bypasses_relation_cache() {
        crate::limits::reset_subtype_thread_local_state();
        let interner = TypeInterner::new();
        let db = QueryCache::new(&interner);
        let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);

        let left_key =
            interner.fresh_type_param(TypeParamInfo::simple(interner.intern_string("Left")));
        let right_key =
            interner.fresh_type_param(TypeParamInfo::simple(interner.intern_string("Right")));
        let object = interner.object_with_index(ObjectShape {
            symbol_index: None,
            symbol: None,
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
        });
        let source = interner.index_access(object, left_key);
        let target = interner.index_access(object, right_key);
        let cache_key = checker.make_cache_key(source, target);
        db.insert_subtype_cache(cache_key, false);

        checker
            .type_param_equivalences
            .push(crate::relations::subtype::TypeParamEquivalence::ids(
                left_key, right_key,
            ));
        assert!(
            checker.check_subtype(source, target).is_true(),
            "active alpha-pairing must recompute instead of replaying a flagless cached false"
        );

        let taint_entry =
            checker.relation_taint_snapshot(crate::limits::weak_type_sensitivity_count());
        checker.record_definitive_verdict(source, target, SubtypeResult::True, true, taint_entry);
        assert_eq!(
            db.lookup_subtype_cache(cache_key),
            Some(false),
            "alpha-scoped verdicts must not overwrite the ordinary relation cache slot"
        );
        crate::limits::reset_subtype_thread_local_state();
    }
// TSZ_INLINE_TEST_END 7f6929c44dff3319f40e8c15f5e9d741db28f795ac69b80f331f8a861dae3c6c

// TSZ_INLINE_TEST_BEGIN bda1a458eeb088aca8c9868762886c4a5733407ea0f3ec08957504965c621189 1939 truncated_meta_type_evaluation_taints_relation_verdict_out_of_caches
    /// End-to-end #14346 verdict-consumption witness: a relation whose
    /// meta-type evaluation seat truncates (the divergent alias
    /// `Rec<T> = Rec<T[]>` grows its argument every step, so the evaluator
    /// bails with `Termination::Incomplete { DepthExceeded }`) must not
    /// publish its verdict to the shared relation cache. Before the typed
    /// verdict was consumed, the budget-truncated comparison was memoized as
    /// a definitive answer for the pair — an artifact of the ambient depth
    /// state, not a pure function of the `RelationCacheKey`.
    ///
    /// Structural axis: keyed on recursion state, not a spelling — the def id
    /// and the type-parameter name both vary.
    #[test]
    fn truncated_meta_type_evaluation_taints_relation_verdict_out_of_caches() {
        use crate::def::DefKind;
        use crate::relations::subtype::TypeEnvironment;

        for (def_raw, param_name) in [(821u32, "T"), (947u32, "Item")] {
            crate::limits::reset_subtype_thread_local_state();
            let interner = TypeInterner::new();
            let def_id = DefId(def_raw);
            let t_param = TypeParamInfo::simple(interner.intern_string(param_name));
            let t_type = interner.fresh_type_param(t_param);
            // Body: `Rec<T[]>` — the alias re-applies itself to an
            // ever-growing argument, so evaluation diverges and a guard bails.
            let grown_arg = interner.array(t_type);
            let body = interner.application(interner.lazy(def_id), vec![grown_arg]);

            let mut env = TypeEnvironment::new();
            env.insert_def_with_params(def_id, body, vec![t_param]);
            env.insert_def_kind(def_id, DefKind::TypeAlias);
            let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);

            let db = QueryCache::new(&interner);
            let mut checker = SubtypeChecker::with_resolver(&interner, &env).with_query_db(&db);
            let key = checker.make_cache_key(app, TypeId::NUMBER);
            let events_at_entry = checker.incomplete_evaluation_relation_event_count();

            // The verdict itself is unchanged by this fix; only its cache
            // publication is suppressed.
            let _ = checker.check_subtype(app, TypeId::NUMBER);

            assert_ne!(
                checker.incomplete_evaluation_relation_event_count(),
                events_at_entry,
                "the truncated meta-type evaluation must be consumed as a taint event"
            );
            assert_eq!(
                db.lookup_subtype_cache(key),
                None,
                "a verdict computed from a guard-truncated evaluation must not be \
                 memoized in the shared relation cache"
            );

            checker.reset();
            assert_eq!(
                checker.incomplete_evaluation_relation_event_count(),
                0,
                "reset must clear the guard-truncated evaluation counter"
            );
            crate::limits::reset_subtype_thread_local_state();
        }
    }
// TSZ_INLINE_TEST_END bda1a458eeb088aca8c9868762886c4a5733407ea0f3ec08957504965c621189
