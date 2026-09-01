//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/class_type/js_class_properties.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e08b7c3c9706d094b149e5da85072aaceb45c913eff5304eac6ba70577358adb 1605 quick_prescan_class_members_keeps_method_placeholders
    #[test]
    fn quick_prescan_class_members_keeps_method_placeholders() {
        let source = r#"
abstract class Boxed<T> {
    readonly value!: T;
    abstract parse(input: T): T;
    sync(input: T): T {
        return this.parse(input);
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let source_file = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), source_file);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        checker.ctx.set_lib_contexts(Vec::new());

        let source_file_node = checker
            .ctx
            .arena
            .get(source_file)
            .expect("source file node");
        let source_file_data = checker
            .ctx
            .arena
            .get_source_file(source_file_node)
            .expect("source file data");
        let class_idx = *source_file_data
            .statements
            .nodes
            .first()
            .expect("class statement");
        let class = checker
            .ctx
            .arena
            .get_class_at(class_idx)
            .expect("class declaration");
        let partial = checker.quick_prescan_class_members(class_idx, class);
        let shape = object_shape_for_type(checker.ctx.types, partial).expect("object shape");

        let parse_name = checker.ctx.types.intern_string("parse");
        let parse_prop = shape
            .properties
            .iter()
            .find(|prop| prop.name == parse_name)
            .expect("abstract method placeholder");
        assert!(parse_prop.is_method);
        assert!(
            callable_shape_for_type(checker.ctx.types, parse_prop.type_id)
                .is_some_and(|shape| !shape.call_signatures.is_empty()),
            "quick prescan should keep callable method placeholders"
        );

        let sync_name = checker.ctx.types.intern_string("sync");
        assert!(
            shape
                .properties
                .iter()
                .any(|prop| prop.name == sync_name && prop.is_method),
            "quick prescan should include concrete methods too"
        );
    }
// TSZ_INLINE_TEST_END e08b7c3c9706d094b149e5da85072aaceb45c913eff5304eac6ba70577358adb
