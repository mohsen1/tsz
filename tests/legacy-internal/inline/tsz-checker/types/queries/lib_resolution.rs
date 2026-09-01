//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/queries/lib_resolution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1b1817a2607cac58ea47fa24a9d89fc45a50383f88320de6e9b80509efde9bcb 1533 keyword_syntax_maps_string
    #[test]
    fn keyword_syntax_maps_string() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::StringKeyword as u16),
            Some(TypeId::STRING)
        );
    }
// TSZ_INLINE_TEST_END 1b1817a2607cac58ea47fa24a9d89fc45a50383f88320de6e9b80509efde9bcb

// TSZ_INLINE_TEST_BEGIN 0dfa31c3066f2d7d9adfcaac8e7e434cbc64af17b9c86ebd0c68faf980294f22 1541 keyword_syntax_maps_number
    #[test]
    fn keyword_syntax_maps_number() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NumberKeyword as u16),
            Some(TypeId::NUMBER)
        );
    }
// TSZ_INLINE_TEST_END 0dfa31c3066f2d7d9adfcaac8e7e434cbc64af17b9c86ebd0c68faf980294f22

// TSZ_INLINE_TEST_BEGIN bdaccb4936fe2bb2d4aa1639715ba5146f6afb053566bd926b6cf0e86f47e31c 1549 keyword_syntax_maps_boolean
    #[test]
    fn keyword_syntax_maps_boolean() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::BooleanKeyword as u16),
            Some(TypeId::BOOLEAN)
        );
    }
// TSZ_INLINE_TEST_END bdaccb4936fe2bb2d4aa1639715ba5146f6afb053566bd926b6cf0e86f47e31c

// TSZ_INLINE_TEST_BEGIN bb0b47351fe29dc1a03593c9c36f0924ccde70adad112621f04aabcc784c3a78 1557 keyword_syntax_maps_void
    #[test]
    fn keyword_syntax_maps_void() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::VoidKeyword as u16),
            Some(TypeId::VOID)
        );
    }
// TSZ_INLINE_TEST_END bb0b47351fe29dc1a03593c9c36f0924ccde70adad112621f04aabcc784c3a78

// TSZ_INLINE_TEST_BEGIN 5e0c8143e5fea86ce190357f87f0f6007c49f706bafedcac0274f0735c700cd0 1565 keyword_syntax_maps_never
    #[test]
    fn keyword_syntax_maps_never() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NeverKeyword as u16),
            Some(TypeId::NEVER)
        );
    }
// TSZ_INLINE_TEST_END 5e0c8143e5fea86ce190357f87f0f6007c49f706bafedcac0274f0735c700cd0

// TSZ_INLINE_TEST_BEGIN e29ac5c9226e62ef3c5beee0a0f9907d5661ea0aa0075c22722f4944d3bf6c87 1573 keyword_syntax_maps_any
    #[test]
    fn keyword_syntax_maps_any() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::AnyKeyword as u16),
            Some(TypeId::ANY)
        );
    }
// TSZ_INLINE_TEST_END e29ac5c9226e62ef3c5beee0a0f9907d5661ea0aa0075c22722f4944d3bf6c87

// TSZ_INLINE_TEST_BEGIN 32681cb8b9cadd346124acd023486a85777afdc946967520bfbfed98552f488d 1581 keyword_syntax_maps_unknown
    #[test]
    fn keyword_syntax_maps_unknown() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::UnknownKeyword as u16),
            Some(TypeId::UNKNOWN)
        );
    }
// TSZ_INLINE_TEST_END 32681cb8b9cadd346124acd023486a85777afdc946967520bfbfed98552f488d

// TSZ_INLINE_TEST_BEGIN 2190a999fff12305b6c3fe95dfc1c0691c08244feebef7efa55958e75f448dc5 1589 keyword_syntax_maps_null
    #[test]
    fn keyword_syntax_maps_null() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::NullKeyword as u16),
            Some(TypeId::NULL)
        );
    }
// TSZ_INLINE_TEST_END 2190a999fff12305b6c3fe95dfc1c0691c08244feebef7efa55958e75f448dc5

// TSZ_INLINE_TEST_BEGIN e7e0204b98c7214bff44b0542d5ebd13af99f92695112a5845b7d806682cffc0 1597 keyword_syntax_maps_undefined
    #[test]
    fn keyword_syntax_maps_undefined() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::UndefinedKeyword as u16),
            Some(TypeId::UNDEFINED)
        );
    }
// TSZ_INLINE_TEST_END e7e0204b98c7214bff44b0542d5ebd13af99f92695112a5845b7d806682cffc0

// TSZ_INLINE_TEST_BEGIN fac61c0a6c383ee3d14ca6e5f8f1eceba1bb3bb1211590453dc2521c906db429 1605 keyword_syntax_maps_object
    #[test]
    fn keyword_syntax_maps_object() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::ObjectKeyword as u16),
            Some(TypeId::OBJECT)
        );
    }
// TSZ_INLINE_TEST_END fac61c0a6c383ee3d14ca6e5f8f1eceba1bb3bb1211590453dc2521c906db429

// TSZ_INLINE_TEST_BEGIN 7e210b400bfc6b56c912e415411427334ec65e0bd7dcd7e94f91e424ca624ab9 1613 keyword_syntax_maps_symbol
    #[test]
    fn keyword_syntax_maps_symbol() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::SymbolKeyword as u16),
            Some(TypeId::SYMBOL)
        );
    }
// TSZ_INLINE_TEST_END 7e210b400bfc6b56c912e415411427334ec65e0bd7dcd7e94f91e424ca624ab9

// TSZ_INLINE_TEST_BEGIN e8f976816cc64d241815e4482bb62b500cad0b3e438d6f9fa87f9b9bfa04fb57 1621 keyword_syntax_maps_bigint
    #[test]
    fn keyword_syntax_maps_bigint() {
        assert_eq!(
            keyword_syntax_to_type_id(SyntaxKind::BigIntKeyword as u16),
            Some(TypeId::BIGINT)
        );
    }
// TSZ_INLINE_TEST_END e8f976816cc64d241815e4482bb62b500cad0b3e438d6f9fa87f9b9bfa04fb57

// TSZ_INLINE_TEST_BEGIN 816fcbcc97edcd91ba04c1b05f6ef29941483ce8381eba497b3c882c4a054f8a 1629 keyword_syntax_returns_none_for_non_keyword
    #[test]
    fn keyword_syntax_returns_none_for_non_keyword() {
        assert_eq!(keyword_syntax_to_type_id(0), None);
        assert_eq!(keyword_syntax_to_type_id(9999), None);
    }
// TSZ_INLINE_TEST_END 816fcbcc97edcd91ba04c1b05f6ef29941483ce8381eba497b3c882c4a054f8a

// TSZ_INLINE_TEST_BEGIN f2d0a5fac3800b84a3d521a23d6be1999882f4e3cc931e631c93c254b4900d24 1635 keyword_name_maps_all_primitives
    #[test]
    fn keyword_name_maps_all_primitives() {
        assert_eq!(keyword_name_to_type_id("string"), Some(TypeId::STRING));
        assert_eq!(keyword_name_to_type_id("number"), Some(TypeId::NUMBER));
        assert_eq!(keyword_name_to_type_id("boolean"), Some(TypeId::BOOLEAN));
        assert_eq!(keyword_name_to_type_id("void"), Some(TypeId::VOID));
        assert_eq!(
            keyword_name_to_type_id("undefined"),
            Some(TypeId::UNDEFINED)
        );
        assert_eq!(keyword_name_to_type_id("null"), Some(TypeId::NULL));
        assert_eq!(keyword_name_to_type_id("never"), Some(TypeId::NEVER));
        assert_eq!(keyword_name_to_type_id("unknown"), Some(TypeId::UNKNOWN));
        assert_eq!(keyword_name_to_type_id("any"), Some(TypeId::ANY));
        assert_eq!(keyword_name_to_type_id("object"), Some(TypeId::OBJECT));
        assert_eq!(keyword_name_to_type_id("symbol"), Some(TypeId::SYMBOL));
        assert_eq!(keyword_name_to_type_id("bigint"), Some(TypeId::BIGINT));
    }
// TSZ_INLINE_TEST_END f2d0a5fac3800b84a3d521a23d6be1999882f4e3cc931e631c93c254b4900d24

// TSZ_INLINE_TEST_BEGIN e5adf53a8fcfbadceb0c366c7812d130ca5bde3fa4105b7d4884df69eab5fd12 1654 keyword_name_returns_none_for_non_keyword
    #[test]
    fn keyword_name_returns_none_for_non_keyword() {
        assert_eq!(keyword_name_to_type_id("Promise"), None);
        assert_eq!(keyword_name_to_type_id("Array"), None);
        assert_eq!(keyword_name_to_type_id("String"), None);
        assert_eq!(keyword_name_to_type_id(""), None);
    }
// TSZ_INLINE_TEST_END e5adf53a8fcfbadceb0c366c7812d130ca5bde3fa4105b7d4884df69eab5fd12

// TSZ_INLINE_TEST_BEGIN 8cb31a3f2668b6ea7d48e0dbb352c7542ec85d19a024a1d7b58493d1184fe897 1662 keyword_name_and_syntax_agree
    #[test]
    fn keyword_name_and_syntax_agree() {
        let pairs = [
            ("string", SyntaxKind::StringKeyword),
            ("number", SyntaxKind::NumberKeyword),
            ("boolean", SyntaxKind::BooleanKeyword),
            ("void", SyntaxKind::VoidKeyword),
            ("undefined", SyntaxKind::UndefinedKeyword),
            ("null", SyntaxKind::NullKeyword),
            ("never", SyntaxKind::NeverKeyword),
            ("unknown", SyntaxKind::UnknownKeyword),
            ("any", SyntaxKind::AnyKeyword),
            ("object", SyntaxKind::ObjectKeyword),
            ("symbol", SyntaxKind::SymbolKeyword),
            ("bigint", SyntaxKind::BigIntKeyword),
        ];
        for (name, kind) in pairs {
            assert_eq!(
                keyword_name_to_type_id(name),
                keyword_syntax_to_type_id(kind as u16),
                "Mismatch for keyword '{name}'"
            );
        }
    }
// TSZ_INLINE_TEST_END 8cb31a3f2668b6ea7d48e0dbb352c7542ec85d19a024a1d7b58493d1184fe897

// TSZ_INLINE_TEST_BEGIN b6260c3680f3909520324584cc922bb245f2bce591080cc6cd4183f515f23992 1687 dedup_empty
    #[test]
    fn dedup_empty() {
        let result = dedup_decl_arenas(&[]);
        assert!(result.is_empty());
    }
// TSZ_INLINE_TEST_END b6260c3680f3909520324584cc922bb245f2bce591080cc6cd4183f515f23992

// TSZ_INLINE_TEST_BEGIN f50fd60412bd93de32369b22e46d9758369d99e2316e5b7075609e0c61fc39f8 1693 dedup_single
    #[test]
    fn dedup_single() {
        let arena = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(result.len(), 1);
    }
// TSZ_INLINE_TEST_END f50fd60412bd93de32369b22e46d9758369d99e2316e5b7075609e0c61fc39f8

// TSZ_INLINE_TEST_BEGIN fb60fb4575567267a2f953cf4ebc98b25675db20b182fe35310d4438b278e62e 1702 dedup_same_arena_same_index
    #[test]
    fn dedup_same_arena_same_index() {
        let arena = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena), (idx, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            1,
            "Duplicate (same arena, same index) should be removed"
        );
    }
// TSZ_INLINE_TEST_END fb60fb4575567267a2f953cf4ebc98b25675db20b182fe35310d4438b278e62e

// TSZ_INLINE_TEST_BEGIN 8cc328cf8f7ca2b559b5015faaa2cfde5dd67d4ded29f42881de4ab22d71a58f 1715 dedup_different_arenas_same_index
    #[test]
    fn dedup_different_arenas_same_index() {
        let arena1 = NodeArena::default();
        let arena2 = NodeArena::default();
        let idx = NodeIndex(0);
        let input = [(idx, &arena1), (idx, &arena2)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            2,
            "Same index from different arenas should be kept"
        );
    }
// TSZ_INLINE_TEST_END 8cc328cf8f7ca2b559b5015faaa2cfde5dd67d4ded29f42881de4ab22d71a58f

// TSZ_INLINE_TEST_BEGIN f08453129a5a21a2fc62d5dea6c401ce8202b9c4d67524438620c7d3729feaa4 1729 dedup_same_arena_different_indices
    #[test]
    fn dedup_same_arena_different_indices() {
        let arena = NodeArena::default();
        let idx0 = NodeIndex(0);
        let idx1 = NodeIndex(1);
        let input = [(idx0, &arena), (idx1, &arena)];
        let result = dedup_decl_arenas(&input);
        assert_eq!(
            result.len(),
            2,
            "Different indices from same arena should be kept"
        );
    }
// TSZ_INLINE_TEST_END f08453129a5a21a2fc62d5dea6c401ce8202b9c4d67524438620c7d3729feaa4

// TSZ_INLINE_TEST_BEGIN 46d1e0367e7b87789e0e5cfd3245287f5794efe25f676ce5730abf8c63071ba4 1745 no_value_resolver_always_returns_none
    #[test]
    fn no_value_resolver_always_returns_none() {
        assert_eq!(super::no_value_resolver(NodeIndex(0)), None);
        assert_eq!(super::no_value_resolver(NodeIndex(42)), None);
        assert_eq!(super::no_value_resolver(NodeIndex(u32::MAX)), None);
    }
// TSZ_INLINE_TEST_END 46d1e0367e7b87789e0e5cfd3245287f5794efe25f676ce5730abf8c63071ba4

// TSZ_INLINE_TEST_BEGIN e3294ceb32d1c5c19efa52bd5d2dd604e35059ac5fb6cd30daa98466adc62661 1752 shared_array_name_resolution_reuses_registered_base_type
    #[test]
    fn shared_array_name_resolution_reuses_registered_base_type() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let array_base = types.factory().object(Vec::new());
        types.set_array_base_type(
            array_base,
            vec![TypeParamInfo {
                name: types.intern_string("T"),
                constraint: None,
                default: None,
                is_const: false,
                origin: tsz_solver::TypeParamOrigin::User,
            }],
        );

        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.share_owner_symbol_type_results = true;

        assert_eq!(checker.resolve_lib_type_by_name("Array"), Some(array_base));
    }
// TSZ_INLINE_TEST_END e3294ceb32d1c5c19efa52bd5d2dd604e35059ac5fb6cd30daa98466adc62661

// TSZ_INLINE_TEST_BEGIN d32db34cbeeb3638d9866612df21c3dba8c7771aee1d4df09e61965af7cad418 1781 known_global_constructor_cache_rejects_non_constructable_type
    #[test]
    fn known_global_constructor_cache_rejects_non_constructable_type() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        let non_constructable = types.factory().object(Vec::new());

        assert!(
            !checker.cached_lib_type_is_usable("ErrorConstructor", Some(non_constructable)),
            "known global constructor cache entries must actually be constructable"
        );
        assert!(
            checker.cached_lib_type_is_usable("Error", Some(non_constructable)),
            "non-constructor lib cache entries are not filtered by constructability"
        );
        assert!(
            !checker.cached_lib_type_is_usable("Error", Some(TypeId(10_000))),
            "cached non-intrinsic TypeIds must belong to the current interner"
        );
    }
// TSZ_INLINE_TEST_END d32db34cbeeb3638d9866612df21c3dba8c7771aee1d4df09e61965af7cad418
