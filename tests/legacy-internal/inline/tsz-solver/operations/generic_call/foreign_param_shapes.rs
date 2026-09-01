//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/generic_call/foreign_param_shapes.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ade91f1a71e1e64a4d0b00717782b38873e25fd4ed09651c9f78a3be2bb25ed3 135 reconstructed_local_placeholder_is_not_foreign
    #[test]
    fn reconstructed_local_placeholder_is_not_foreign() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("__local_placeholder");
        let info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::InferPlaceholder { id: 7 },
        };
        let original = interner.fresh_type_param(info);
        let reconstructed = interner.fresh_type_param(info);
        assert_ne!(original, reconstructed);
        let local_placeholders = FxHashMap::from_iter([(original, InferenceVar(0))]);

        assert!(!is_bare_foreign_type_param(
            &interner,
            reconstructed,
            &[],
            &local_placeholders,
        ));

        for origin in [
            TypeParamOrigin::User,
            TypeParamOrigin::DeclScoped {
                file: interner.intern_string("user-source.ts"),
                node: 1,
            },
        ] {
            let user_param = interner.fresh_type_param(TypeParamInfo { origin, ..info });
            assert!(
                is_bare_foreign_type_param(&interner, user_param, &[], &local_placeholders),
                "a same-spelled user binder must remain foreign"
            );
        }

        let unrelated_placeholder = interner.fresh_type_param(TypeParamInfo {
            origin: TypeParamOrigin::InferPlaceholder { id: 8 },
            ..info
        });
        assert!(is_bare_foreign_type_param(
            &interner,
            unrelated_placeholder,
            &[],
            &local_placeholders,
        ));
    }
// TSZ_INLINE_TEST_END ade91f1a71e1e64a4d0b00717782b38873e25fd4ed09651c9f78a3be2bb25ed3
