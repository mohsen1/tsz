//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/functions/checking/name_pairing.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c751f86c2cfeb6ceff24111dc4e2dab07a3d6e94d19d1942faf870cec3b8491f 76 identity_order_yields_identity_permutation
    #[test]
    fn identity_order_yields_identity_permutation() {
        let src = [tp(1), tp(2), tp(3)];
        let tgt = [tp(1), tp(2), tp(3)];
        assert_eq!(
            name_aware_target_permutation(&src, &tgt),
            Some(vec![0, 1, 2])
        );
    }
// TSZ_INLINE_TEST_END c751f86c2cfeb6ceff24111dc4e2dab07a3d6e94d19d1942faf870cec3b8491f

// TSZ_INLINE_TEST_BEGIN 7e28783d49dc6e6b55a8c10fdc687bc1747e17e1bfaa7dc206834dcf7e88f823 86 reordered_same_names_pair_by_name
    #[test]
    fn reordered_same_names_pair_by_name() {
        // source <A,E> (1,2) vs target <E,A> (2,1): source[0]=A pairs target[1],
        // source[1]=E pairs target[0].
        let src = [tp(1), tp(2)];
        let tgt = [tp(2), tp(1)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), Some(vec![1, 0]));
    }
// TSZ_INLINE_TEST_END 7e28783d49dc6e6b55a8c10fdc687bc1747e17e1bfaa7dc206834dcf7e88f823

// TSZ_INLINE_TEST_BEGIN 8a243288da04dd8978958b51f2130b3635ee7d9d14d7ebd37065d129b12f6733 95 different_names_fall_back_to_positional
    #[test]
    fn different_names_fall_back_to_positional() {
        // source <A,E> (1,2) vs target <T,U> (3,4): no shared names -> None.
        let src = [tp(1), tp(2)];
        let tgt = [tp(3), tp(4)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }
// TSZ_INLINE_TEST_END 8a243288da04dd8978958b51f2130b3635ee7d9d14d7ebd37065d129b12f6733

// TSZ_INLINE_TEST_BEGIN 7b3ebef7a7ee3378d6d3b5af33de3e574386b73f088f7b83a183acfc65cca116 103 length_mismatch_is_none
    #[test]
    fn length_mismatch_is_none() {
        let src = [tp(1), tp(2)];
        let tgt = [tp(1)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }
// TSZ_INLINE_TEST_END 7b3ebef7a7ee3378d6d3b5af33de3e574386b73f088f7b83a183acfc65cca116

// TSZ_INLINE_TEST_BEGIN f5ef51b6902d5f4f7e2dc96e889f33a21482f128357c34d9a744c596bb491e14 110 repeated_names_consume_each_target_once
    #[test]
    fn repeated_names_consume_each_target_once() {
        // multiset {A,A,E} vs {A,E,A}: each source consumes one unused same-named
        // target in order.
        let src = [tp(1), tp(1), tp(2)];
        let tgt = [tp(1), tp(2), tp(1)];
        assert_eq!(
            name_aware_target_permutation(&src, &tgt),
            Some(vec![0, 2, 1])
        );
    }
// TSZ_INLINE_TEST_END f5ef51b6902d5f4f7e2dc96e889f33a21482f128357c34d9a744c596bb491e14

// TSZ_INLINE_TEST_BEGIN 59270520e60604fe8054cb764bb4f3249e702c7526aaac8a669f65ea9af2a916 122 unequal_multiset_same_len_is_none
    #[test]
    fn unequal_multiset_same_len_is_none() {
        // {A,A} vs {A,E}: second source A finds no second target A -> None.
        let src = [tp(1), tp(1)];
        let tgt = [tp(1), tp(2)];
        assert_eq!(name_aware_target_permutation(&src, &tgt), None);
    }
// TSZ_INLINE_TEST_END 59270520e60604fe8054cb764bb4f3249e702c7526aaac8a669f65ea9af2a916
