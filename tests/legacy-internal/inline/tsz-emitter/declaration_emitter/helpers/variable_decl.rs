//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/variable_decl.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 32bdce3b0a6f9cb151d1d44fec574dc949dbb4fff3bdff31ee2a1ef7f9b59824 1973 synthetic_extends_source_type_policy_prefers_intersections
    #[test]
    fn synthetic_extends_source_type_policy_prefers_intersections() {
        assert!(
            DeclarationEmitter::should_prefer_synthetic_extends_source_type_text(
                "CtorA & CtorB",
                "CtorA",
            )
        );
    }
// TSZ_INLINE_TEST_END 32bdce3b0a6f9cb151d1d44fec574dc949dbb4fff3bdff31ee2a1ef7f9b59824

// TSZ_INLINE_TEST_BEGIN f1ccff2c01438340330afe40ae7137e5b957a39ae6ce59485ee7ba66054cabdf 1983 synthetic_extends_source_type_policy_prefers_constructor_object_for_conditional_infer
    #[test]
    fn synthetic_extends_source_type_policy_prefers_constructor_object_for_conditional_infer() {
        assert!(
            DeclarationEmitter::should_prefer_synthetic_extends_source_type_text(
                "{ new (): X; prototype: X }",
                "T extends U ? infer R : never",
            )
        );
    }
// TSZ_INLINE_TEST_END f1ccff2c01438340330afe40ae7137e5b957a39ae6ce59485ee7ba66054cabdf

// TSZ_INLINE_TEST_BEGIN bd228742f65bb9b1aa1090b2b1b61fe1d33b9abe7f47873f28c8cfddb914728a 1993 synthetic_extends_source_type_policy_keeps_plain_printed_type
    #[test]
    fn synthetic_extends_source_type_policy_keeps_plain_printed_type() {
        assert!(
            !DeclarationEmitter::should_prefer_synthetic_extends_source_type_text("Ctor", "Ctor",)
        );
    }
// TSZ_INLINE_TEST_END bd228742f65bb9b1aa1090b2b1b61fe1d33b9abe7f47873f28c8cfddb914728a
