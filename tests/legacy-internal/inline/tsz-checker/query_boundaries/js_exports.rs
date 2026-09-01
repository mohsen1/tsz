//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/js_exports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 65eed09d91f4bfd741fcd2147b3651221ebec9cd761c0491a1658d81422d9fd4 1900 normalize_property_declaration_order_preserves_existing_source_order
    #[test]
    fn normalize_property_declaration_order_preserves_existing_source_order() {
        let db = TypeInterner::new();
        let mut props = vec![prop(&db, "configs", 3), prop(&db, "default", 1)];

        JsExportSurface::normalize_property_declaration_order(&mut props);

        assert_eq!(db.resolve_atom_ref(props[0].name).as_ref(), "default");
        assert_eq!(props[0].declaration_order, 1);
        assert_eq!(db.resolve_atom_ref(props[1].name).as_ref(), "configs");
        assert_eq!(props[1].declaration_order, 2);
    }
// TSZ_INLINE_TEST_END 65eed09d91f4bfd741fcd2147b3651221ebec9cd761c0491a1658d81422d9fd4

// TSZ_INLINE_TEST_BEGIN 5ddf789d6c203e94ad4535117963e3934c761a4086ab7e706ae50fa276e6012e 1913 normalize_property_declaration_order_prioritizes_explicit_members_before_unset_members
    #[test]
    fn normalize_property_declaration_order_prioritizes_explicit_members_before_unset_members() {
        let db = TypeInterner::new();
        let mut props = vec![prop(&db, "configs", 0), prop(&db, "default", 1)];

        JsExportSurface::normalize_property_declaration_order(&mut props);

        assert_eq!(db.resolve_atom_ref(props[0].name).as_ref(), "default");
        assert_eq!(props[0].declaration_order, 1);
        assert_eq!(db.resolve_atom_ref(props[1].name).as_ref(), "configs");
        assert_eq!(props[1].declaration_order, 2);
    }
// TSZ_INLINE_TEST_END 5ddf789d6c203e94ad4535117963e3934c761a4086ab7e706ae50fa276e6012e

// TSZ_INLINE_TEST_BEGIN b6f9d0fb54b8b01f89e04d56a06dac78d45b760973a19f99d964efb316ec7573 1926 constructs_commonjs_json_export_surfaces
    #[test]
    fn constructs_commonjs_json_export_surfaces() {
        let db = TypeInterner::new();
        let present = json_module_object_property(&db, "present", TypeId::STRING, 1);
        assert_eq!(db.resolve_atom_ref(present.name).as_ref(), "present");
        assert_eq!(present.type_id, TypeId::STRING);
        assert!(!present.optional);
        assert_eq!(present.declaration_order, 1);

        let missing = json_module_missing_property(&db, "missing", 2);
        assert_eq!(db.resolve_atom_ref(missing.name).as_ref(), "missing");
        assert_eq!(missing.type_id, TypeId::UNDEFINED);
        assert_eq!(missing.write_type, TypeId::UNDEFINED);
        assert!(missing.optional);

        let object = json_module_object_type(&db, vec![present.clone(), missing]);
        assert_eq!(
            object,
            db.object(vec![
                present,
                json_module_missing_property(&db, "missing", 2)
            ])
        );
        assert_eq!(
            json_module_union(&db, vec![TypeId::STRING, TypeId::NUMBER]),
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        );
        assert_eq!(
            json_module_array_type(&db, TypeId::STRING),
            db.array(TypeId::STRING)
        );

        let esm_namespace = json_esm_namespace_type(&db, TypeId::BOOLEAN);
        let esm_shape = tsz_solver::type_queries::get_object_shape(&db, esm_namespace)
            .expect("ESM JSON namespace should be an object");
        assert_eq!(esm_shape.properties.len(), 1);
        let default_prop = &esm_shape.properties[0];
        assert_eq!(db.resolve_atom_ref(default_prop.name).as_ref(), "default");
        assert_eq!(default_prop.type_id, TypeId::BOOLEAN);
        assert!(!default_prop.optional);

        assert_eq!(commonjs_json_namespace_type(&db, object), object);

        let object_with_default = json_module_object_type(
            &db,
            vec![json_module_object_property(
                &db,
                "default",
                TypeId::STRING,
                1,
            )],
        );
        let cjs_namespace = commonjs_json_namespace_type(&db, object_with_default);
        let cjs_shape = tsz_solver::type_queries::get_object_shape(&db, cjs_namespace)
            .expect("CJS JSON namespace should be an object");
        assert_eq!(cjs_shape.properties.len(), 1);
        assert_eq!(cjs_shape.properties[0].type_id, object_with_default);
        assert_eq!(cjs_shape.properties[0].write_type, object_with_default);
        assert!(!cjs_shape.properties[0].optional);
        assert!(!cjs_shape.properties[0].readonly);

        let late = commonjs_namespace_any_property(&db, "late", 3);
        assert_eq!(db.resolve_atom_ref(late.name).as_ref(), "late");
        assert_eq!(late.type_id, TypeId::ANY);
        assert_eq!(late.write_type, TypeId::ANY);
        assert!(!late.optional);
        assert_eq!(commonjs_empty_namespace_type(&db), db.object(Vec::new()));
    }
// TSZ_INLINE_TEST_END b6f9d0fb54b8b01f89e04d56a06dac78d45b760973a19f99d964efb316ec7573
