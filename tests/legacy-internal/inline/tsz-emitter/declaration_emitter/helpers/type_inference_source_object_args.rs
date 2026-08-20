//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_source_object_args.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 325568a78216476347321a22b10db540be4aaa1c375c09cfd25572f6b7a4f530 1247 mapped_accessor_object_argument_infers_public_value_map
    #[test]
    fn mapped_accessor_object_argument_infers_public_value_map() {
        let mut parser = ParserState::new(
            "accessor-map.ts".to_string(),
            r#"
type Accessor<V> = {
    get?(): V;
    set?(value: V): void;
};
type AccessorBag<S> = { [Key in keyof S]: (() => S[Key]) | Accessor<S[Key]> };
type Options<S> = {
    computed?: AccessorBag<S>;
};
let arg = {
    computed: {
        total(): number {
            return 1;
        },
        label: {
            get() {
                return "ready";
            },
            set(value: string) {
            }
        }
    }
};
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let options_type = emitter
            .find_type_alias_type_node_in_arena(arena, "Options")
            .expect("options alias type");
        let arg_idx = arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let node_idx = NodeIndex(idx as u32);
                (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && emitter
                        .object_literal_member_by_name(node_idx, "computed")
                        .is_some())
                .then_some(node_idx)
            })
            .expect("argument object literal");
        let computed_member_idx = emitter
            .object_literal_member_by_name(arg_idx, "computed")
            .expect("computed member");
        assert_eq!(
            emitter
                .object_literal_property_value_map_type_text_with_context(computed_member_idx, &[]),
            Some("{\n    total: number;\n    label: string;\n}".to_string())
        );
        let mut substitutions = Vec::new();
        emitter.infer_object_argument_substitutions_from_type_node(
            arena,
            options_type,
            arg_idx,
            &["S".to_string()],
            &[],
            &mut substitutions,
            0,
        );

        assert_eq!(
            substitutions,
            vec![(
                "S".to_string(),
                "{\n    total: number;\n    label: string;\n}".to_string()
            )]
        );
    }
// TSZ_INLINE_TEST_END 325568a78216476347321a22b10db540be4aaa1c375c09cfd25572f6b7a4f530

// TSZ_INLINE_TEST_BEGIN d79e3f461e0ecbb83b083911efc776ab958bd1ca72e9b776f7b5eda8300edd57 1324 object_literal_method_and_accessor_this_property_returns_use_sibling_public_types
    #[test]
    fn object_literal_method_and_accessor_this_property_returns_use_sibling_public_types() {
        let mut parser = ParserState::new(
            "this-property-public-type.ts".to_string(),
            r#"
let arg = {
    a: 1,
    b: "ready",
    f() {
        return this.a;
    },
    get d() {
        return this.a;
    },
    get e() {
        return this.b;
    }
};
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let arg_idx = arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let node_idx = NodeIndex(idx as u32);
                (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && emitter
                        .object_literal_member_by_name(node_idx, "f")
                        .is_some())
                .then_some(node_idx)
            })
            .expect("argument object literal");

        assert_eq!(
            emitter.object_literal_public_type_text_with_context(arg_idx, None, &[]),
            Some(
                "{\n    a: number;\n    b: string;\n    f(): number;\n    d: number;\n    e: string;\n}"
                    .to_string()
            )
        );
    }
// TSZ_INLINE_TEST_END d79e3f461e0ecbb83b083911efc776ab958bd1ca72e9b776f7b5eda8300edd57

// TSZ_INLINE_TEST_BEGIN d9fc68a3fcc762a12317893cc2b1a7557cd8a1e721fcfe2da95efb1ce4dc9a18 1371 this_type_context_marker_is_not_a_value_map_inference_mention
    #[test]
    fn this_type_context_marker_is_not_a_value_map_inference_mention() {
        let mut parser = ParserState::new(
            "this-type-context-marker.ts".to_string(),
            r#"
type ContextOnly<Model> = ThisType<Model>;
type ValueAndContext<Model> = Model & ThisType<{ current: Model }>;
type AliasAndContext<Model> = ValueAlias & ThisType<Model>;
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let context_only_type = emitter
            .find_type_alias_type_node_in_arena(arena, "ContextOnly")
            .expect("context-only alias type");
        let value_and_context_type = emitter
            .find_type_alias_type_node_in_arena(arena, "ValueAndContext")
            .expect("value-and-context alias type");
        let alias_and_context_type = emitter
            .find_type_alias_type_node_in_arena(arena, "AliasAndContext")
            .expect("alias-and-context alias type");
        let type_params = ["Model".to_string()];
        let aliases = [("ValueAlias".to_string(), "Model".to_string())];

        assert!(
            !DeclarationEmitter::type_node_mentions_mapped_name_outside_this_type(
                arena,
                context_only_type,
                "Model",
                &type_params,
                &aliases,
                0,
            ),
            "`ThisType<Model>` is only contextual and must not infer Model"
        );
        assert!(
            DeclarationEmitter::type_node_mentions_mapped_name_outside_this_type(
                arena,
                value_and_context_type,
                "Model",
                &type_params,
                &aliases,
                0,
            ),
            "Model outside `ThisType` should still infer"
        );
        assert!(
            DeclarationEmitter::type_node_mentions_mapped_name_outside_this_type(
                arena,
                alias_and_context_type,
                "Model",
                &type_params,
                &aliases,
                0,
            ),
            "aliases outside `ThisType` should still infer"
        );
    }
// TSZ_INLINE_TEST_END d9fc68a3fcc762a12317893cc2b1a7557cd8a1e721fcfe2da95efb1ce4dc9a18

// TSZ_INLINE_TEST_BEGIN 2e3c030a3ddd60907ee7eda1d02323a8e6aef75441589e5d3af6d1dd56696dba 1432 non_mapped_generic_member_alias_does_not_infer_object_value_map
    #[test]
    fn non_mapped_generic_member_alias_does_not_infer_object_value_map() {
        let mut parser = ParserState::new(
            "non-mapped-wrapper.ts".to_string(),
            r#"
type Wrapper<V> = { value: V };
type Options<S> = { computed?: Wrapper<S> };
let arg = {
    computed: {
        total(): number {
            return 1;
        },
        label: {
            get() {
                return "ready";
            }
        }
    }
};
"#
            .to_string(),
        );
        parser.parse_source_file();
        let arena = parser.get_arena();
        let emitter = DeclarationEmitter::new(arena);
        let options_type = emitter
            .find_type_alias_type_node_in_arena(arena, "Options")
            .expect("options alias type");
        let arg_idx = arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let node_idx = NodeIndex(idx as u32);
                (node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && emitter
                        .object_literal_member_by_name(node_idx, "computed")
                        .is_some())
                .then_some(node_idx)
            })
            .expect("argument object literal");
        let mut substitutions = Vec::new();
        emitter.infer_object_argument_substitutions_from_type_node(
            arena,
            options_type,
            arg_idx,
            &["S".to_string()],
            &[],
            &mut substitutions,
            0,
        );

        assert!(
            substitutions.is_empty(),
            "non-mapped wrappers must not infer object value maps: {substitutions:?}"
        );
    }
// TSZ_INLINE_TEST_END 2e3c030a3ddd60907ee7eda1d02323a8e6aef75441589e5d3af6d1dd56696dba
