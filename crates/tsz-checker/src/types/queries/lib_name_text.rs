//! Entity-name text helpers shared by lib query resolution paths.

use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;

pub(super) fn entity_name_text_in_arena(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;

    if node.kind == SyntaxKind::Identifier as u16 {
        return arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone());
    }

    if node.kind == syntax_kind_ext::QUALIFIED_NAME {
        let qn = arena.get_qualified_name(node)?;
        let left = entity_name_text_in_arena(arena, qn.left)?;
        let right = entity_name_text_in_arena(arena, qn.right)?;
        return Some(format!("{left}.{right}"));
    }

    if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        && let Some(access) = arena.get_access_expr(node)
    {
        let left = entity_name_text_in_arena(arena, access.expression)?;
        let right = arena
            .get(access.name_or_argument)
            .and_then(|right_node| arena.get_identifier(right_node))?;
        return Some(format!("{left}.{}", right.escaped_text));
    }

    None
}

pub(super) fn entity_name_text_from_decl_arenas(
    node_idx: NodeIndex,
    decl_arenas: &[(NodeIndex, &NodeArena)],
    fallback_arena: &NodeArena,
) -> Option<String> {
    for (_, arena) in decl_arenas {
        if let Some(name) = entity_name_text_in_arena(arena, node_idx) {
            return Some(name);
        }
    }
    entity_name_text_in_arena(fallback_arena, node_idx)
}
