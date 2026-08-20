//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/diagnostics/display_provenance.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c2596e84014db8a6ab727c991105003e83c30fb3d599868111df467c2b840c3f 74 alias_application_records_display_alias
    #[test]
    fn alias_application_records_display_alias() {
        let interner = TypeInterner::new();
        let evaluated = interner.object(vec![]);
        let application = interner.application(TypeId::STRING, vec![TypeId::NUMBER]);

        record_alias_application(
            &interner,
            AliasApplicationProvenance {
                evaluated,
                application,
            },
            AliasApplicationPriority::PreserveExisting,
        );

        assert_eq!(display_alias(&interner, evaluated), Some(application));
    }
// TSZ_INLINE_TEST_END c2596e84014db8a6ab727c991105003e83c30fb3d599868111df467c2b840c3f

// TSZ_INLINE_TEST_BEGIN 18b6cb80c343f329bdb5a587bcc2a49e7040869c900480b148e6ce701ff80092 92 fresh_object_display_records_properties
    #[test]
    fn fresh_object_display_records_properties() {
        let interner = TypeInterner::new();
        let property_name = interner.intern_string("value");
        let ty = interner.object(vec![]);

        record_fresh_object_literal_display(
            &interner,
            FreshObjectLiteralDisplayProvenance {
                type_id: ty,
                properties: vec![PropertyInfo::new(property_name, TypeId::STRING)],
            },
        );

        let props = interner
            .get_display_properties(ty)
            .expect("display properties");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, property_name);
        assert_eq!(props[0].type_id, TypeId::STRING);
    }
// TSZ_INLINE_TEST_END 18b6cb80c343f329bdb5a587bcc2a49e7040869c900480b148e6ce701ff80092

// TSZ_INLINE_TEST_BEGIN f4ef708ac58fdc7d7314e67b8c0ebe8c23f732211ea0ffe0f04ca51c611fe765 114 flattened_union_origin_records_source_members
    #[test]
    fn flattened_union_origin_records_source_members() {
        let interner = TypeInterner::new();
        let inner = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let union = interner.union(vec![inner, TypeId::BOOLEAN]);

        record_union_origin(
            &interner,
            UnionOriginProvenance {
                union_type_id: union,
                origin_members: vec![inner, TypeId::BOOLEAN],
            },
        );

        assert!(matches!(interner.lookup(union), Some(TypeData::Union(_))));
        assert_eq!(
            interner
                .get_union_origin(union)
                .as_deref()
                .map(Vec::as_slice),
            Some([inner, TypeId::BOOLEAN].as_slice())
        );
    }
// TSZ_INLINE_TEST_END f4ef708ac58fdc7d7314e67b8c0ebe8c23f732211ea0ffe0f04ca51c611fe765
