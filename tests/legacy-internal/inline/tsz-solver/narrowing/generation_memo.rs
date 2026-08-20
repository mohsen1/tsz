//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/narrowing/generation_memo.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e23b4cc54fc9cc6f57b9fae68b93de7d41602e3b77eef998989a5b71f396e868 94 serves_only_the_matching_generation
    #[test]
    fn serves_only_the_matching_generation() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        memo.insert(TypeId::STRING, 7, TypeId::NUMBER);

        assert_eq!(memo.get(&TypeId::STRING, 7), Some(TypeId::NUMBER));
        assert_eq!(memo.get(&TypeId::STRING, 8), None);
        assert_eq!(memo.get(&TypeId::BOOLEAN, 7), None);
    }
// TSZ_INLINE_TEST_END e23b4cc54fc9cc6f57b9fae68b93de7d41602e3b77eef998989a5b71f396e868

// TSZ_INLINE_TEST_BEGIN eee8fd9eef652f687eef00c15c30d38488d65178a10d13970dd56548b9899dd2 105 reinsert_at_same_generation_updates_in_place
    #[test]
    fn reinsert_at_same_generation_updates_in_place() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        memo.insert(TypeId::STRING, 3, TypeId::NUMBER);
        memo.insert(TypeId::STRING, 3, TypeId::BOOLEAN);

        assert_eq!(memo.len(), 1);
        assert_eq!(memo.get(&TypeId::STRING, 3), Some(TypeId::BOOLEAN));
    }
// TSZ_INLINE_TEST_END eee8fd9eef652f687eef00c15c30d38488d65178a10d13970dd56548b9899dd2

// TSZ_INLINE_TEST_BEGIN e052b3cb9ec540789b27d07706ef552e369b10ca2d8f41dc02e5e8841fda34d9 116 bounds_retained_generations_per_key_and_evicts_oldest
    #[test]
    fn bounds_retained_generations_per_key_and_evicts_oldest() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        for generation in 1..=(MAX_GENERATIONS_PER_NARROWING_KEY as u64 + 3) {
            memo.insert(TypeId::STRING, generation, TypeId::NUMBER);
        }

        assert_eq!(memo.len(), MAX_GENERATIONS_PER_NARROWING_KEY);

        for generation in 1..=3 {
            assert_eq!(memo.get(&TypeId::STRING, generation), None);
        }
        for generation in 4..=7 {
            assert_eq!(memo.get(&TypeId::STRING, generation), Some(TypeId::NUMBER));
        }
    }
// TSZ_INLINE_TEST_END e052b3cb9ec540789b27d07706ef552e369b10ca2d8f41dc02e5e8841fda34d9

// TSZ_INLINE_TEST_BEGIN de5f288c816b1b6db4202d3e1c02952ac7cd2a57107bcc429bf5819baea54da5 134 bounds_each_key_independently
    #[test]
    fn bounds_each_key_independently() {
        let mut memo = GenerationMemo::<TypeId, TypeId>::default();

        for generation in 1..=6 {
            memo.insert(TypeId::STRING, generation, TypeId::NUMBER);
            memo.insert(TypeId::BOOLEAN, generation, TypeId::STRING);
        }

        assert_eq!(memo.len(), MAX_GENERATIONS_PER_NARROWING_KEY * 2);
        assert_eq!(memo.get(&TypeId::STRING, 6), Some(TypeId::NUMBER));
        assert_eq!(memo.get(&TypeId::BOOLEAN, 6), Some(TypeId::STRING));
        assert_eq!(memo.get(&TypeId::STRING, 1), None);
        assert_eq!(memo.get(&TypeId::BOOLEAN, 1), None);
    }
// TSZ_INLINE_TEST_END de5f288c816b1b6db4202d3e1c02952ac7cd2a57107bcc429bf5819baea54da5
