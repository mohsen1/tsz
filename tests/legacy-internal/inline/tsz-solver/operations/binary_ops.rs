//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/binary_ops.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 48678282cc68c13e819f40c790dff483930987447fa7db98e410ae60680ef38e 1431 valid_key_type_visit_state_enters_new_type
    #[test]
    fn valid_key_type_visit_state_enters_new_type() {
        let mut seen = FxHashSet::default();

        let state = ValidKeyTypeVisitState::record(TypeId::STRING, false, &mut seen);

        assert_eq!(state, ValidKeyTypeVisitState::Entered);
        assert!(seen.contains(&TypeId::STRING));
    }
// TSZ_INLINE_TEST_END 48678282cc68c13e819f40c790dff483930987447fa7db98e410ae60680ef38e

// TSZ_INLINE_TEST_BEGIN 97dcc159149f6f3af1406a1174258eff23900432c2e68a2b18fec797e3ef3cb2 1441 valid_key_type_visit_state_reentry_keeps_concrete_fallback
    #[test]
    fn valid_key_type_visit_state_reentry_keeps_concrete_fallback() {
        let mut seen = FxHashSet::default();

        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::STRING, false, &mut seen),
            ValidKeyTypeVisitState::Entered
        );
        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::STRING, false, &mut seen),
            ValidKeyTypeVisitState::AlreadyVisited { fallback: false }
        );
    }
// TSZ_INLINE_TEST_END 97dcc159149f6f3af1406a1174258eff23900432c2e68a2b18fec797e3ef3cb2

// TSZ_INLINE_TEST_BEGIN 98a4cdb07252f302492694cc6d2876de7d754bc820674b63f6174c85224b7e43 1455 valid_key_type_visit_state_reentry_keeps_deferred_fallback
    #[test]
    fn valid_key_type_visit_state_reentry_keeps_deferred_fallback() {
        let mut seen = FxHashSet::default();

        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::STRING, true, &mut seen),
            ValidKeyTypeVisitState::Entered
        );
        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::STRING, true, &mut seen),
            ValidKeyTypeVisitState::AlreadyVisited { fallback: true }
        );
    }
// TSZ_INLINE_TEST_END 98a4cdb07252f302492694cc6d2876de7d754bc820674b63f6174c85224b7e43

// TSZ_INLINE_TEST_BEGIN defb614a8a1ff7e04afae3368faee9f9b9314aba4e5ecb69105ddaad37854b89 1469 valid_key_type_visit_state_distinguishes_types
    #[test]
    fn valid_key_type_visit_state_distinguishes_types() {
        let mut seen = FxHashSet::default();

        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::STRING, false, &mut seen),
            ValidKeyTypeVisitState::Entered
        );
        assert_eq!(
            ValidKeyTypeVisitState::record(TypeId::NUMBER, false, &mut seen),
            ValidKeyTypeVisitState::Entered
        );
    }
// TSZ_INLINE_TEST_END defb614a8a1ff7e04afae3368faee9f9b9314aba4e5ecb69105ddaad37854b89
