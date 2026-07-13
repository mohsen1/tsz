//! Binding-name collectors for the `CommonJS` module transform.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Collect exported names from a variable declaration, including binding patterns.
pub(crate) fn collect_declaration_names(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    exports: &mut Vec<String>,
) {
    let Some(decl_node) = arena.get(decl_idx) else {
        return;
    };

    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
        if let Some(decl_list) = arena.get_variable(decl_node) {
            for &inner_decl_idx in &decl_list.declarations.nodes {
                collect_declaration_names(arena, inner_decl_idx, exports);
            }
        }
        return;
    }

    if let Some(decl) = arena.get_variable_declaration(decl_node) {
        collect_binding_names(arena, decl.name, exports);
    }
}

fn collect_binding_names(arena: &NodeArena, name_idx: NodeIndex, exports: &mut Vec<String>) {
    if name_idx.is_none() {
        return;
    }

    let Some(node) = arena.get(name_idx) else {
        return;
    };

    if node.kind == SyntaxKind::Identifier as u16 {
        if let Some(id) = arena.get_identifier(node) {
            exports.push(id.escaped_text.to_string());
        }
        return;
    }

    match node.kind {
        k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
        {
            if let Some(pattern) = arena.get_binding_pattern(node) {
                for &elem_idx in &pattern.elements.nodes {
                    collect_binding_names_from_element(arena, elem_idx, exports);
                }
            }
        }
        k if k == syntax_kind_ext::BINDING_ELEMENT => {
            if let Some(elem) = arena.get_binding_element(node) {
                collect_binding_names(arena, elem.name, exports);
            }
        }
        _ => {}
    }
}

fn collect_binding_names_from_element(
    arena: &NodeArena,
    elem_idx: NodeIndex,
    exports: &mut Vec<String>,
) {
    if elem_idx.is_none() {
        return;
    }

    let Some(elem_node) = arena.get(elem_idx) else {
        return;
    };

    if let Some(elem) = arena.get_binding_element(elem_node) {
        collect_binding_names(arena, elem.name, exports);
    }
}
