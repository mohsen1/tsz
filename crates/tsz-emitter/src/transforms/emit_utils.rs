use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_scanner::SyntaxKind;

/// Get identifier text from a node index, returning `None` if the node is not an identifier.
pub(crate) fn identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

/// Get identifier text from a node index, returning an empty string on failure.
pub(crate) fn identifier_text_or_empty(arena: &NodeArena, idx: NodeIndex) -> String {
    identifier_text(arena, idx).unwrap_or_default()
}

#[cfg(test)]
#[path = "../../tests/emit_utils.rs"]
mod tests;
