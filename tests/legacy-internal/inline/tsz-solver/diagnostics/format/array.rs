//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/diagnostics/format/array.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 38ee8b35497e64181e8ae47fdacc5df32f5410c2518f6dcac800058e767467d8 42 format_array_of_infer_is_parenthesized
    #[test]
    fn format_array_of_infer_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let arr = db.array(infer);
        assert_eq!(fmt.format(arr), "(infer E)[]");
    }
// TSZ_INLINE_TEST_END 38ee8b35497e64181e8ae47fdacc5df32f5410c2518f6dcac800058e767467d8

// TSZ_INLINE_TEST_BEGIN 69138be02e773eb2915687eec922ffda1c95edcd953c559135edcb9351608f6d 58 format_array_of_infer_is_parenthesized_with_renamed_binder
    #[test]
    fn format_array_of_infer_is_parenthesized_with_renamed_binder() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        for name in ["E", "Q", "U", "_T"] {
            let infer = db.infer(TypeParamInfo {
                name: db.intern_string(name),
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            });
            let arr = db.array(infer);
            assert_eq!(fmt.format(arr), format!("(infer {name})[]"));
        }
    }
// TSZ_INLINE_TEST_END 69138be02e773eb2915687eec922ffda1c95edcd953c559135edcb9351608f6d

// TSZ_INLINE_TEST_BEGIN a28a3c4ae7c1015001bfac950b8ad2d1147d55501fb96d942e83ae3f4e52f9d7 76 format_array_of_infer_with_constraint_is_parenthesized
    #[test]
    fn format_array_of_infer_with_constraint_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let arr = db.array(infer);
        assert_eq!(fmt.format(arr), "(infer E extends string)[]");
    }
// TSZ_INLINE_TEST_END a28a3c4ae7c1015001bfac950b8ad2d1147d55501fb96d942e83ae3f4e52f9d7

// TSZ_INLINE_TEST_BEGIN 8e84a01347756dda6eaef8188fdb58a5f597ea9a3a116e3fa3a0ca2722bef67f 92 format_array_of_conditional_is_parenthesized
    #[test]
    fn format_array_of_conditional_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cond = db.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::BOOLEAN,
            false_type: TypeId::NEVER,
            is_distributive: false,
        });
        let arr = db.array(cond);
        let result = fmt.format(arr);
        assert_eq!(
            result, "(string extends number ? boolean : never)[]",
            "Array of conditional should be parenthesized, got: {result}"
        );
    }
// TSZ_INLINE_TEST_END 8e84a01347756dda6eaef8188fdb58a5f597ea9a3a116e3fa3a0ca2722bef67f

// TSZ_INLINE_TEST_BEGIN 88e2201ff54b6e5af91e2b988ebcdd7dcbefaa4bde8a866b4beefb26851b5182 112 format_array_of_keyof_is_parenthesized
    #[test]
    fn format_array_of_keyof_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tp = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let keyof = db.keyof(tp);
        let arr = db.array(keyof);
        assert_eq!(fmt.format(arr), "(keyof T)[]");
    }
// TSZ_INLINE_TEST_END 88e2201ff54b6e5af91e2b988ebcdd7dcbefaa4bde8a866b4beefb26851b5182

// TSZ_INLINE_TEST_BEGIN c31910952e3e0fcc82c80a3feb5aeed8eb18f3456924860c99d39fddf2a4d0a8 129 format_array_of_primitive_is_unparenthesized_control
    #[test]
    fn format_array_of_primitive_is_unparenthesized_control() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
    }
// TSZ_INLINE_TEST_END c31910952e3e0fcc82c80a3feb5aeed8eb18f3456924860c99d39fddf2a4d0a8

// TSZ_INLINE_TEST_BEGIN 0fad589f85f72955f764631d0b459a3d8e1488729b3a69c7f55174f328aed1b9 137 format_readonly_array_of_infer_is_parenthesized
    #[test]
    fn format_readonly_array_of_infer_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });

        let readonly_array_base = db.unresolved_type_name(db.intern_string("ReadonlyArray"));
        let app = db.application(readonly_array_base, vec![infer]);
        assert_eq!(fmt.format(app), "readonly (infer E)[]");
    }
// TSZ_INLINE_TEST_END 0fad589f85f72955f764631d0b459a3d8e1488729b3a69c7f55174f328aed1b9

// TSZ_INLINE_TEST_BEGIN fbe31c6c6ce3ef6415d63497e805bf91f66904f197c6a74a9e2c68d015c9cda9 155 format_array_of_readonly_array_is_parenthesized
    #[test]
    fn format_array_of_readonly_array_is_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // `(readonly string[])[]`: the inner readonly array must be parenthesized
        // as an array element, otherwise the `readonly` prefix reads as applying
        // to the outer `[][]` (`readonly string[][]`).
        let readonly_string_array = db.readonly_type(db.array(TypeId::STRING));
        let outer = db.array(readonly_string_array);
        assert_eq!(fmt.format(outer), "(readonly string[])[]");
    }
// TSZ_INLINE_TEST_END fbe31c6c6ce3ef6415d63497e805bf91f66904f197c6a74a9e2c68d015c9cda9

// TSZ_INLINE_TEST_BEGIN 8d2c32bfce6f2ea365f1bbfea14ad5fe9c34316648e7b923de238f8851397808 168 format_infer_with_constraint_includes_extends
    #[test]
    fn format_infer_with_constraint_includes_extends() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("E"),
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert_eq!(fmt.format(infer), "infer E extends string");
    }
// TSZ_INLINE_TEST_END 8d2c32bfce6f2ea365f1bbfea14ad5fe9c34316648e7b923de238f8851397808
