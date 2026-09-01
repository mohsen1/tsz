//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/class_type/helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a8b2aa76404e85dc74611a56ac18de52671f4ed455f9d1af38aef75c796f9fa2 806 skip_base_instantiation_only_without_generics
    #[test]
    fn skip_base_instantiation_only_without_generics() {
        assert!(can_skip_base_instantiation(0, 0));
        assert!(!can_skip_base_instantiation(1, 0));
        assert!(!can_skip_base_instantiation(0, 1));
        assert!(!can_skip_base_instantiation(2, 3));
    }
// TSZ_INLINE_TEST_END a8b2aa76404e85dc74611a56ac18de52671f4ed455f9d1af38aef75c796f9fa2

// TSZ_INLINE_TEST_BEGIN f6a641e1cd603c9921c1ee1bb74a728a2a3a4e05e7f2531f8c0ba4ecc98a8e1b 814 class_inheritance_depth_guard_is_conservative
    #[test]
    fn class_inheritance_depth_guard_is_conservative() {
        assert!(!exceeds_class_inheritance_depth_limit(1));
        assert!(!exceeds_class_inheritance_depth_limit(100));
        assert!(!exceeds_class_inheritance_depth_limit(256));
        assert!(exceeds_class_inheritance_depth_limit(257));
    }
// TSZ_INLINE_TEST_END f6a641e1cd603c9921c1ee1bb74a728a2a3a4e05e7f2531f8c0ba4ecc98a8e1b

// TSZ_INLINE_TEST_BEGIN 6bd02c8517b5ef86d8b9ca52da2fe38687b6e9d3f584e58d82d3c52a241575f4 822 in_progress_class_instance_uses_cached_or_error
    #[test]
    fn in_progress_class_instance_uses_cached_or_error() {
        assert_eq!(
            in_progress_class_instance_result(true, Some(TypeId(42))),
            Some(TypeId(42))
        );
        assert_eq!(
            in_progress_class_instance_result(true, None),
            Some(TypeId::ERROR)
        );
        assert_eq!(in_progress_class_instance_result(false, None), None);
    }
// TSZ_INLINE_TEST_END 6bd02c8517b5ef86d8b9ca52da2fe38687b6e9d3f584e58d82d3c52a241575f4
