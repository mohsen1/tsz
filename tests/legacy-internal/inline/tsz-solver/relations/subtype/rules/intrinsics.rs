//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/intrinsics.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5e46559a21042e137ff4f1ba27d3ec8c397a5c302e31f1f24f0cb94ae83446f7 801 apparent_primitive_shape_is_cached_per_checker
    #[test]
    fn apparent_primitive_shape_is_cached_per_checker() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        let first = checker
            .apparent_primitive_shape_for_type(TypeId::STRING)
            .expect("string should have an apparent shape");
        let cached_after_first = checker.apparent_primitive_shapes.clone();
        let second = checker
            .apparent_primitive_shape_for_type(TypeId::STRING)
            .expect("string should reuse its apparent shape");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            cached_after_first, checker.apparent_primitive_shapes,
            "repeated lookup should not intern another apparent shape"
        );
        assert_eq!(
            checker
                .apparent_primitive_shapes
                .iter()
                .filter(|entry| entry.is_some())
                .count(),
            1
        );
    }
// TSZ_INLINE_TEST_END 5e46559a21042e137ff4f1ba27d3ec8c397a5c302e31f1f24f0cb94ae83446f7

// TSZ_INLINE_TEST_BEGIN c77d93817c6d3879cfa2310ba0e18eafb38195f47fc814a0c66a227ba68de332 829 unresolved_lazy_object_keyword_probe_records_relation_event
    #[test]
    fn unresolved_lazy_object_keyword_probe_records_relation_event() {
        crate::limits::reset_subtype_thread_local_state();
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);
        let unresolved = interner.lazy(DefId(9001));
        let before = checker.unresolved_lazy_relation_event_count();

        assert!(!checker.is_object_keyword_type(unresolved));
        assert_ne!(
            checker.unresolved_lazy_relation_event_count(),
            before,
            "Lazy <: object miss must keep the relation result non-cacheable"
        );
        crate::limits::reset_subtype_thread_local_state();
    }
// TSZ_INLINE_TEST_END c77d93817c6d3879cfa2310ba0e18eafb38195f47fc814a0c66a227ba68de332
