//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/declarations/class/static_field_erasure.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6a604c524d2289a82aac7919344e29c13563e952cd5130aee455f4422812d255 49 no_init_with_define_is_erased
    #[test]
    fn no_init_with_define_is_erased() {
        assert!(static_no_init_field_is_erased(true, true));
    }
// TSZ_INLINE_TEST_END 6a604c524d2289a82aac7919344e29c13563e952cd5130aee455f4422812d255

// TSZ_INLINE_TEST_BEGIN e7e5304f7e132ae708e596ec8af0a2e9e1900a1e2fcbca6ad92ebb017aa6e28e 54 initialized_static_field_is_not_erased
    #[test]
    fn initialized_static_field_is_not_erased() {
        assert!(!static_no_init_field_is_erased(false, true));
    }
// TSZ_INLINE_TEST_END e7e5304f7e132ae708e596ec8af0a2e9e1900a1e2fcbca6ad92ebb017aa6e28e

// TSZ_INLINE_TEST_BEGIN 217209a63ba8aff87d385736cdce946050b5d7023b6c8ee8fb3543aaea0d6b73 59 no_init_without_define_is_not_erased
    #[test]
    fn no_init_without_define_is_not_erased() {
        // Without define semantics a bare typed static field has no runtime
        // form anyway; the caller filters it earlier, but the predicate must
        // not claim erasure on its own.
        assert!(!static_no_init_field_is_erased(true, false));
    }
// TSZ_INLINE_TEST_END 217209a63ba8aff87d385736cdce946050b5d7023b6c8ee8fb3543aaea0d6b73
