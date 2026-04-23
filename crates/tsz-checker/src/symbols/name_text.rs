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
    if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
        return None;
    }

    let access = arena.get_access_expr(node)?;
    let left = property_access_chain_text_in_arena(arena, access.expression)?;
    let right = arena.get_identifier_at(access.name_or_argument)?;
    Some(format!("{left}.{}", right.escaped_text))
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
