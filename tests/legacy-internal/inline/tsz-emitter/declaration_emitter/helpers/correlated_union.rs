//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/correlated_union.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 83fdfa9249e4cf59ae2b0d32e8fffd629490eabe606127a8bb569caad6b6edcb 1915 correlated_alias_shape_detects_renamed_discriminant_and_callback
    #[test]
    fn correlated_alias_shape_detects_renamed_discriminant_and_callback() {
        let mut parser = ParserState::new(
            "shape.ts".to_string(),
            r#"
interface Registry {
    alpha: AlphaEvent;
}
interface AlphaEvent {
    alpha: true;
}
type Entry<Key extends keyof Registry> = { [Choice in Key]: {
    readonly kind: Choice;
    readonly enabled?: boolean;
    readonly handler: (payload: Registry[Choice]) => void;
}}[Key];
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let alias_type_node = emitter
            .find_type_alias_type_node_in_arena(arena, "Entry")
            .expect("alias type node");
        let shape = emitter
            .correlated_alias_shape(arena, alias_type_node)
            .expect("correlated alias shape");

        assert_eq!(shape.mapped_param_name, "Choice");
        assert_eq!(shape.discriminant_property_name, "kind");
        assert_eq!(shape.callback_property_name, "handler");
        assert_eq!(shape.callback_parameter_name, "payload");
        assert_eq!(shape.callback_map_type_name, "Registry");
        assert_eq!(shape.callback_return_type_text, "void");
        assert_eq!(
            emitter
                .interface_member_type_text_from_arena(arena, "Registry", "alpha")
                .as_deref(),
            Some("AlphaEvent")
        );
    }
// TSZ_INLINE_TEST_END 83fdfa9249e4cf59ae2b0d32e8fffd629490eabe606127a8bb569caad6b6edcb

// TSZ_INLINE_TEST_BEGIN 74b372dfa13aedcebbe6605781161b501e35bd08f0f2b31badfb3e4edab2ce34 1958 mapped_argument_object_members_split_commas_and_semicolons
    #[test]
    fn mapped_argument_object_members_split_commas_and_semicolons() {
        assert_eq!(
            DeclarationEmitter::infer_unwrapped_isomorphic_mapped_argument_text(
                "{ a: Box<number>, b: Box<string[]> }",
                "Box"
            )
            .as_deref(),
            Some("{\n    a: number;\n    b: string[];\n}")
        );
        assert_eq!(
            DeclarationEmitter::infer_required_from_partial_argument_text(
                "{ a: number | undefined, b?: string[] }",
            )
            .as_deref(),
            Some("{\n    a: number;\n    b: string[];\n}")
        );
        assert_eq!(
            DeclarationEmitter::infer_unwrapped_isomorphic_mapped_argument_text(
                "{ a: Box<{ nested: string, count: number }>; b: Box<Array<string, number>> }",
                "Box"
            )
            .as_deref(),
            Some("{\n    a: { nested: string, count: number };\n    b: Array<string, number>;\n}")
        );
    }
// TSZ_INLINE_TEST_END 74b372dfa13aedcebbe6605781161b501e35bd08f0f2b31badfb3e4edab2ce34
