use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

pub(crate) fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    if let Some(text) = arena.identifier_text_owned(idx) {
        return Some(text);
    }
    let node = arena.get(idx)?;

    if node.kind == syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        let mut combined = String::with_capacity(left.len() + 1 + right.len());
        combined.push_str(&left);
        combined.push('.');
        combined.push_str(&right);
        return Some(combined);
    }

    None
}

pub(crate) fn expression_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;

    if node.kind == SyntaxKind::Identifier as u16 || node.kind == syntax_kind_ext::QUALIFIED_NAME {
        return entity_name_text_in_arena(arena, idx);
    }

    if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
        let paren = arena.get_parenthesized(node)?;
        return expression_name_text_in_arena(arena, paren.expression);
    }

    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = expression_name_text_in_arena(arena, access.expression)?;
        let right_node = arena.get(access.name_or_argument)?;
        let right = arena.get_identifier(right_node)?;
        return Some(format!("{left}.{}", right.escaped_text));
    }

    None
}

pub(crate) fn property_access_chain_text_in_arena(
    arena: &NodeArena,
    idx: NodeIndex,
) -> Option<String> {
    if let Some(text) = arena.identifier_text_owned(idx) {
        return Some(text);
    }
    let node = arena.get(idx)?;
    match node.kind {
        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            let access = arena.get_access_expr(node)?;
            let left = property_access_chain_text_in_arena(arena, access.expression)?;
            let right = arena.get_identifier_at(access.name_or_argument)?;
            Some(format!("{left}.{}", right.escaped_text))
        }
        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
            let access = arena.get_access_expr(node)?;
            let left = property_access_chain_text_in_arena(arena, access.expression)?;
            let right = static_element_access_key_text_in_arena(arena, access.name_or_argument)?;
            Some(format!("{left}.{right}"))
        }
        _ => None,
    }
}

pub(crate) fn static_element_access_key_text_in_arena(
    arena: &NodeArena,
    idx: NodeIndex,
) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
        let paren = arena.get_parenthesized(node)?;
        return static_element_access_key_text_in_arena(arena, paren.expression);
    }
    if node.kind == SyntaxKind::StringLiteral as u16
        || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        || node.kind == SyntaxKind::NumericLiteral as u16
    {
        return arena.get_literal(node).map(|literal| literal.text.clone());
    }
    None
}

pub(crate) fn simple_computed_name_expr_text_in_arena(
    arena: &NodeArena,
    idx: NodeIndex,
) -> Option<String> {
    let node = arena.get(idx)?;
    match node.kind {
        k if k == SyntaxKind::Identifier as u16 => arena.identifier_text_owned(idx),
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            let access = arena.get_access_expr(node)?;
            let left = simple_computed_name_expr_text_in_arena(arena, access.expression)?;
            let right = arena.get_identifier_text(access.name_or_argument)?;
            Some(format!("{left}.{right}"))
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            let call = arena.get_call_expr(node)?;
            let callee = simple_computed_name_expr_text_in_arena(arena, call.expression)?;
            let args = call.arguments.as_ref()?;
            if !args.nodes.is_empty() {
                return None;
            }
            Some(format!("{callee}()"))
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            let paren = arena.get_parenthesized(node)?;
            simple_computed_name_expr_text_in_arena(arena, paren.expression)
        }
        _ => None,
    }
}

pub(crate) fn is_zero_arg_call_like_expr_in_arena(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            arena.get_call_expr(node).is_some_and(|call| {
                call.arguments
                    .as_ref()
                    .is_some_and(|args| args.nodes.is_empty())
                    && simple_computed_name_expr_text_in_arena(arena, call.expression).is_some()
            })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => arena
            .get_parenthesized(node)
            .is_some_and(|paren| is_zero_arg_call_like_expr_in_arena(arena, paren.expression)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_parser::parser::ParserState;

    /// Parse `source` and return the first top-level expression statement's
    /// expression node index, plus a borrowed reference to the arena.
    /// Used by the tests to drive the public helpers against well-known shapes.
    fn parse_first_expression(source: &str) -> (ParserState, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let stmt_idx = source_file
            .statements
            .nodes
            .first()
            .copied()
            .expect("at least one statement");
        let stmt_node = arena.get(stmt_idx).expect("statement node");
        assert_eq!(
            stmt_node.kind,
            syntax_kind_ext::EXPRESSION_STATEMENT,
            "expected an expression statement; got kind {}",
            stmt_node.kind
        );
        let expr_stmt = arena
            .get_expression_statement(stmt_node)
            .expect("expression statement");
        let expr_idx = expr_stmt.expression;
        (parser, expr_idx)
    }

    // ---------- entity_name_text_in_arena -----------------------------------

    #[test]
    fn entity_name_returns_identifier_text_for_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            entity_name_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }

    #[test]
    fn entity_name_returns_dotted_text_for_qualified_name_in_type_position() {
        // Trigger a QUALIFIED_NAME by parsing it inside a type alias rhs.
        let mut parser = ParserState::new("test.ts".to_string(), "type T = a.b.c;".to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        // Walk down to the type reference's name (a QualifiedName).
        let root_node = arena.get(root).expect("root node");
        let source_file = arena.get_source_file(root_node).expect("source file");
        let stmt_idx = source_file
            .statements
            .nodes
            .first()
            .copied()
            .expect("type alias statement");
        // Find the deepest QualifiedName under this statement.
        fn find_qualified_name(arena: &NodeArena, idx: NodeIndex) -> Option<NodeIndex> {
            let node = arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                return Some(idx);
            }
            for child in arena.get_children(idx) {
                if let Some(found) = find_qualified_name(arena, child) {
                    return Some(found);
                }
            }
            None
        }
        let qn_idx = find_qualified_name(arena, stmt_idx).expect("qualified name in `a.b.c`");
        assert_eq!(
            entity_name_text_in_arena(arena, qn_idx),
            Some("a.b.c".to_string()),
        );
    }

    #[test]
    fn entity_name_returns_none_for_call_expression() {
        let (parser, idx) = parse_first_expression("foo();");
        assert_eq!(entity_name_text_in_arena(parser.get_arena(), idx), None);
    }

    // ---------- expression_name_text_in_arena -------------------------------

    #[test]
    fn expression_name_handles_property_access() {
        let (parser, idx) = parse_first_expression("a.b.c;");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("a.b.c".to_string()),
        );
    }

    #[test]
    fn expression_name_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(foo);");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }

    #[test]
    fn expression_name_unwraps_parentheses_around_property_access() {
        let (parser, idx) = parse_first_expression("(a.b);");
        assert_eq!(
            expression_name_text_in_arena(parser.get_arena(), idx),
            Some("a.b".to_string()),
        );
    }

    #[test]
    fn expression_name_returns_none_for_call_expression() {
        let (parser, idx) = parse_first_expression("foo();");
        assert_eq!(expression_name_text_in_arena(parser.get_arena(), idx), None,);
    }

    // ---------- property_access_chain_text_in_arena -------------------------

    #[test]
    fn chain_text_handles_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }

    #[test]
    fn chain_text_handles_property_access_chain() {
        let (parser, idx) = parse_first_expression("a.b.c;");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            Some("a.b.c".to_string()),
        );
    }

    #[test]
    fn chain_text_returns_none_for_parenthesized() {
        // Unlike `expression_name_text_in_arena`, the chain helper does NOT
        // recurse through parentheses.
        let (parser, idx) = parse_first_expression("(foo);");
        assert_eq!(
            property_access_chain_text_in_arena(parser.get_arena(), idx),
            None,
        );
    }

    // ---------- simple_computed_name_expr_text_in_arena ---------------------

    #[test]
    fn simple_computed_name_handles_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("foo".to_string()),
        );
    }

    #[test]
    fn simple_computed_name_handles_zero_arg_call() {
        let (parser, idx) = parse_first_expression("Symbol.iterator();");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("Symbol.iterator()".to_string()),
        );
    }

    #[test]
    fn simple_computed_name_rejects_call_with_args() {
        let (parser, idx) = parse_first_expression("foo(1);");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            None,
        );
    }

    #[test]
    fn simple_computed_name_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(a.b);");
        assert_eq!(
            simple_computed_name_expr_text_in_arena(parser.get_arena(), idx),
            Some("a.b".to_string()),
        );
    }

    // ---------- is_zero_arg_call_like_expr_in_arena -------------------------

    #[test]
    fn is_zero_arg_call_like_true_for_zero_arg_call() {
        let (parser, idx) = parse_first_expression("Symbol.iterator();");
        assert!(is_zero_arg_call_like_expr_in_arena(parser.get_arena(), idx));
    }

    #[test]
    fn is_zero_arg_call_like_false_for_call_with_args() {
        let (parser, idx) = parse_first_expression("foo(1);");
        assert!(!is_zero_arg_call_like_expr_in_arena(
            parser.get_arena(),
            idx
        ));
    }

    #[test]
    fn is_zero_arg_call_like_false_for_bare_identifier() {
        let (parser, idx) = parse_first_expression("foo;");
        assert!(!is_zero_arg_call_like_expr_in_arena(
            parser.get_arena(),
            idx
        ));
    }

    #[test]
    fn is_zero_arg_call_like_unwraps_parentheses() {
        let (parser, idx) = parse_first_expression("(foo());");
        assert!(is_zero_arg_call_like_expr_in_arena(parser.get_arena(), idx));
    }
}
