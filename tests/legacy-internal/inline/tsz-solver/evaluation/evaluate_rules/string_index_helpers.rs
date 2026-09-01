//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/string_index_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c80318f08c296d7ffc1a7a6ecd436b2013c5a7ba6e147086211137c0dea047ae 214 template_pattern_string_index_rejects_non_matching_literal_key
    #[test]
    fn template_pattern_string_index_rejects_non_matching_literal_key() {
        let db = TypeInterner::new();
        let prefix = db.intern_string("data-");
        let key_type = db.template_literal(vec![
            TemplateSpan::Text(prefix),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });

        let matching = db.literal_string("data-id");
        let non_matching = db.literal_string("other");

        assert_eq!(evaluate_index_access(&db, object, matching), TypeId::NUMBER);
        assert_eq!(
            evaluate_index_access(&db, object, non_matching),
            TypeId::UNDEFINED
        );
    }
// TSZ_INLINE_TEST_END c80318f08c296d7ffc1a7a6ecd436b2013c5a7ba6e147086211137c0dea047ae

// TSZ_INLINE_TEST_BEGIN a8a1379045e4423c14c88dd1207b21313b2ab756c9fd651a6dc4ee8ef518e753 262 numeric_literal_index_into_string_index_signature_resolves_value_type
    #[test]
    fn numeric_literal_index_into_string_index_signature_resolves_value_type() {
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::NUMBER);

        // Reported repro plus adjacent numeric literals.
        for n in [42.0, 0.0, -1.0, 1_000_000.0] {
            assert_eq!(
                evaluate_index_access(&db, object, db.literal_number(n)),
                TypeId::NUMBER,
                "D[{n}] should resolve to the string index value type"
            );
        }

        // `D[number]` and `D[string]` already worked; keep them as controls.
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::NUMBER),
            TypeId::NUMBER
        );
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::STRING),
            TypeId::NUMBER
        );
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_string("x")),
            TypeId::NUMBER
        );
    }
// TSZ_INLINE_TEST_END a8a1379045e4423c14c88dd1207b21313b2ab756c9fd651a6dc4ee8ef518e753

// TSZ_INLINE_TEST_BEGIN abbc9cefeb130b46ce63cb6014044757229bb880a4ddbdce1f6164d2e6070d5f 291 numeric_literal_index_is_structural_over_value_type
    #[test]
    fn numeric_literal_index_is_structural_over_value_type() {
        // Renaming/changing the value type must not affect the rule.
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::BOOLEAN);
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(7.0)),
            TypeId::BOOLEAN
        );
    }
// TSZ_INLINE_TEST_END abbc9cefeb130b46ce63cb6014044757229bb880a4ddbdce1f6164d2e6070d5f

// TSZ_INLINE_TEST_BEGIN fc6556bdde27540e6f82bdefbee69c085ab85b1086de933c0e626f2e9ac79b2f 302 numeric_index_signature_takes_precedence_over_string
    #[test]
    fn numeric_index_signature_takes_precedence_over_string() {
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
        });
        // A numeric literal prefers the number index signature.
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(5.0)),
            TypeId::BOOLEAN
        );
    }
// TSZ_INLINE_TEST_END fc6556bdde27540e6f82bdefbee69c085ab85b1086de933c0e626f2e9ac79b2f

// TSZ_INLINE_TEST_BEGIN cfbfd3c2668bf549de22c68adcf1029b5ef1b354c9ef25add0b817f8b1b41228 330 numeric_index_signature_only_resolves_for_numeric_literal
    #[test]
    fn numeric_index_signature_only_resolves_for_numeric_literal() {
        // `E = { [k: number]: boolean }`; `E[5]` -> boolean control.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(5.0)),
            TypeId::BOOLEAN
        );
    }
// TSZ_INLINE_TEST_END cfbfd3c2668bf549de22c68adcf1029b5ef1b354c9ef25add0b817f8b1b41228

// TSZ_INLINE_TEST_BEGIN 84d321b360ba6953136bd79dd251cf593396471a952d06cdf4ac96045b2e950d 353 numeric_literal_index_without_any_index_signature_is_undefined
    #[test]
    fn numeric_literal_index_without_any_index_signature_is_undefined() {
        // Negative control: no string or number index signature.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(42.0)),
            TypeId::UNDEFINED
        );
    }
// TSZ_INLINE_TEST_END 84d321b360ba6953136bd79dd251cf593396471a952d06cdf4ac96045b2e950d

// TSZ_INLINE_TEST_BEGIN b20bd12205ce89cbc8638bcc9abee1f43bec58a14a472f20c0a27afd7cce0868 371 numeric_literal_does_not_match_template_pattern_string_index
    #[test]
    fn numeric_literal_does_not_match_template_pattern_string_index() {
        // A template-literal-pattern key must NOT be matched by a numeric
        // literal index (the pattern only covers matching string keys).
        let db = TypeInterner::new();
        let prefix = db.intern_string("data-");
        let key_type = db.template_literal(vec![
            TemplateSpan::Text(prefix),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, db.literal_number(42.0)),
            TypeId::UNDEFINED
        );
    }
// TSZ_INLINE_TEST_END b20bd12205ce89cbc8638bcc9abee1f43bec58a14a472f20c0a27afd7cce0868

// TSZ_INLINE_TEST_BEGIN 97665a53a3321c9e8951ee1aaedaa3434e1eac113d5117259a9a01079436a5eb 409 tagged_number_index_resolves_numeric_index_signature
    #[test]
    fn tagged_number_index_resolves_numeric_index_signature() {
        // `{ [x: number]: V }[number & { brand }]` -> V, exactly like `D[number]`.
        // This is the numeric mirror of the existing `string & { brand }` path
        // and the regression repro from operatorsAndIntersectionTypes.ts.
        for value_type in [TypeId::STRING, TypeId::BOOLEAN, TypeId::NUMBER] {
            let db = TypeInterner::new();
            let object = db.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
                properties: Vec::new(),
                string_index: None,
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type,
                    readonly: false,
                    param_name: None,
                }),
                symbol_index: None,
                symbol: None,
            });
            // Vary the brand name/type so the rule is structural, not name-driven.
            let key = tagged_number(&db, "serialNo", TypeId::BOOLEAN);
            assert_eq!(
                evaluate_index_access(&db, object, key),
                value_type,
                "tagged number key must resolve through the numeric index signature"
            );
        }
    }
// TSZ_INLINE_TEST_END 97665a53a3321c9e8951ee1aaedaa3434e1eac113d5117259a9a01079436a5eb

// TSZ_INLINE_TEST_BEGIN 8f0302101d40ce938f5deb9b6a70196c5fa1181ae60090fd4a3efee58f10d55d 439 tagged_number_index_falls_back_to_string_index_signature
    #[test]
    fn tagged_number_index_falls_back_to_string_index_signature() {
        // `{ [x: string]: V }[number & { brand }]` -> V: a numeric key coerces to
        // a string key, so it resolves through a string index signature when no
        // numeric one is present.
        let db = TypeInterner::new();
        let object = plain_string_index_object(&db, TypeId::BOOLEAN);
        let key = tagged_number(&db, "tag", db.literal_number(1.0));
        assert_eq!(evaluate_index_access(&db, object, key), TypeId::BOOLEAN);
    }
// TSZ_INLINE_TEST_END 8f0302101d40ce938f5deb9b6a70196c5fa1181ae60090fd4a3efee58f10d55d

// TSZ_INLINE_TEST_BEGIN 76d91d89eaf92ef990386113ef59c3f434ecc645a641a827aa524e9b134d795f 450 tagged_number_index_prefers_numeric_over_string_index_signature
    #[test]
    fn tagged_number_index_prefers_numeric_over_string_index_signature() {
        // With both signatures present the numeric one wins, like a bare `number`
        // key.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
        });
        let key = tagged_number(&db, "id", TypeId::STRING);
        assert_eq!(evaluate_index_access(&db, object, key), TypeId::BOOLEAN);
    }
// TSZ_INLINE_TEST_END 76d91d89eaf92ef990386113ef59c3f434ecc645a641a827aa524e9b134d795f

// TSZ_INLINE_TEST_BEGIN 5a42c5ff47d0097b7d3947c1b76b424cb34e2f27181235dc1fb3b6c4694b0934 477 tagged_number_index_without_any_index_signature_stays_deferred
    #[test]
    fn tagged_number_index_without_any_index_signature_stays_deferred() {
        // Negative control: no index signature to resolve through. A generic
        // (intersection) key with nothing to match is kept as a deferred
        // `IndexAccess` rather than collapsed to a concrete value type.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        let key = tagged_number(&db, "tag", TypeId::NUMBER);
        let result = evaluate_index_access(&db, object, key);
        assert!(
            matches!(db.lookup(result), Some(TypeData::IndexAccess(_, _))),
            "an unmatched tagged-number key must stay deferred, got {result:?}"
        );
    }
// TSZ_INLINE_TEST_END 5a42c5ff47d0097b7d3947c1b76b424cb34e2f27181235dc1fb3b6c4694b0934

// TSZ_INLINE_TEST_BEGIN 0a71211c5b5e1fa646cd063480dbebc60cd72becff978dc654e65835c8c6029c 499 tagged_string_does_not_match_numeric_index_signature
    #[test]
    fn tagged_string_does_not_match_numeric_index_signature() {
        // Negative control / asymmetry: a `string & { brand }` key is NOT a
        // number subtype, so it must not resolve through a numeric-only index
        // signature (tsc errors on this). It must not collapse to the numeric
        // value type; it stays a deferred `IndexAccess`.
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: None,
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
        });
        let brand_atom = db.intern_string("g");
        let brand = db.object(vec![PropertyInfo::new(brand_atom, TypeId::NUMBER)]);
        let tagged_string = db.intersection2(TypeId::STRING, brand);
        let result = evaluate_index_access(&db, object, tagged_string);
        assert_ne!(
            result,
            TypeId::BOOLEAN,
            "a tagged-string key must not match the numeric index signature"
        );
        assert!(
            matches!(db.lookup(result), Some(TypeData::IndexAccess(_, _))),
            "an unmatched tagged-string key must stay deferred, got {result:?}"
        );
    }
// TSZ_INLINE_TEST_END 0a71211c5b5e1fa646cd063480dbebc60cd72becff978dc654e65835c8c6029c

// TSZ_INLINE_TEST_BEGIN 74d5f650098cbcbc7511f0cb7b65360bd4c3888b09e01415100b9f0e1ed3f8d5 538 property_key_union_index_serves_symbol_and_keyof_includes_symbol
    /// A non-numeric index signature whose key spans `symbol` (here a
    /// `string | number | symbol` union, the structural form of `PropertyKey`)
    /// serves symbol-bearing keys, and its `keyof` includes `symbol`. Regression
    /// for the dropped symbol arm (#14315).
    #[test]
    fn property_key_union_index_serves_symbol_and_keyof_includes_symbol() {
        use crate::evaluation::evaluate::evaluate_keyof;

        let db = TypeInterner::new();
        let key = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: key,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });

        // Indexed access by `symbol` resolves through the signature value type.
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::SYMBOL),
            TypeId::NUMBER,
            "a `string | number | symbol` keyed signature must serve a symbol index"
        );
        // A unique-symbol key is a subtype of `symbol`, so it resolves too.
        let uniq = db.unique_symbol(crate::types::SymbolRef(11));
        assert_eq!(
            evaluate_index_access(&db, object, uniq),
            TypeId::NUMBER,
            "a unique-symbol key must serve through the symbol-bearing signature"
        );

        // `keyof` includes the `symbol` arm.
        let keyof = evaluate_keyof(&db, object);
        let includes_symbol = match db.lookup(keyof) {
            Some(TypeData::Union(members)) => db.type_list(members).contains(&TypeId::SYMBOL),
            _ => keyof == TypeId::SYMBOL,
        };
        assert!(
            includes_symbol,
            "keyof of a symbol-bearing index signature must include `symbol`, got {:?}",
            db.lookup(keyof)
        );
    }
// TSZ_INLINE_TEST_END 74d5f650098cbcbc7511f0cb7b65360bd4c3888b09e01415100b9f0e1ed3f8d5

// TSZ_INLINE_TEST_BEGIN ed9074b2ae59575429d83e949d5e672dfd9e9e449cdb9c6c4aa89927b7ff52ab 587 symbol_only_index_rejects_string_key
    /// A symbol-only index signature serves symbol keys but not bare `string`
    /// (the fix must not over-accept).
    #[test]
    fn symbol_only_index_rejects_string_key() {
        let db = TypeInterner::new();
        let object = db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: Vec::new(),
            string_index: Some(IndexSignature {
                key_type: TypeId::SYMBOL,
                value_type: TypeId::BOOLEAN,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol_index: None,
            symbol: None,
        });
        assert_eq!(
            evaluate_index_access(&db, object, TypeId::SYMBOL),
            TypeId::BOOLEAN,
            "a symbol-only signature must serve a symbol index"
        );
        assert_ne!(
            evaluate_index_access(&db, object, TypeId::STRING),
            TypeId::BOOLEAN,
            "a symbol-only signature must not serve a bare string index"
        );
    }
// TSZ_INLINE_TEST_END ed9074b2ae59575429d83e949d5e672dfd9e9e449cdb9c6c4aa89927b7ff52ab
