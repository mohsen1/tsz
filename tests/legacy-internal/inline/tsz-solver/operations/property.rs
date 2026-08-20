//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/property.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b7586e09dda1cf800ae17a5ee503a69d56344117f96fa8b8f2b465f351c1f635 1602 memo_hit_miss_resolves_once
    /// Miss then hit: `resolve` runs exactly once for a key, and the stored
    /// result is returned verbatim on the second access.
    #[test]
    fn memo_hit_miss_resolves_once() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let prop = interner.intern_string("x");

        let calls = Cell::new(0u32);
        let run = || {
            evaluator.memoize_deferred_property(
                obj,
                prop,
                || panic!("cycle fallback must not run on the hot path"),
                || {
                    calls.set(calls.get() + 1);
                    marker(7)
                },
            )
        };

        let first = run();
        let second = run();
        assert_eq!(first.success_type(), Some(TypeId(7)));
        assert_eq!(second.success_type(), Some(TypeId(7)));
        assert_eq!(
            calls.get(),
            1,
            "resolve should run once across two accesses"
        );
    }
// TSZ_INLINE_TEST_END b7586e09dda1cf800ae17a5ee503a69d56344117f96fa8b8f2b465f351c1f635

// TSZ_INLINE_TEST_BEGIN c5d23b80f25e3df7ce01e9f4c75884b73f137738ac7d018f90f095710f9ebcd4 1634 memo_keys_are_per_type_and_prop
    /// Distinct property names (and distinct types) are independent keys.
    #[test]
    fn memo_keys_are_per_type_and_prop() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let other = TypeId(200);
        let x = interner.intern_string("x");
        let y = interner.intern_string("y");

        let panic_cycle = || panic!("no cycle expected");
        let rx = evaluator.memoize_deferred_property(obj, x, panic_cycle, || marker(1));
        let ry = evaluator.memoize_deferred_property(obj, y, panic_cycle, || marker(2));
        let rz = evaluator.memoize_deferred_property(other, x, panic_cycle, || marker(3));
        assert_eq!(rx.success_type(), Some(TypeId(1)));
        assert_eq!(ry.success_type(), Some(TypeId(2)));
        assert_eq!(rz.success_type(), Some(TypeId(3)));
    }
// TSZ_INLINE_TEST_END c5d23b80f25e3df7ce01e9f4c75884b73f137738ac7d018f90f095710f9ebcd4

// TSZ_INLINE_TEST_BEGIN ab079dbde733432456a65e18f50b6f1acaa0a30e00b5738c84d6d8b68ae27af4 1654 memo_key_includes_flags
    /// The two mutable flags participate in the key, so a result computed under
    /// one flag configuration is never reused under another.
    #[test]
    fn memo_key_includes_flags() {
        let interner = TypeInterner::new();
        let mut evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let prop = interner.intern_string("x");
        let options_prop = interner.intern_string("options");
        let exact_prop = interner.intern_string("exact");
        let private_prop = interner.intern_string("private");
        let panic_cycle = || panic!("no cycle expected");

        evaluator.set_skip_this_binding(false);
        let a = evaluator.memoize_deferred_property(obj, prop, panic_cycle, || marker(10));
        evaluator.set_skip_this_binding(true);
        let b = evaluator.memoize_deferred_property(obj, prop, panic_cycle, || marker(20));
        evaluator.set_skip_this_binding(false);
        let a_again = evaluator.memoize_deferred_property(obj, prop, panic_cycle, || {
            panic!("flag=false entry must be cached")
        });

        assert_eq!(a.success_type(), Some(TypeId(10)));
        assert_eq!(b.success_type(), Some(TypeId(20)));
        assert_eq!(a_again.success_type(), Some(TypeId(10)));

        evaluator.set_no_unchecked_indexed_access(false);
        let c0 = evaluator.memoize_deferred_property(obj, options_prop, panic_cycle, || marker(30));
        evaluator.set_no_unchecked_indexed_access(true);
        let c1 = evaluator.memoize_deferred_property(obj, options_prop, panic_cycle, || marker(31));
        evaluator.set_no_unchecked_indexed_access(false);
        let c0_again = evaluator.memoize_deferred_property(obj, options_prop, panic_cycle, || {
            panic!("noUncheckedIndexedAccess=false entry must stay cached separately")
        });
        assert_eq!(c0.success_type(), Some(TypeId(30)));
        assert_eq!(c1.success_type(), Some(TypeId(31)));
        assert_eq!(c0_again.success_type(), Some(TypeId(30)));

        evaluator.set_exact_optional_property_types(false);
        let d = evaluator.memoize_deferred_property(obj, exact_prop, panic_cycle, || marker(40));
        evaluator.set_exact_optional_property_types(true);
        let e = evaluator.memoize_deferred_property(obj, exact_prop, panic_cycle, || marker(50));
        evaluator.set_exact_optional_property_types(false);
        let d_again = evaluator.memoize_deferred_property(obj, exact_prop, panic_cycle, || {
            panic!("exactOptionalPropertyTypes=false entry must stay cached separately")
        });
        assert_eq!(d.success_type(), Some(TypeId(40)));
        assert_eq!(e.success_type(), Some(TypeId(50)));
        assert_eq!(d_again.success_type(), Some(TypeId(40)));

        evaluator.set_allow_private_identifier_properties(true);
        let f = evaluator.memoize_deferred_property(obj, private_prop, panic_cycle, || marker(60));
        evaluator.set_allow_private_identifier_properties(false);
        let g = evaluator.memoize_deferred_property(obj, private_prop, panic_cycle, || marker(70));
        evaluator.set_allow_private_identifier_properties(true);
        let f_again = evaluator.memoize_deferred_property(obj, private_prop, panic_cycle, || {
            panic!("private-visibility=true entry must stay cached separately")
        });
        assert_eq!(f.success_type(), Some(TypeId(60)));
        assert_eq!(g.success_type(), Some(TypeId(70)));
        assert_eq!(f_again.success_type(), Some(TypeId(60)));
    }
// TSZ_INLINE_TEST_END ab079dbde733432456a65e18f50b6f1acaa0a30e00b5738c84d6d8b68ae27af4

// TSZ_INLINE_TEST_BEGIN f13b2ca7ddb852bcd7d584f8fec228a2483f12156158268d1556865e0c5ec4c2 1718 memo_cycle_reentry_uses_fallback
    /// Cycle-safety: a re-entry of the same key while it is in progress returns
    /// the `on_cycle` fallback rather than re-running `resolve`, and the
    /// outermost result is what gets memoized.
    #[test]
    fn memo_cycle_reentry_uses_fallback() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let prop = interner.intern_string("x");

        let resolve_calls = Cell::new(0u32);
        let cycle_calls = Cell::new(0u32);

        let outer = evaluator.memoize_deferred_property(
            obj,
            prop,
            || {
                cycle_calls.set(cycle_calls.get() + 1);
                marker(999)
            },
            || {
                resolve_calls.set(resolve_calls.get() + 1);
                // Re-enter the same key: simulates a cyclic deferred base.
                let inner = evaluator.memoize_deferred_property(
                    obj,
                    prop,
                    || {
                        cycle_calls.set(cycle_calls.get() + 1);
                        marker(999)
                    },
                    || panic!("inner resolve must be short-circuited by InProgress marker"),
                );
                assert_eq!(
                    inner.success_type(),
                    Some(TypeId(999)),
                    "re-entry should hit the cycle fallback"
                );
                marker(42)
            },
        );

        assert_eq!(outer.success_type(), Some(TypeId(42)));
        assert_eq!(resolve_calls.get(), 1, "outer resolve runs once");
        assert_eq!(cycle_calls.get(), 1, "cycle fallback runs once on re-entry");
        assert!(
            !evaluator.property_result_cacheable(),
            "same-key in-progress cycle fallback must taint outer property-cache publication"
        );

        // After the cycle resolves, the fallback-derived outer result is not
        // memoized: a fresh access must recompute rather than reusing `42`.
        let recompute_calls = Cell::new(0u32);
        let again = evaluator.memoize_deferred_property(
            obj,
            prop,
            || panic!("no cycle on cached access"),
            || {
                recompute_calls.set(recompute_calls.get() + 1);
                marker(7)
            },
        );
        assert_eq!(again.success_type(), Some(TypeId(7)));
        assert_eq!(
            recompute_calls.get(),
            1,
            "fallback-derived outer result must not be cached"
        );
    }
// TSZ_INLINE_TEST_END f13b2ca7ddb852bcd7d584f8fec228a2483f12156158268d1556865e0c5ec4c2

// TSZ_INLINE_TEST_BEGIN 189c1ce4d0e679d70cdb35768132950d50b5d63e86862db23d9ce6704ee025a8 1787 memo_skips_caching_truncated_results
    /// Truncation-safety: when the recursion guard reports truncation during a
    /// resolution, the (depth-dependent) result is returned but NOT cached, so a
    /// later un-truncated access recomputes the complete answer.
    #[test]
    fn memo_skips_caching_truncated_results() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let prop = interner.intern_string("x");

        // First access trips the guard's exceeded flag during resolution.
        let truncated = evaluator.memoize_deferred_property(
            obj,
            prop,
            || panic!("no cycle"),
            || {
                evaluator.guard.borrow_mut().mark_exceeded();
                marker(1)
            },
        );
        assert_eq!(truncated.success_type(), Some(TypeId(1)));

        // The truncated result must not have been cached: a second access
        // re-runs resolve. (The guard stays exceeded, so this is still treated
        // as truncated and remains uncached, which is the conservative,
        // behaviour-preserving choice.)
        let calls = Cell::new(0u32);
        let recomputed = evaluator.memoize_deferred_property(
            obj,
            prop,
            || panic!("no cycle"),
            || {
                calls.set(calls.get() + 1);
                marker(2)
            },
        );
        assert_eq!(calls.get(), 1, "truncated result must not be cached");
        assert_eq!(recomputed.success_type(), Some(TypeId(2)));
    }
// TSZ_INLINE_TEST_END 189c1ce4d0e679d70cdb35768132950d50b5d63e86862db23d9ce6704ee025a8

// TSZ_INLINE_TEST_BEGIN 7bd910d6a0bc9578fc6df5aa2583658c6e567853279b47649d32024def17b86b 1824 memo_skips_caching_guard_denied_results
    #[test]
    fn memo_skips_caching_guard_denied_results() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let obj = TypeId(100);
        let prop = interner.intern_string("x");

        let degraded = evaluator.memoize_deferred_property(
            obj,
            prop,
            || panic!("no cycle"),
            || {
                let _active = evaluator
                    .enter_property_access_guard(obj)
                    .expect("first entry should be accepted");
                assert!(
                    evaluator.enter_property_access_guard(obj).is_none(),
                    "same-key recursive guard entry should be denied"
                );
                marker(1)
            },
        );
        assert_eq!(degraded.success_type(), Some(TypeId(1)));
        assert!(
            !evaluator.property_result_cacheable(),
            "guard-denied fallback must taint outer property-cache publication"
        );

        let calls = Cell::new(0u32);
        let recomputed = evaluator.memoize_deferred_property(
            obj,
            prop,
            || panic!("no cycle"),
            || {
                calls.set(calls.get() + 1);
                marker(2)
            },
        );
        assert_eq!(calls.get(), 1, "guard-denied result must not be cached");
        assert_eq!(recomputed.success_type(), Some(TypeId(2)));
    }
// TSZ_INLINE_TEST_END 7bd910d6a0bc9578fc6df5aa2583658c6e567853279b47649d32024def17b86b

// TSZ_INLINE_TEST_BEGIN 73a1af205662627ca09e785366b4e1b430a21b37ffd30362df0fdb0353a99cff 1866 deferred_conditional_property_access_populates_local_memo
    #[test]
    fn deferred_conditional_property_access_populates_local_memo() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let common = interner.intern_string("common");
        let t_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let true_branch = interner.object(vec![PropertyInfo::new(common, TypeId::NUMBER)]);
        let false_branch = interner.object(vec![PropertyInfo::new(common, TypeId::STRING)]);
        let cond = interner.conditional(ConditionalType {
            check_type: t_param,
            extends_type: TypeId::STRING,
            true_type: true_branch,
            false_type: false_branch,
            is_distributive: true,
        });

        let first = evaluator.resolve_property_access_atom(cond, common);
        assert!(
            matches!(first, PropertyAccessResult::Success { .. }),
            "common property should resolve through deferred conditional apparent union"
        );
        assert_eq!(evaluator.deferred_property_memo_entries(), 1);

        let second = evaluator.resolve_property_access_atom(cond, common);
        assert!(
            matches!(second, PropertyAccessResult::Success { .. }),
            "memoized deferred conditional fallback must preserve top-level correction"
        );
        assert_eq!(
            evaluator.deferred_property_memo_entries(),
            1,
            "second access should reuse the existing deferred conditional memo entry"
        );
    }
// TSZ_INLINE_TEST_END 73a1af205662627ca09e785366b4e1b430a21b37ffd30362df0fdb0353a99cff

// TSZ_INLINE_TEST_BEGIN 2808b92d83ffd0b270b303a0584cb2cf041514df69d1d54098e5b32817c47916 1907 deferred_index_access_property_access_populates_local_memo
    #[test]
    fn deferred_index_access_property_access_populates_local_memo() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let name_atom = interner.intern_string("name");
        let age_atom = interner.intern_string("age");
        let obj = interner.object(vec![
            PropertyInfo::new(name_atom, TypeId::STRING),
            PropertyInfo::new(age_atom, TypeId::NUMBER),
        ]);
        let key_param = interner.type_param(TypeParamInfo {
            name: interner.intern_string("K"),
            constraint: Some(interner.keyof(obj)),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let indexed = interner.index_access(obj, key_param);

        let first = evaluator.resolve_property_access_atom(indexed, name_atom);
        let first_type = first
            .success_type()
            .expect("deferred member should succeed");
        assert!(
            matches!(
                interner.lookup(first_type),
                Some(TypeData::IndexAccess(source, prop))
                    if source == indexed && prop == interner.literal_string_atom(name_atom)
            ),
            "homomorphic deferred property access should stay indexed"
        );
        assert_eq!(evaluator.deferred_property_memo_entries(), 1);

        let second = evaluator.resolve_property_access_atom(indexed, name_atom);
        assert_eq!(second.success_type(), Some(first_type));
        assert_eq!(
            evaluator.deferred_property_memo_entries(),
            1,
            "second access should reuse the existing deferred index-access memo entry"
        );
    }
// TSZ_INLINE_TEST_END 2808b92d83ffd0b270b303a0584cb2cf041514df69d1d54098e5b32817c47916

// TSZ_INLINE_TEST_BEGIN feef900ec2999302d5f15f0c466fa722bb016f25d1c946a22c6ab21e94f97dd2 1949 type_query_property_access_populates_local_memo
    #[test]
    fn type_query_property_access_populates_local_memo() {
        let interner = TypeInterner::new();
        let evaluator = PropertyAccessEvaluator::new(&interner);
        let prop = interner.intern_string("value");
        let query = interner.type_query(crate::SymbolRef(77));

        let first = evaluator.resolve_property_access_atom(query, prop);
        assert_eq!(first.success_type(), Some(TypeId::ANY));
        assert_eq!(evaluator.deferred_property_memo_entries(), 1);

        let second = evaluator.resolve_property_access_atom(query, prop);
        assert_eq!(second.success_type(), Some(TypeId::ANY));
        assert_eq!(
            evaluator.deferred_property_memo_entries(),
            1,
            "second access should reuse the existing type-query memo entry"
        );
    }
// TSZ_INLINE_TEST_END feef900ec2999302d5f15f0c466fa722bb016f25d1c946a22c6ab21e94f97dd2
