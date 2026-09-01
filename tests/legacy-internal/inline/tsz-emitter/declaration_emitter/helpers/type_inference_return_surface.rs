//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_return_surface.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 0a8a528b358f0fe99effe1c073449a4fae47906b488293520cb855df1ce1c51f 256 source_type_detects_nested_conditional_alias_application
    #[test]
    fn source_type_detects_nested_conditional_alias_application() {
        let mut parser = ParserState::new(
            "return-surface.ts".to_string(),
            r#"
type Next<T, Fn> = Fn extends (value: T) => unknown ? (value: T) => ReturnType<Fn> : never;
interface Box<T> {
    pipe<Fn extends (value: T) => unknown>(fn: Fn): Box<Next<T, Fn>>;
}
"#
            .to_string(),
        );
        parser.parse_source_file();
        let emitter = DeclarationEmitter::new(&parser.arena);
        let return_type = parser
            .arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                (node.kind == syntax_kind_ext::TYPE_REFERENCE)
                    .then_some(NodeIndex(idx as u32))
                    .filter(|&idx| {
                        emitter
                            .source_slice_from_arena(&parser.arena, idx)
                            .is_some_and(|text| text.trim() == "Box<Next<T, Fn>>")
                    })
            })
            .expect("method return type");

        assert!(emitter.source_type_contains_conditional_alias_application(
            &parser.arena,
            return_type,
            0
        ));
    }
// TSZ_INLINE_TEST_END 0a8a528b358f0fe99effe1c073449a4fae47906b488293520cb855df1ce1c51f

// TSZ_INLINE_TEST_BEGIN 071acc70c6bd40d693be22b15d534dd5de050636d365cec13b305688e016ebb7 325 exported_single_conditional_alias_reference_is_preservable
    #[test]
    fn exported_single_conditional_alias_reference_is_preservable() {
        assert!(first_function_return_annotation_is_preservable(
            r#"
export type Cond<T> = T extends string ? { s: T } : { n: T };
declare function make<T>(t: T): Cond<T>;
"#,
        ));
    }
// TSZ_INLINE_TEST_END 071acc70c6bd40d693be22b15d534dd5de050636d365cec13b305688e016ebb7

// TSZ_INLINE_TEST_BEGIN 75aece400ee9546064b7e9641eebc8461d963594a27b74181ef58862501a4711 335 unexported_conditional_alias_reference_is_not_preservable
    #[test]
    fn unexported_conditional_alias_reference_is_not_preservable() {
        assert!(!first_function_return_annotation_is_preservable(
            r#"
type Cond<T> = T extends string ? { s: T } : { n: T };
declare function make<T>(t: T): Cond<T>;
"#,
        ));
    }
// TSZ_INLINE_TEST_END 75aece400ee9546064b7e9641eebc8461d963594a27b74181ef58862501a4711

// TSZ_INLINE_TEST_BEGIN 724910ce52a29d27819300421a64408468e1bb63b6b546851a62f0d2b77c23c4 345 exported_non_conditional_alias_reference_is_not_preservable
    #[test]
    fn exported_non_conditional_alias_reference_is_not_preservable() {
        // Non-conditional alias applications already round-trip through the
        // normal reuse path, so they are intentionally excluded.
        assert!(!first_function_return_annotation_is_preservable(
            r#"
export type Plain<T> = { value: T };
declare function make<T>(t: T): Plain<T>;
"#,
        ));
    }
// TSZ_INLINE_TEST_END 724910ce52a29d27819300421a64408468e1bb63b6b546851a62f0d2b77c23c4

// TSZ_INLINE_TEST_BEGIN 80e789282dbad5a3dcd50cf0b27a69a3396f81c7f2d6fdb747a0809c0758a628 357 nested_conditional_alias_reference_is_not_preservable
    #[test]
    fn nested_conditional_alias_reference_is_not_preservable() {
        // The conditional alias is not the *whole* return type, so the alias name
        // does not stand for the inferred type and must not be preserved here.
        assert!(!first_function_return_annotation_is_preservable(
            r#"
export type Cond<T> = T extends string ? { s: T } : { n: T };
declare function wrap<T>(t: T): { value: Cond<T> };
"#,
        ));
    }
// TSZ_INLINE_TEST_END 80e789282dbad5a3dcd50cf0b27a69a3396f81c7f2d6fdb747a0809c0758a628
