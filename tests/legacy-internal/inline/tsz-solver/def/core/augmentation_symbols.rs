//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/def/core/augmentation_symbols.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bf014f448baf2235ccb4bdfec7df4a0bd5dd14159399dacf1fcae50e0f32e629 173 module_augmentation_symbol_def_registers_missing_edge
    #[test]
    fn module_augmentation_symbol_def_registers_missing_edge() {
        let store = DefinitionStore::new();
        let def_id = DefId(42);

        store.register_module_augmentation_symbol_def(100, def_id);

        assert_eq!(store.find_def_by_symbol(100), Some(def_id));
    }
// TSZ_INLINE_TEST_END bf014f448baf2235ccb4bdfec7df4a0bd5dd14159399dacf1fcae50e0f32e629

// TSZ_INLINE_TEST_BEGIN f9abc47092799c06a6658776b92803379611e33171a1d33dfc11368b23e2057f 183 module_augmentation_symbol_def_keeps_first_edge
    #[test]
    fn module_augmentation_symbol_def_keeps_first_edge() {
        let store = DefinitionStore::new();
        let first = DefId(42);
        let second = DefId(43);

        store.register_module_augmentation_symbol_def(100, first);
        store.register_module_augmentation_symbol_def(100, second);

        assert_eq!(store.find_def_by_symbol(100), Some(first));
    }
// TSZ_INLINE_TEST_END f9abc47092799c06a6658776b92803379611e33171a1d33dfc11368b23e2057f

// TSZ_INLINE_TEST_BEGIN 061c33b90d31a14fe806634725876ac9ed8f7a815c5141a37baa71adbae06b24 195 module_augmented_body_redirects_empty_plain_object
    #[test]
    fn module_augmented_body_redirects_empty_plain_object() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let empty = types.object(Vec::new());
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            Some(augmented)
        );
    }
// TSZ_INLINE_TEST_END 061c33b90d31a14fe806634725876ac9ed8f7a815c5141a37baa71adbae06b24

// TSZ_INLINE_TEST_BEGIN 0eff8f283aa43e740577abb2c4c579a9d56c8ed8ef29cde42e77c4958683fc2b 214 module_augmented_body_public_lookup_is_raw_when_publication_flag_off
    #[test]
    fn module_augmented_body_public_lookup_is_raw_when_publication_flag_off() {
        let store = DefinitionStore::new();
        if store.module_augmented_body_publication_enabled() {
            return;
        }
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let empty = types.object(Vec::new());
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[]));

        assert_eq!(store.module_augmented_body_for(def_id, empty, &types), None);
        assert_eq!(
            store.module_augmented_body_or_current(def_id, empty, &types),
            empty
        );
    }
// TSZ_INLINE_TEST_END 0eff8f283aa43e740577abb2c4c579a9d56c8ed8ef29cde42e77c4958683fc2b

// TSZ_INLINE_TEST_BEGIN 4f3347c328109549fc989b212482f159e9096c8d9c1c240cb0d46656798ce554 237 module_augmented_body_keeps_non_empty_current_body
    #[test]
    fn module_augmented_body_keeps_non_empty_current_body() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let current = types.object(vec![PropertyInfo::new(
            types.intern_string("current"),
            TypeId::NUMBER,
        )]);
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, current, &types),
            None
        );
    }
// TSZ_INLINE_TEST_END 4f3347c328109549fc989b212482f159e9096c8d9c1c240cb0d46656798ce554

// TSZ_INLINE_TEST_BEGIN 1f6031b8825c06bb4e9837bec0c4e81f3114ade2abe77cd0d94ca37f7524dd96 259 module_augmented_body_keeps_first_publication
    #[test]
    fn module_augmented_body_keeps_first_publication() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let first = types.object(vec![PropertyInfo::new(
            types.intern_string("first"),
            TypeId::STRING,
        )]);
        let second = types.object(vec![PropertyInfo::new(
            types.intern_string("second"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, first, &[]));
        assert!(!store.register_module_augmented_body(def_id, second, &[]));

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, types.object(Vec::new()), &types),
            Some(first)
        );
    }
// TSZ_INLINE_TEST_END 1f6031b8825c06bb4e9837bec0c4e81f3114ade2abe77cd0d94ca37f7524dd96

// TSZ_INLINE_TEST_BEGIN 8d7e66a88aafeed872b8313d45907ea6c1012e19ff4b8976e1f238a241e283ab 282 module_augmented_body_invalidates_when_augmentation_file_changes
    #[test]
    fn module_augmented_body_invalidates_when_augmentation_file_changes() {
        let store = DefinitionStore::new();
        let types = TypeInterner::new();
        let def_id = DefId(42);
        let empty = types.object(Vec::new());
        let augmented = types.object(vec![PropertyInfo::new(
            types.intern_string("member"),
            TypeId::STRING,
        )]);

        assert!(store.register_module_augmented_body(def_id, augmented, &[7]));
        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            Some(augmented)
        );

        assert_eq!(store.invalidate_file(7), 0);

        assert_eq!(
            store.module_augmented_body_for_registered(def_id, empty, &types),
            None
        );
    }
// TSZ_INLINE_TEST_END 8d7e66a88aafeed872b8313d45907ea6c1012e19ff4b8976e1f238a241e283ab
