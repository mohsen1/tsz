//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_imported_calls.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 067cd1f582320b3cc6dc3d44d60173eb357bd651fc42db42dae3504f807c93c8 611 nested_arrow_object_type_text_normalizes_inner_indent
    #[test]
    fn nested_arrow_object_type_text_normalizes_inner_indent() {
        let normalized = DeclarationEmitter::normalize_nested_arrow_object_type_text(
            "() => {\n        value: string;\n    }",
        );

        assert_eq!(normalized, "() => {\n    value: string;\n}");
    }
// TSZ_INLINE_TEST_END 067cd1f582320b3cc6dc3d44d60173eb357bd651fc42db42dae3504f807c93c8

// TSZ_INLINE_TEST_BEGIN bfa3b565732ea466035d00762a1dbc5dff84cc6804a54d573e7da9dd35117653 620 nested_arrow_object_type_text_keeps_unmatched_text_unchanged
    #[test]
    fn nested_arrow_object_type_text_keeps_unmatched_text_unchanged() {
        let source = "() => { value: string; }";

        assert_eq!(
            DeclarationEmitter::normalize_nested_arrow_object_type_text(source),
            source
        );
    }
// TSZ_INLINE_TEST_END bfa3b565732ea466035d00762a1dbc5dff84cc6804a54d573e7da9dd35117653
