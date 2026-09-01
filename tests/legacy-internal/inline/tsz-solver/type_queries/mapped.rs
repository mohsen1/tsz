//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/mapped.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fa28618c312eac3ed674e0a591b80f209d949fabf973f9d0e73c45e194a43ff8 1683 test_identity_mapped_passthrough_concrete_primitive
    #[test]
    fn test_identity_mapped_passthrough_concrete_primitive() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build: { [K in keyof T]: T[K] } where T is a type parameter
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let constraint = interner.keyof(t_param);
        let template = interner.index_access(t_param, k_param);
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint,
            name_type: None,
            template,
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("should be a mapped type");

        // Concrete primitives pass through
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::STRING),
            Some(TypeId::STRING)
        );
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::BOOLEAN),
            Some(TypeId::BOOLEAN)
        );
    }
// TSZ_INLINE_TEST_END fa28618c312eac3ed674e0a591b80f209d949fabf973f9d0e73c45e194a43ff8

// TSZ_INLINE_TEST_BEGIN f7fa5e239b3e228231f5eacb2ec4f8247618bbd707037457447f2c7c2424bb7b 1741 test_identity_mapped_passthrough_any_no_constraint
    #[test]
    fn test_identity_mapped_passthrough_any_no_constraint() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build identity mapped type with unconstrained T
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // `any` with no array constraint -> produces object with index signatures (not `any`)
        let result = evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY);
        assert!(result.is_some());
        let result = result.expect("result should be Some");
        assert_ne!(
            result,
            TypeId::ANY,
            "Objectish<any> should not passthrough to any"
        );

        // unknown with no array constraint -> no passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::UNKNOWN),
            None
        );
    }
// TSZ_INLINE_TEST_END f7fa5e239b3e228231f5eacb2ec4f8247618bbd707037457447f2c7c2424bb7b

// TSZ_INLINE_TEST_BEGIN e7295eedbee3d6e6f32b803229c9f0911b4416325794ffd7dd1ae7d3fe0c7a6f 1799 test_identity_mapped_passthrough_any_with_array_constraint
    #[test]
    fn test_identity_mapped_passthrough_any_with_array_constraint() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build identity mapped type with T extends any[]
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let array_constraint = interner.factory().array(TypeId::ANY);
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: Some(array_constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // `any` with array constraint -> passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY),
            Some(TypeId::ANY)
        );
    }
// TSZ_INLINE_TEST_END e7295eedbee3d6e6f32b803229c9f0911b4416325794ffd7dd1ae7d3fe0c7a6f

// TSZ_INLINE_TEST_BEGIN ef27b73bca78816c21ec204f30533dc3a967ddc471a82fb6b37c96d7e76d08e7 1848 test_identity_mapped_passthrough_non_identity
    #[test]
    fn test_identity_mapped_passthrough_non_identity() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        // Build non-identity mapped type: { [K in keyof T]: string }
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: TypeId::STRING, // Non-identity: template is string, not T[K]
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        // Non-identity mapped type -> no passthrough
        assert_eq!(
            evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            None
        );
    }
// TSZ_INLINE_TEST_END ef27b73bca78816c21ec204f30533dc3a967ddc471a82fb6b37c96d7e76d08e7

// TSZ_INLINE_TEST_BEGIN d84d2e3f45792e6e47321db7b89b42760909503e95e705dc30973e1f82f496be 1889 finite_mapped_property_display_type_preserves_raw_index_access_surface
    #[test]
    fn finite_mapped_property_display_type_preserves_raw_index_access_surface() {
        use crate::types::MappedType;

        let interner = TypeInterner::new();

        let s_name = interner.intern_string("S");
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let a_name = interner.intern_string("a");

        let s_param = interner.type_param(TypeParamInfo {
            name: s_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let key_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });

        let state = interner.object(vec![crate::types::PropertyInfo::opt(a_name, t_param)]);
        let source = interner.intersection(vec![s_param, state]);
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: interner.literal_string("a"),
            name_type: None,
            template: interner.index_access(source, key_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("mapped type should have id");

        let actual = get_finite_mapped_property_display_type(&interner, mapped_id, "a")
            .expect("display type should resolve");
        let expected = interner.union2(
            interner.index_access(source, interner.literal_string("a")),
            TypeId::UNDEFINED,
        );

        assert_eq!(actual, expected);
    }
// TSZ_INLINE_TEST_END d84d2e3f45792e6e47321db7b89b42760909503e95e705dc30973e1f82f496be
