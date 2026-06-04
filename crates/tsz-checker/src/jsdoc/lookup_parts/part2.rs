#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::node::NodeAccess;
    use tsz_solver::construction::TypeInterner;

    fn enclosing_expression_statement(parser: &ParserState, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..6 {
            let ext = parser.get_arena().get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = parser.get_arena().get(parent)?;
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

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
}
