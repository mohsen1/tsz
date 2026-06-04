#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerOptions, ScriptTarget};
    use crate::query_boundaries::type_construction::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::node::NodeArena;

    fn find_node_by_text_and_kind(
        arena: &NodeArena,
        source: &str,
        kind: u16,
        text: &str,
    ) -> Option<NodeIndex> {
        (0..arena.len()).find_map(|i| {
            let idx = NodeIndex(i as u32);
            let node = arena.get(idx)?;
            (node.kind == kind && &source[node.pos as usize..node.end as usize] == text)
                .then_some(idx)
        })
    }

    #[test]
    fn get_class_name_from_expression_resolves_named_class_expression_return() {
        use tsz_parser::parser::syntax_kind_ext;

        let source = r#"
const C = class D {
    static #field = D.#method();
    static #method() { return 42; }
    static getClass() { return D; }
};

C.getClass().#method;
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );

        checker.check_source_file(root);

        let call_idx = find_node_by_text_and_kind(
            parser.get_arena(),
            source,
            syntax_kind_ext::CALL_EXPRESSION,
            "C.getClass()",
        )
        .expect("expected to find `C.getClass()` call expression");

        assert_eq!(
            checker.get_class_name_from_expression(call_idx),
            Some("D".to_string())
        );
    }
}
