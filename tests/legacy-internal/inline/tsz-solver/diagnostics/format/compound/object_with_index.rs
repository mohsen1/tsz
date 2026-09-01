//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/diagnostics/format/compound/object_with_index.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7dcd9270fcf59461ef0fdfda772afc8f4c534797c20fa21be32eb6fde33f88ea 164 format_array_like_object_with_index_prefers_es5_display_head
    #[test]
    fn format_array_like_object_with_index_prefers_es5_display_head() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
        let includes = db.function(FunctionShape::new(
            vec![ParamInfo {
                name: Some(db.intern_string("searchElement")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            TypeId::BOOLEAN,
        ));

        let shape = crate::types::ObjectShape {
            properties: vec![
                PropertyInfo::new(db.intern_string("includes"), includes),
                PropertyInfo::new(db.intern_string("toString"), method),
                PropertyInfo::new(db.intern_string("toLocaleString"), method),
            ],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);

        assert!(
            result.starts_with("{ [x: number]: number; toString:"),
            "Expected Array display head after index signature, got: {result}"
        );
    }
// TSZ_INLINE_TEST_END 7dcd9270fcf59461ef0fdfda772afc8f4c534797c20fa21be32eb6fde33f88ea

// TSZ_INLINE_TEST_BEGIN a8e12d31a06eb22df98ea483f08cab8bc95eeaab832e5654a4cd5735d93a0f07 205 format_array_like_object_with_symbol_tail_omits_late_methods
    #[test]
    fn format_array_like_object_with_symbol_tail_omits_late_methods() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let method = db.function(FunctionShape::new(vec![], TypeId::STRING));
        let includes = db.function(FunctionShape::new(
            vec![ParamInfo {
                name: Some(db.intern_string("searchElement")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            TypeId::BOOLEAN,
        ));

        let mut properties = vec![
            PropertyInfo::new(db.intern_string("toString"), method),
            PropertyInfo::new(db.intern_string("toLocaleString"), method),
            PropertyInfo::new(db.intern_string("pop"), method),
            PropertyInfo::new(db.intern_string("push"), method),
            PropertyInfo::new(db.intern_string("includes"), includes),
        ];
        properties
            .extend((1..=27).map(|idx| {
                PropertyInfo::new(db.intern_string(&format!("p{idx}")), TypeId::NUMBER)
            }));
        properties.push(PropertyInfo {
            name: db.intern_string("[Symbol.unscopables]"),
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: true,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: true,
            single_quoted_name: false,
            non_widening: false,
        });

        let shape = crate::types::ObjectShape {
            properties,
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            symbol_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);

        assert!(
            result.contains("... 30 more ..."),
            "Expected tsc-style omitted count for array-like display, got: {result}"
        );
        assert!(
            result.contains("readonly [Symbol.unscopables]: any"),
            "Expected symbol tail for truncated mapped-array display, got: {result}"
        );
        assert!(
            !result.contains("pop:") && !result.contains("push:"),
            "Expected late array methods to stay behind the omitted marker, got: {result}"
        );
    }
// TSZ_INLINE_TEST_END a8e12d31a06eb22df98ea483f08cab8bc95eeaab832e5654a4cd5735d93a0f07
