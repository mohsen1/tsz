//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/symbols/name_text.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bb997f6814516eb824ae28289a3c9f8527823c62d9a83b89fed532fa4208776d 293 entity_name_returns_identifier_text_for_bare_identifier
    #[test]
    fn entity_name_returns_identifier_text_for_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            entity_name_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }
// TSZ_INLINE_TEST_END bb997f6814516eb824ae28289a3c9f8527823c62d9a83b89fed532fa4208776d

// TSZ_INLINE_TEST_BEGIN 0343e0f657ec2c1400de20f61fc3a3d91be815c8569501f23d8b45ba1f850800 302 entity_name_returns_dotted_text_for_qualified_name_in_type_position
    #[test]
    fn entity_name_returns_dotted_text_for_qualified_name_in_type_position() {
        // Trigger a QUALIFIED_NAME by parsing it inside a type alias rhs.
        let mut parser = ParserState::new("test.ts".to_string(), "type T = a.b.c;".to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        // Walk down to the type reference's name (a QualifiedName).
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let stmt_idx = source_file
            .statements
            .nodes
            .first()
            .copied()
            .expect("type alias statement");
        // Find the deepest QualifiedName under this statement.
        fn find_qualified_name(arena: &NodeArena, idx: NodeIndex) -> Option<NodeIndex> {
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                return Some(idx);
            }
            for child in arena.get_children(idx) {
                if let Some(found) = find_qualified_name(arena, child) {
                    return Some(found);
                }
            }
            None
        }
        let qn_idx = find_qualified_name(arena, stmt_idx).expect("qualified name in `a.b.c`");
        assert_eq!(
            entity_name_text_in_arena(arena, qn_idx),
            Some("a.b.c".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 0343e0f657ec2c1400de20f61fc3a3d91be815c8569501f23d8b45ba1f850800

// TSZ_INLINE_TEST_BEGIN 1ba7f127ae797a08707bf08ceaee59cdd92e8bd81861ec2c9dd592839ad58b63 337 entity_name_returns_none_for_call_expression
    #[test]
    fn entity_name_returns_none_for_call_expression() {
        let (parser, idx) = parse_first_expression("foo();");
        assert_eq!(entity_name_text_in_arena(parser.get_arena(), idx), None);
    }
// TSZ_INLINE_TEST_END 1ba7f127ae797a08707bf08ceaee59cdd92e8bd81861ec2c9dd592839ad58b63

// TSZ_INLINE_TEST_BEGIN 09dc1cc22501bf602303504f994e1805aba315fb7a5bc17c8906361cbbef09ff 345 expression_name_handles_property_access
    #[test]
    fn expression_name_handles_property_access() {
        let (parser, idx) = parse_first_expression("a.b.c;");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("a.b.c".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 09dc1cc22501bf602303504f994e1805aba315fb7a5bc17c8906361cbbef09ff

// TSZ_INLINE_TEST_BEGIN 0fec83fe5e5ce6d0537cdae509526004b342d14f3bff9e683b9ab4ec333feb0f 354 expression_name_unwraps_parentheses
    #[test]
    fn expression_name_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(foo);");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 0fec83fe5e5ce6d0537cdae509526004b342d14f3bff9e683b9ab4ec333feb0f

// TSZ_INLINE_TEST_BEGIN 4d46b3a87a291a3357f86e88242630a0fcf43e12b1b7890f57067c3300c0448c 363 expression_name_unwraps_parentheses_around_property_access
    #[test]
    fn expression_name_unwraps_parentheses_around_property_access() {
        let (parser, idx) = parse_first_expression("(a.b);");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("a.b".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 4d46b3a87a291a3357f86e88242630a0fcf43e12b1b7890f57067c3300c0448c

// TSZ_INLINE_TEST_BEGIN da5d19bddaf3edb251afa3dd0d978f37dd36c1f47a5b3bf7853c2847a7f4a0d2 372 expression_name_returns_none_for_call_expression
    #[test]
    fn expression_name_returns_none_for_call_expression() {
        let (parser, idx) = parse_first_expression("foo();");
        assert_eq!(expression_name_text_in_arena(parser.get_arena(), idx), None,);
    }
// TSZ_INLINE_TEST_END da5d19bddaf3edb251afa3dd0d978f37dd36c1f47a5b3bf7853c2847a7f4a0d2

// TSZ_INLINE_TEST_BEGIN f78c9de9de0c67e9163c426da62371a8d8780e18407ded206f83bf462f468a88 380 chain_text_handles_bare_identifier
    #[test]
    fn chain_text_handles_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }
// TSZ_INLINE_TEST_END f78c9de9de0c67e9163c426da62371a8d8780e18407ded206f83bf462f468a88

// TSZ_INLINE_TEST_BEGIN 4ab6cc25b867451f970a6f9f7511038af8a95cd1463bbef396e8c6983aba1a0d 389 chain_text_handles_property_access_chain
    #[test]
    fn chain_text_handles_property_access_chain() {
        let (parser, idx) = parse_first_expression("a.b.c;");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            Some("a.b.c".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 4ab6cc25b867451f970a6f9f7511038af8a95cd1463bbef396e8c6983aba1a0d

// TSZ_INLINE_TEST_BEGIN 9b0c8f85903a3d1330f9bef0cb5bdfd6feee19eb38ef6d4e2b8004e21a9e7331 398 chain_text_returns_none_for_parenthesized
    #[test]
    fn chain_text_returns_none_for_parenthesized() {
        // Unlike `expression_name_text_in_arena`, the chain helper does NOT
        // recurse through parentheses.
        let (parser, idx) = parse_first_expression("(foo);");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            None,
        );
    }
// TSZ_INLINE_TEST_END 9b0c8f85903a3d1330f9bef0cb5bdfd6feee19eb38ef6d4e2b8004e21a9e7331

// TSZ_INLINE_TEST_BEGIN 1a231e258c0869a907b551530f051a83d798b8ac40a74ba42fb17b484e7348f8 411 simple_computed_name_handles_identifier
    #[test]
    fn simple_computed_name_handles_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 1a231e258c0869a907b551530f051a83d798b8ac40a74ba42fb17b484e7348f8

// TSZ_INLINE_TEST_BEGIN 09b50cdbc6d535d02377064516fdd80f834de63bb8ed9fef33a7021ce03fee8d 420 simple_computed_name_handles_zero_arg_call
    #[test]
    fn simple_computed_name_handles_zero_arg_call() {
        let (parser, idx) = parse_first_expression("Symbol.iterator();");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("Symbol.iterator()".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 09b50cdbc6d535d02377064516fdd80f834de63bb8ed9fef33a7021ce03fee8d

// TSZ_INLINE_TEST_BEGIN 44577bf74e1554c2d193859d041248376f4e1ee1572456ec7ea70779338e4551 429 simple_computed_name_rejects_call_with_args
    #[test]
    fn simple_computed_name_rejects_call_with_args() {
        let (parser, idx) = parse_first_expression("foo(1);");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            None,
        );
    }
// TSZ_INLINE_TEST_END 44577bf74e1554c2d193859d041248376f4e1ee1572456ec7ea70779338e4551

// TSZ_INLINE_TEST_BEGIN 558db1773eba84abc324aab7c70a99f1e9209b962921615879f2b2b3da2dda46 438 simple_computed_name_unwraps_parentheses
    #[test]
    fn simple_computed_name_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(a.b);");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("a.b".to_string()),
        );
    }
// TSZ_INLINE_TEST_END 558db1773eba84abc324aab7c70a99f1e9209b962921615879f2b2b3da2dda46

// TSZ_INLINE_TEST_BEGIN bd9693965c3223a6f258eb61e084f09ff27a3fee4581144bf45a87071ce209a0 449 is_zero_arg_call_like_true_for_zero_arg_call
    #[test]
    fn is_zero_arg_call_like_true_for_zero_arg_call() {
        let (parser, idx) = parse_first_expression("Symbol.iterator();");
        assert!(is_zero_arg_call_like_expr_in_arena(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END bd9693965c3223a6f258eb61e084f09ff27a3fee4581144bf45a87071ce209a0

// TSZ_INLINE_TEST_BEGIN b9be5cc93d075a0a7dd531095f243079566928ced74a874a5e1264611fb9ee67 455 is_zero_arg_call_like_false_for_call_with_args
    #[test]
    fn is_zero_arg_call_like_false_for_call_with_args() {
        let (parser, idx) = parse_first_expression("foo(1);");
        assert!(!is_zero_arg_call_like_expr_in_arena(
            parser.get_arena(),
            idx
        ));
    }
// TSZ_INLINE_TEST_END b9be5cc93d075a0a7dd531095f243079566928ced74a874a5e1264611fb9ee67

// TSZ_INLINE_TEST_BEGIN 1b85044357391ea143a6d407378240148ecd4b9ac1fbdc413f3d061a5aecf226 464 is_zero_arg_call_like_false_for_bare_identifier
    #[test]
    fn is_zero_arg_call_like_false_for_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert!(!is_zero_arg_call_like_expr_in_arena(
            parser.get_arena(),
            idx
        ));
    }
// TSZ_INLINE_TEST_END 1b85044357391ea143a6d407378240148ecd4b9ac1fbdc413f3d061a5aecf226

// TSZ_INLINE_TEST_BEGIN 9efc4dd4c544c6850015f3231369470022ab3254374d667d77ed056499f97356 473 is_zero_arg_call_like_unwraps_parentheses
    #[test]
    fn is_zero_arg_call_like_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(foo());");
        assert!(is_zero_arg_call_like_expr_in_arena(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END 9efc4dd4c544c6850015f3231369470022ab3254374d667d77ed056499f97356

// TSZ_INLINE_TEST_BEGIN ccf4b6d66963fd175ac1465adf79491a0a60bd2985af561f99e6bb289b56745c 481 callee_name_matches_bare_identifier
    #[test]
    fn callee_name_matches_bare_identifier() {
        let (parser, idx) = parse_first_expression("arrayToEnum;");
        assert!(callee_is_array_to_enum_named(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END ccf4b6d66963fd175ac1465adf79491a0a60bd2985af561f99e6bb289b56745c

// TSZ_INLINE_TEST_BEGIN 5f11f972a8bd9a8cd0359c73f9705fb1222dcecf44d541f0c1fc4108916e7ef7 487 callee_name_matches_property_access_member
    #[test]
    fn callee_name_matches_property_access_member() {
        let (parser, idx) = parse_first_expression("util.arrayToEnum;");
        assert!(callee_is_array_to_enum_named(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END 5f11f972a8bd9a8cd0359c73f9705fb1222dcecf44d541f0c1fc4108916e7ef7

// TSZ_INLINE_TEST_BEGIN 5ab8862aaebd2c3f54b5e1fda79b89561be48c98f37c5c1a78111441dddac737 493 callee_name_matches_deep_property_access_member
    #[test]
    fn callee_name_matches_deep_property_access_member() {
        let (parser, idx) = parse_first_expression("a.b.arrayToEnum;");
        assert!(callee_is_array_to_enum_named(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END 5ab8862aaebd2c3f54b5e1fda79b89561be48c98f37c5c1a78111441dddac737

// TSZ_INLINE_TEST_BEGIN 0e467865ddf584f4c0dae15be7035892eb61a6148c7f3fc4b318e7ab2d652a14 499 callee_name_rejects_renamed_binder
    #[test]
    fn callee_name_rejects_renamed_binder() {
        // A user identifier that merely contains the substring must not match;
        // the name-key is an exact-segment check, not a substring test.
        let (parser, idx) = parse_first_expression("notArrayToEnumish;");
        assert!(!callee_is_array_to_enum_named(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END 0e467865ddf584f4c0dae15be7035892eb61a6148c7f3fc4b318e7ab2d652a14

// TSZ_INLINE_TEST_BEGIN 0a5a7295e2f1111cb269aba1bc1c4b76b931e4a0667db5ebccbbea5538293fa6 507 callee_name_rejects_property_access_with_other_member
    #[test]
    fn callee_name_rejects_property_access_with_other_member() {
        let (parser, idx) = parse_first_expression("arrayToEnum.other;");
        assert!(!callee_is_array_to_enum_named(parser.get_arena(), idx));
    }
// TSZ_INLINE_TEST_END 0a5a7295e2f1111cb269aba1bc1c4b76b931e4a0667db5ebccbbea5538293fa6

// TSZ_INLINE_TEST_BEGIN cbba8bfddab7bb45bd319a8e29a34c7b89cbe352fac8a5457d8beb0d79c79d22 513 literal_names_extracts_string_members_for_bare_callee
    #[test]
    fn literal_names_extracts_string_members_for_bare_callee() {
        let (parser, idx) = parse_first_expression(r#"arrayToEnum(["a", "b", "c"]);"#);
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
        );
    }
// TSZ_INLINE_TEST_END cbba8bfddab7bb45bd319a8e29a34c7b89cbe352fac8a5457d8beb0d79c79d22

// TSZ_INLINE_TEST_BEGIN c8bcb874be138225a2c3556711fa7d6a9eed4bbfd4bab8d1b106d6fa7906b472 522 literal_names_extracts_string_members_for_property_access_callee
    #[test]
    fn literal_names_extracts_string_members_for_property_access_callee() {
        let (parser, idx) = parse_first_expression(r#"util.arrayToEnum(["x", "y"]);"#);
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            Some(vec!["x".to_string(), "y".to_string()]),
        );
    }
// TSZ_INLINE_TEST_END c8bcb874be138225a2c3556711fa7d6a9eed4bbfd4bab8d1b106d6fa7906b472

// TSZ_INLINE_TEST_BEGIN 18c07b4f18d77e5892eb1e7c849e4d2501f89366b7a9aa3e290432f368135600 531 literal_names_skips_non_string_members
    #[test]
    fn literal_names_skips_non_string_members() {
        let (parser, idx) = parse_first_expression(r#"arrayToEnum(["a", 1, "b"]);"#);
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            Some(vec!["a".to_string(), "b".to_string()]),
        );
    }
// TSZ_INLINE_TEST_END 18c07b4f18d77e5892eb1e7c849e4d2501f89366b7a9aa3e290432f368135600

// TSZ_INLINE_TEST_BEGIN 966bcdccc18924883f3182be39ebdaca72823b3b333fe809f42f7424f0f828e1 540 literal_names_rejects_renamed_callee
    #[test]
    fn literal_names_rejects_renamed_callee() {
        // The same call shape under a different identifier must not be recovered;
        // this is the anti-hardcoding contract — the name-key lives in exactly
        // one place and any other identifier yields `None`.
        let (parser, idx) = parse_first_expression(r#"makeEnum(["a", "b"]);"#);
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            None
        );
    }
// TSZ_INLINE_TEST_END 966bcdccc18924883f3182be39ebdaca72823b3b333fe809f42f7424f0f828e1

// TSZ_INLINE_TEST_BEGIN 98931c6400c961c9fd1247a2af096ceb34328591030212a9b0927e9e19416196 552 literal_names_rejects_non_array_first_argument
    #[test]
    fn literal_names_rejects_non_array_first_argument() {
        let (parser, idx) = parse_first_expression(r#"arrayToEnum("a");"#);
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            None
        );
    }
// TSZ_INLINE_TEST_END 98931c6400c961c9fd1247a2af096ceb34328591030212a9b0927e9e19416196

// TSZ_INLINE_TEST_BEGIN 86d137e3c97df31edb48a952b26dd8a02a8207dd05ad53e159efe7eae2110caf 561 literal_names_returns_none_for_non_call
    #[test]
    fn literal_names_returns_none_for_non_call() {
        let (parser, idx) = parse_first_expression("arrayToEnum;");
        assert_eq!(
            array_to_enum_call_literal_names(parser.get_arena(), idx),
            None
        );
    }
// TSZ_INLINE_TEST_END 86d137e3c97df31edb48a952b26dd8a02a8207dd05ad53e159efe7eae2110caf
