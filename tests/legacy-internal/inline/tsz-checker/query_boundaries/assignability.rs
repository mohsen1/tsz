//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/assignability.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c7b1b6c73a8c4e82c312edd2fdaee97f03c253641bae035b75b3c356c446d0ed 1835 target_property_index_uses_first_atom_match
    #[test]
    fn target_property_index_uses_first_atom_match() {
        let db = TypeInterner::new();
        let name = db.intern_string("renamed");
        let mut index = TargetPropertyIndex::default();

        index.insert(&PropertyInfo::new(name, TypeId::STRING));
        index.insert(&PropertyInfo::new(name, TypeId::NUMBER));

        let source = PropertyInfo::new(name, TypeId::BOOLEAN);
        assert_eq!(index.matching_type_for(&db, &source), Some(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END c7b1b6c73a8c4e82c312edd2fdaee97f03c253641bae035b75b3c356c446d0ed

// TSZ_INLINE_TEST_BEGIN 26e70a1083e5fe6b51f24e528b1c869fcbce6415bf9afeff7fa3542b253d67ee 1848 target_property_index_keeps_string_fallback
    #[test]
    fn target_property_index_keeps_string_fallback() {
        let db = TypeInterner::new();
        let name = db.intern_string("fallbackName");
        let mut index = TargetPropertyIndex::default();

        index.fallback_order.push((name, TypeId::NUMBER));

        assert_eq!(
            index.matching_type_by_resolved_name(&db, name),
            Some(TypeId::NUMBER)
        );
    }
// TSZ_INLINE_TEST_END 26e70a1083e5fe6b51f24e528b1c869fcbce6415bf9afeff7fa3542b253d67ee

// TSZ_INLINE_TEST_BEGIN 52d79190205c08d758d2d0871c8664e8c902f99243d5fc32db04e1250009d6ed 1862 symbol_named_source_property_is_accepted_by_property_key_index_signature
    #[test]
    fn symbol_named_source_property_is_accepted_by_property_key_index_signature() {
        let db = TypeInterner::new();
        let property_key = db.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: property_key,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert!(classification.excess_properties.is_empty());
    }
// TSZ_INLINE_TEST_END 52d79190205c08d758d2d0871c8664e8c902f99243d5fc32db04e1250009d6ed

// TSZ_INLINE_TEST_BEGIN dc425d4df65304b923823865a4246f872149ec0c3073f31bee63aa8c7eac06d9 1886 symbol_named_source_property_is_excess_for_plain_string_index_signature
    #[test]
    fn symbol_named_source_property_is_excess_for_plain_string_index_signature() {
        let db = TypeInterner::new();
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert_eq!(classification.excess_properties.len(), 1);
    }
// TSZ_INLINE_TEST_END dc425d4df65304b923823865a4246f872149ec0c3073f31bee63aa8c7eac06d9

// TSZ_INLINE_TEST_BEGIN 9c5047f2d88525cf7d5d360a031a41926c7149f43a8ecbe1b1e3d901730b1208 1909 optional_mapped_implicit_undefined_is_structural_across_param_names
    #[test]
    fn optional_mapped_implicit_undefined_is_structural_across_param_names() {
        let db = TypeInterner::new();

        for name in ["K", "Prop"] {
            let mapped = db.mapped(MappedType {
                type_param: TypeParamInfo::simple(db.intern_string(name)),
                constraint: TypeId::STRING,
                template: TypeId::NUMBER,
                name_type: None,
                readonly_modifier: None,
                optional_modifier: Some(MappedModifier::Add),
            });

            assert!(optional_mapped_type_adds_implicit_undefined(
                &db, &db, mapped
            ));
        }
    }
// TSZ_INLINE_TEST_END 9c5047f2d88525cf7d5d360a031a41926c7149f43a8ecbe1b1e3d901730b1208

// TSZ_INLINE_TEST_BEGIN a0a8410d92c970c179390eb4b185284e871fb395eb1f3f3d912e8683ed18dbba 1929 optional_mapped_implicit_undefined_rejects_existing_undefined_template
    #[test]
    fn optional_mapped_implicit_undefined_rejects_existing_undefined_template() {
        let db = TypeInterner::new();
        let template = db.union2(TypeId::NUMBER, TypeId::UNDEFINED);
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }
// TSZ_INLINE_TEST_END a0a8410d92c970c179390eb4b185284e871fb395eb1f3f3d912e8683ed18dbba

// TSZ_INLINE_TEST_BEGIN 894b8457b248d4a30bddd10e7b4aa16ac4de415f3dbc6f49d7e767a21d3ddbb0 1947 optional_mapped_implicit_undefined_respects_display_alias_surface
    #[test]
    fn optional_mapped_implicit_undefined_respects_display_alias_surface() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });
        let alias = db.application(db.lazy(DefId(1)), vec![TypeId::STRING]);
        db.store_display_alias(mapped, alias);

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }
// TSZ_INLINE_TEST_END 894b8457b248d4a30bddd10e7b4aa16ac4de415f3dbc6f49d7e767a21d3ddbb0
