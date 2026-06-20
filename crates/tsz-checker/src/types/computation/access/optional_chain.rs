use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

pub(crate) fn is_optional_chain(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                access.question_dot_token || is_optional_chain(arena, access.expression)
            } else {
                false
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            if node.is_optional_chain() {
                return true;
            }
            if let Some(call) = arena.get_call_expr(node) {
                is_optional_chain(arena, call.expression)
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(crate) fn optional_chain_root(arena: &NodeArena, idx: NodeIndex) -> NodeIndex {
    let Some(node) = arena.get(idx) else {
        return idx;
    };
    match node.kind {
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                return optional_chain_root(arena, call.expression);
            }
            idx
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                return optional_chain_root(arena, access.expression);
            }
            idx
        }
        _ => idx,
    }
}
