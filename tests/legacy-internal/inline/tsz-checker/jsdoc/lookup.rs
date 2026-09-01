//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/jsdoc/lookup.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 18f013599f249fd650498c464d041ba84bc04d15fcbd3bd5a95f5626b9522f65 1693 jsdoc_direct_lookup_sees_prototype_property_statement_type
    #[test]
    fn jsdoc_direct_lookup_sees_prototype_property_statement_type() {
        let source = r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#;
        let options = crate::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..crate::context::CheckerOptions::default()
        };
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let access_idx = parser
            .get_arena()
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let access = parser.get_arena().get_access_expr(node)?;
                let name = parser
                    .get_arena()
                    .get_identifier_text(access.name_or_argument)?;
                if name != "x" {
                    return None;
                }
                let base = parser.get_arena().get(access.expression)?;
                if base.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let base_access = parser.get_arena().get_access_expr(base)?;
                let base_name = parser
                    .get_arena()
                    .get_identifier_text(base_access.name_or_argument)?;
                (base_name == "prototype").then_some(NodeIndex(idx as u32))
            })
            .expect("missing prototype property access");
        let stmt_idx = enclosing_expression_statement(&parser, access_idx)
            .expect("missing enclosing statement for prototype property access");
        let sf = checker
            .source_file_data_for_node(stmt_idx)
            .expect("missing source file data");
        let raw_leading = checker.try_leading_jsdoc(
            &sf.comments,
            parser
                .get_arena()
                .get(stmt_idx)
                .expect("stmt_idx node must exist")
                .pos,
            &sf.text,
        );
        assert!(
            raw_leading.is_some(),
            "expected raw leading JSDoc for prototype statement"
        );
        let ancestor = checker.jsdoc_type_annotation_for_node(stmt_idx);
        let direct = checker.jsdoc_type_annotation_for_node_direct(stmt_idx);
        assert_eq!(
            ancestor.map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
        assert_eq!(
            direct.map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
    }
// TSZ_INLINE_TEST_END 18f013599f249fd650498c464d041ba84bc04d15fcbd3bd5a95f5626b9522f65

// TSZ_INLINE_TEST_BEGIN f950354f42ff585b118671aaf215c44ecaf0290811b46543824e253941e61db3 1779 jsdoc_typedef_prescan_cache_keys_by_file_and_name
    #[test]
    fn jsdoc_typedef_prescan_cache_keys_by_file_and_name() {
        let source = r#"
/** @typedef {{ value: number }} Payload */
let value;
"#;
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            crate::context::CheckerOptions {
                allow_js: true,
                check_js: true,
                ..crate::context::CheckerOptions::default()
            },
        );
        let source_file = parser
            .get_arena()
            .source_files
            .first()
            .expect("source file should be available after parse");

        assert!(checker.source_file_has_jsdoc_typedef_named_cached(7, 0, source_file, "Payload"));
        assert!(checker.source_file_has_jsdoc_typedef_named_cached(7, 0, source_file, "Payload"));
        assert_eq!(
            checker
                .ctx
                .jsdoc_global_typedef_lookup_cache
                .typedef_presence_by_file
                .len(),
            1
        );

        assert!(!checker.source_file_has_jsdoc_typedef_named_cached(
            7,
            0,
            source_file,
            "MissingPayload"
        ));
        assert!(checker.source_file_has_jsdoc_typedef_named_cached(8, 0, source_file, "Payload"));
        assert_eq!(
            checker
                .ctx
                .jsdoc_global_typedef_lookup_cache
                .typedef_presence_by_file
                .len(),
            3
        );
    }
// TSZ_INLINE_TEST_END f950354f42ff585b118671aaf215c44ecaf0290811b46543824e253941e61db3
