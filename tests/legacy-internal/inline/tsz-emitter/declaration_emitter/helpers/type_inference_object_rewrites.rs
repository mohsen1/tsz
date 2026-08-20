//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_object_rewrites.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3af1d1a0736cab76315438e0a9715aceca00ff3ec07f5da8a7c530e1d81a4c8a 1548 top_level_union_is_parenthesized
    #[test]
    fn top_level_union_is_parenthesized() {
        assert_eq!(paren("string | number"), "(string | number)");
        assert_eq!(
            paren("boolean | bigint | symbol"),
            "(boolean | bigint | symbol)"
        );
    }
// TSZ_INLINE_TEST_END 3af1d1a0736cab76315438e0a9715aceca00ff3ec07f5da8a7c530e1d81a4c8a

// TSZ_INLINE_TEST_BEGIN 2aa66faad751f3c7ee24910c24cf3c32a23ae26d8d9792392faa054b39eef689 1557 top_level_union_with_object_members_is_parenthesized
    #[test]
    fn top_level_union_with_object_members_is_parenthesized() {
        assert_eq!(paren("{ a: 1 } | { b: 2 }"), "({ a: 1 } | { b: 2 })");
    }
// TSZ_INLINE_TEST_END 2aa66faad751f3c7ee24910c24cf3c32a23ae26d8d9792392faa054b39eef689

// TSZ_INLINE_TEST_BEGIN ac91b9b901c2ec3ee83e74be01a4af3beeea363b36d93773f1036d1e3119133f 1566 nested_union_inside_application_is_not_parenthesized
    #[test]
    fn nested_union_inside_application_is_not_parenthesized() {
        assert_eq!(paren("Box<string | number>"), "Box<string | number>");
        assert_eq!(paren("Map<string, A | B>"), "Map<string, A | B>");
    }
// TSZ_INLINE_TEST_END ac91b9b901c2ec3ee83e74be01a4af3beeea363b36d93773f1036d1e3119133f

// TSZ_INLINE_TEST_BEGIN 7b0a59634217a78be6632097ddfdb33756916521df24754483abe8da72debd29 1572 nested_union_inside_tuple_is_not_parenthesized
    #[test]
    fn nested_union_inside_tuple_is_not_parenthesized() {
        assert_eq!(
            paren("[string | number, boolean]"),
            "[string | number, boolean]"
        );
    }
// TSZ_INLINE_TEST_END 7b0a59634217a78be6632097ddfdb33756916521df24754483abe8da72debd29

// TSZ_INLINE_TEST_BEGIN a2cfbe94ebd0c1987d0bc760e83b49d9570f86462ce66a9af31dec2c3924f538 1582 top_level_intersection_is_parenthesized
    #[test]
    fn top_level_intersection_is_parenthesized() {
        assert_eq!(paren("A & B"), "(A & B)");
        assert_eq!(paren("{ x: 1 } & { y: 2 }"), "({ x: 1 } & { y: 2 })");
    }
// TSZ_INLINE_TEST_END a2cfbe94ebd0c1987d0bc760e83b49d9570f86462ce66a9af31dec2c3924f538

// TSZ_INLINE_TEST_BEGIN d32840063e5976001fd754bff27efc722ea22e49e28cb40f1dcdefedb16cd94e 1588 nested_intersection_inside_application_is_not_parenthesized
    #[test]
    fn nested_intersection_inside_application_is_not_parenthesized() {
        assert_eq!(paren("Foo<A & B>"), "Foo<A & B>");
    }
// TSZ_INLINE_TEST_END d32840063e5976001fd754bff27efc722ea22e49e28cb40f1dcdefedb16cd94e

// TSZ_INLINE_TEST_BEGIN a2bf19e99aa2c9ecb239a703d6dfd7853c55e4a50dc8ed4ea64fe366c49b5cef 1595 function_type_is_parenthesized
    #[test]
    fn function_type_is_parenthesized() {
        assert_eq!(paren("() => void"), "(() => void)");
        assert_eq!(paren("(x: number) => string"), "((x: number) => string)");
    }
// TSZ_INLINE_TEST_END a2bf19e99aa2c9ecb239a703d6dfd7853c55e4a50dc8ed4ea64fe366c49b5cef

// TSZ_INLINE_TEST_BEGIN da90e0952fac7fc04e830583c4cb4bde268f462cb94cbdaea7bd00066290adb6 1601 constructor_type_is_parenthesized
    #[test]
    fn constructor_type_is_parenthesized() {
        assert_eq!(paren("new () => Foo"), "(new () => Foo)");
    }
// TSZ_INLINE_TEST_END da90e0952fac7fc04e830583c4cb4bde268f462cb94cbdaea7bd00066290adb6

// TSZ_INLINE_TEST_BEGIN a3da08b7c971ceb06b71cc5f5ecc67b13a588f3df0012b8b385adac6cb31a995 1608 conditional_type_is_parenthesized
    #[test]
    fn conditional_type_is_parenthesized() {
        assert_eq!(
            paren("T extends string ? 1 : 0"),
            "(T extends string ? 1 : 0)"
        );
        // Renamed bound variable: proves the rule is not keyed on `T`.
        assert_eq!(
            paren("Elem extends number ? A : B"),
            "(Elem extends number ? A : B)"
        );
    }
// TSZ_INLINE_TEST_END a3da08b7c971ceb06b71cc5f5ecc67b13a588f3df0012b8b385adac6cb31a995

// TSZ_INLINE_TEST_BEGIN fceaa42c4b732e3f8bbabe411953b418d029d35ae8fef0fc6633cef9041d83a9 1621 keyof_type_is_parenthesized
    #[test]
    fn keyof_type_is_parenthesized() {
        assert_eq!(paren("keyof T"), "(keyof T)");
        assert_eq!(paren("keyof SomeOther"), "(keyof SomeOther)");
    }
// TSZ_INLINE_TEST_END fceaa42c4b732e3f8bbabe411953b418d029d35ae8fef0fc6633cef9041d83a9

// TSZ_INLINE_TEST_BEGIN 726ac304f3c2b2d67f58f0a8a0833f56c3ebdfe28609057a5852a5c24e4f8303 1627 infer_type_is_parenthesized
    #[test]
    fn infer_type_is_parenthesized() {
        assert_eq!(paren("infer E"), "(infer E)");
        assert_eq!(paren("infer Q9"), "(infer Q9)");
    }
// TSZ_INLINE_TEST_END 726ac304f3c2b2d67f58f0a8a0833f56c3ebdfe28609057a5852a5c24e4f8303

// TSZ_INLINE_TEST_BEGIN 6f6135945e8ba0eccc18220f7f73a80344c233813baa0add4e45e8862fc65fb4 1635 primary_types_stay_bare
    #[test]
    fn primary_types_stay_bare() {
        assert_eq!(paren("number"), "number");
        assert_eq!(paren("string"), "string");
        assert_eq!(paren("Box<number>"), "Box<number>");
        assert_eq!(paren("[number, string]"), "[number, string]");
        assert_eq!(paren("{ a: number }"), "{ a: number }");
    }
// TSZ_INLINE_TEST_END 6f6135945e8ba0eccc18220f7f73a80344c233813baa0add4e45e8862fc65fb4

// TSZ_INLINE_TEST_BEGIN 0c255b15ce651377f1bf643b9c4c4ec7f7f88b3db4378581c43a0353f2cd8e46 1644 already_parenthesized_text_is_not_double_wrapped
    #[test]
    fn already_parenthesized_text_is_not_double_wrapped() {
        assert_eq!(paren("(string | number)"), "(string | number)");
        assert_eq!(paren("(() => void)"), "(() => void)");
    }
// TSZ_INLINE_TEST_END 0c255b15ce651377f1bf643b9c4c4ec7f7f88b3db4378581c43a0353f2cd8e46

// TSZ_INLINE_TEST_BEGIN c1a225f1a98fc56dfc1c7e88c1b27c99071984397a4c956e1ba121719d9f12eb 1650 empty_text_is_passed_through
    #[test]
    fn empty_text_is_passed_through() {
        assert_eq!(paren(""), "");
        assert_eq!(paren("   "), "");
    }
// TSZ_INLINE_TEST_END c1a225f1a98fc56dfc1c7e88c1b27c99071984397a4c956e1ba121719d9f12eb

// TSZ_INLINE_TEST_BEGIN b68d91b6756f986842517021275b1553863de57e81b400ecb3b19c410aaadc9c 1665 rewrites_string_index_key_to_number_key
    #[test]
    fn rewrites_string_index_key_to_number_key() {
        assert_eq!(
            rewrite("    [x: string]: boolean;").as_deref(),
            Some("    [x: number]: boolean;")
        );
        assert_eq!(
            rewrite("\t[x: string]: Widget;").as_deref(),
            Some("\t[x: number]: Widget;")
        );
    }
// TSZ_INLINE_TEST_END b68d91b6756f986842517021275b1553863de57e81b400ecb3b19c410aaadc9c

// TSZ_INLINE_TEST_BEGIN 0e8560ab65dbcef06a8bc8ed1e7c41630b369d1c5078d90e3e64bd14ba97f838 1677 preserves_readonly_modifier_and_value_text
    #[test]
    fn preserves_readonly_modifier_and_value_text() {
        assert_eq!(
            rewrite("    readonly [x: string]: Foo | Bar;").as_deref(),
            Some("    readonly [x: number]: Foo | Bar;")
        );
    }
// TSZ_INLINE_TEST_END 0e8560ab65dbcef06a8bc8ed1e7c41630b369d1c5078d90e3e64bd14ba97f838

// TSZ_INLINE_TEST_BEGIN 288c49f81bc48a964b8923d1f6a5d958cab27b0ea911f1a2eb15ef588f58f225 1685 ignores_non_string_index_lines
    #[test]
    fn ignores_non_string_index_lines() {
        assert_eq!(rewrite("    [x: number]: boolean;"), None);
        assert_eq!(rewrite("    value: boolean;"), None);
    }
// TSZ_INLINE_TEST_END 288c49f81bc48a964b8923d1f6a5d958cab27b0ea911f1a2eb15ef588f58f225

// TSZ_INLINE_TEST_BEGIN f4f5720397c5773a22207c648cb17603311f8a02ea7eb3ab5f5479abb91cbc5d 1691 rewrites_index_signature_value_type_without_changing_key
    #[test]
    fn rewrites_index_signature_value_type_without_changing_key() {
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "    [x: string]: Beta | Alpha;",
                "Alpha | Beta",
            )
            .as_deref(),
            Some("    [x: string]: Alpha | Beta;")
        );
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "    readonly [x: number]: Second | First;",
                "First | Second",
            )
            .as_deref(),
            Some("    readonly [x: number]: First | Second;")
        );
    }
// TSZ_INLINE_TEST_END f4f5720397c5773a22207c648cb17603311f8a02ea7eb3ab5f5479abb91cbc5d

// TSZ_INLINE_TEST_BEGIN 18ffd1df99a9af29973d8ebe39f5d93972981ace1a3fe7bb3eb0a57d2becf05c 1711 preserves_index_signature_spacing_and_suffix
    #[test]
    fn preserves_index_signature_spacing_and_suffix() {
        assert_eq!(
            DeclarationEmitter::object_index_signature_line_with_value_type(
                "\t[x: symbol]:   Old | New;  ",
                "New | Old",
            )
            .as_deref(),
            Some("\t[x: symbol]:   New | Old;  ")
        );
    }
// TSZ_INLINE_TEST_END 18ffd1df99a9af29973d8ebe39f5d93972981ace1a3fe7bb3eb0a57d2becf05c
