//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/indexed_access_key_space.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9f7851926221af8eb238c22e3be2b77c8d7b0bf05c6c71ef81eab82d98901790 48 constructs_indexed_access_key_space_surfaces
    #[test]
    fn constructs_indexed_access_key_space_surfaces() {
        let db = TypeInterner::new();
        let atom = db.intern_string("field");
        let string_key = literal_string_key(&db, atom);
        let number_key = literal_number_key(&db, 1.0);

        assert_eq!(string_key, db.literal_string_atom(atom));
        assert_eq!(number_key, db.literal_number(1.0));
        assert_eq!(keyof_type(&db, TypeId::STRING), db.keyof(TypeId::STRING));
        assert_eq!(
            indexed_access_type(&db, TypeId::STRING, number_key),
            db.index_access(TypeId::STRING, number_key)
        );
        assert_eq!(literal_key_union(&db, vec![]), None);
        assert_eq!(
            literal_key_union(&db, vec![string_key, number_key]),
            Some(db.union(vec![string_key, number_key]))
        );
        assert_eq!(
            key_space_union(&db, vec![TypeId::STRING, TypeId::NUMBER]),
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        );
        assert_eq!(
            string_or_number_key_space(&db),
            db.union2(TypeId::STRING, TypeId::NUMBER)
        );
    }
// TSZ_INLINE_TEST_END 9f7851926221af8eb238c22e3be2b77c8d7b0bf05c6c71ef81eab82d98901790
