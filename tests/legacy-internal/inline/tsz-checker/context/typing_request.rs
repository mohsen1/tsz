//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/typing_request.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 6c90170f523ae9b32cf442d16e7932b76d20df919c162a73b6cfc805e185ad08 210 default_request_is_none
    #[test]
    fn default_request_is_none() {
        let req = TypingRequest::default();
        assert_eq!(req, TypingRequest::NONE);
        assert!(req.is_empty());
        assert!(!req.flow.skip_flow_narrowing());
        assert!(!req.origin.is_assertion());
    }
// TSZ_INLINE_TEST_END 6c90170f523ae9b32cf442d16e7932b76d20df919c162a73b6cfc805e185ad08

// TSZ_INLINE_TEST_BEGIN 79ff6c81c83ce03ca4729b0f97bda9233e0a172fe9cb04c5eb88707cd94ed7cd 219 with_contextual_type_sets_type
    #[test]
    fn with_contextual_type_sets_type() {
        let req = TypingRequest::with_contextual_type(TypeId::STRING);
        assert_eq!(req.contextual_type, Some(TypeId::STRING));
        assert!(!req.origin.is_assertion());
        assert!(!req.flow.skip_flow_narrowing());
        assert!(!req.is_empty());
    }
// TSZ_INLINE_TEST_END 79ff6c81c83ce03ca4729b0f97bda9233e0a172fe9cb04c5eb88707cd94ed7cd

// TSZ_INLINE_TEST_BEGIN 1e64ca84df5afcd55dbd7612c99456cd98d0eef315f56fc253ce531ddd7955c1 228 for_assertion_sets_origin
    #[test]
    fn for_assertion_sets_origin() {
        let req = TypingRequest::for_assertion(TypeId::NUMBER);
        assert_eq!(req.contextual_type, Some(TypeId::NUMBER));
        assert!(req.origin.is_assertion());
        assert!(!req.flow.skip_flow_narrowing());
    }
// TSZ_INLINE_TEST_END 1e64ca84df5afcd55dbd7612c99456cd98d0eef315f56fc253ce531ddd7955c1

// TSZ_INLINE_TEST_BEGIN 316c224667291b33e4bfb6e05f7ce4b817a99a27f0211d4b8898a762ca46a8c3 236 for_write_context_skips_flow
    #[test]
    fn for_write_context_skips_flow() {
        let req = TypingRequest::for_write_context();
        assert!(req.flow.skip_flow_narrowing());
        assert!(req.contextual_type.is_none());
    }
// TSZ_INLINE_TEST_END 316c224667291b33e4bfb6e05f7ce4b817a99a27f0211d4b8898a762ca46a8c3

// TSZ_INLINE_TEST_BEGIN c1a7a32c78c8462f515102c5406e75e185a8fbeb41c8104f08849e9f87915030 243 builder_chain
    #[test]
    fn builder_chain() {
        let req = TypingRequest::NONE
            .contextual(TypeId::BOOLEAN)
            .assertion()
            .write();
        assert_eq!(req.contextual_type, Some(TypeId::BOOLEAN));
        assert!(req.origin.is_assertion());
        assert!(req.flow.skip_flow_narrowing());
    }
// TSZ_INLINE_TEST_END c1a7a32c78c8462f515102c5406e75e185a8fbeb41c8104f08849e9f87915030
