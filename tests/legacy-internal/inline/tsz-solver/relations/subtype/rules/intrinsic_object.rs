//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/intrinsic_object.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e4824a11faf8738dd8871a53494fb3c70d489a6b898037043011f333506ca87d 115 test_intrinsic_object_matrix
    /// Verify the full 13-row × 3-column matrix of `intrinsic_vs_object_super`.
    #[test]
    fn test_intrinsic_object_matrix() {
        use IntrinsicKind as K;
        use IntrinsicObjectKind as T;

        // never → true for all three
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::ObjectKeyword),
            Some(true)
        );
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::EmptyObject),
            Some(true)
        );
        assert_eq!(
            intrinsic_vs_object_super(K::Never, T::GlobalObject),
            Some(true)
        );

        // nullish / void / unknown → false for all three
        for nullish in [K::Null, K::Undefined, K::Void, K::Unknown] {
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::ObjectKeyword),
                Some(false),
                "expected {nullish:?} <: object = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::EmptyObject),
                Some(false),
                "expected {nullish:?} <: {{}} = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(nullish, T::GlobalObject),
                Some(false),
                "expected {nullish:?} <: Object = false"
            );
        }

        // primitives → false for object, true for {} and Object
        for prim in [K::String, K::Number, K::Boolean, K::Bigint, K::Symbol] {
            assert_eq!(
                intrinsic_vs_object_super(prim, T::ObjectKeyword),
                Some(false),
                "expected {prim:?} <: object = false"
            );
            assert_eq!(
                intrinsic_vs_object_super(prim, T::EmptyObject),
                Some(true),
                "expected {prim:?} <: {{}} = true"
            );
            assert_eq!(
                intrinsic_vs_object_super(prim, T::GlobalObject),
                Some(true),
                "expected {prim:?} <: Object = true"
            );
        }

        // object/Function → true for all three
        for non_prim in [K::Object, K::Function] {
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::ObjectKeyword),
                Some(true)
            );
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::EmptyObject),
                Some(true)
            );
            assert_eq!(
                intrinsic_vs_object_super(non_prim, T::GlobalObject),
                Some(true)
            );
        }

        // any → None for all three (context-dependent)
        assert_eq!(intrinsic_vs_object_super(K::Any, T::ObjectKeyword), None);
        assert_eq!(intrinsic_vs_object_super(K::Any, T::EmptyObject), None);
        assert_eq!(intrinsic_vs_object_super(K::Any, T::GlobalObject), None);
    }
// TSZ_INLINE_TEST_END e4824a11faf8738dd8871a53494fb3c70d489a6b898037043011f333506ca87d
