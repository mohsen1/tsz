//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_function_text.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9884dd58198131e570d31b1274f291db5e0ea31ffea07467f32f627e60f47c20 526 rest_type_param_infers_rest_array_from_rest_callback
    #[test]
    fn rest_type_param_infers_rest_array_from_rest_callback() {
        let source = DeclarationEmitter::parse_function_type_text("(...args: A) => B").unwrap();
        let argument =
            DeclarationEmitter::parse_function_type_text("(...args: any[]) => boolean").unwrap();
        let mut substitutions = Vec::new();

        DeclarationEmitter::infer_function_type_substitutions(
            &source,
            &argument,
            &["A".to_string(), "B".to_string()],
            &[],
            &mut substitutions,
        );

        assert_eq!(
            substitutions,
            vec![
                ("A".to_string(), "any[]".to_string()),
                ("B".to_string(), "boolean".to_string()),
            ]
        );
    }
// TSZ_INLINE_TEST_END 9884dd58198131e570d31b1274f291db5e0ea31ffea07467f32f627e60f47c20

// TSZ_INLINE_TEST_BEGIN 2230d9e9c1ac54a9f9e383eaee9426b94b05f5df261b505f2b9dac24ce78d821 550 blocked_return_type_param_does_not_infer_from_callback_return
    #[test]
    fn blocked_return_type_param_does_not_infer_from_callback_return() {
        let source =
            DeclarationEmitter::parse_function_type_text("(a: NoInfer<Shape>, b: number) => Shape")
                .unwrap();
        let argument =
            DeclarationEmitter::parse_function_type_text("(a: unknown, b: number) => number")
                .unwrap();
        let mut substitutions = Vec::new();

        DeclarationEmitter::infer_function_type_substitutions(
            &source,
            &argument,
            &["Shape".to_string()],
            &["Shape".to_string()],
            &mut substitutions,
        );

        assert!(substitutions.is_empty());
    }
// TSZ_INLINE_TEST_END 2230d9e9c1ac54a9f9e383eaee9426b94b05f5df261b505f2b9dac24ce78d821
