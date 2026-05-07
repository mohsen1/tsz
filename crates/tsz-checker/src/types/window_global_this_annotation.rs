use crate::context::CheckerContext;
use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

pub(crate) fn is_window_and_typeof_global_this_type_node(
    arena: &NodeArena,
    mut idx: NodeIndex,
) -> bool {
    loop {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let Some(wrapped) = arena.get_wrapped_type(node) else {
                return false;
            };
            idx = wrapped.type_node;
            continue;
        }
        if node.kind != syntax_kind_ext::INTERSECTION_TYPE {
            return false;
        }
        let Some(composite) = arena.get_composite_type(node) else {
            return false;
        };
        if composite.types.nodes.len() != 2 {
            return false;
        }

        let mut has_window = false;
        let mut has_global_this = false;
        for &member in &composite.types.nodes {
            has_window |= is_window_type_reference_node(arena, member);
            has_global_this |=
                super::type_node_helpers::is_typeof_global_this_type_node(arena, member);
        }
        return has_window && has_global_this;
    }
}

pub(crate) fn declared_type_annotation_for_symbol(
    ctx: &CheckerContext<'_>,
    sym_id: SymbolId,
) -> Option<NodeIndex> {
    let symbol = ctx.binder.get_symbol(sym_id)?;
    let mut decl = symbol.value_declaration;
    if decl.is_none() {
        decl = symbol.primary_declaration()?;
    }
    let decl_node = ctx.arena.get(decl)?;
    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        let var_decl = ctx.arena.get_variable_declaration(decl_node)?;
        return var_decl
            .type_annotation
            .is_some()
            .then_some(var_decl.type_annotation);
    }
    if decl_node.kind == syntax_kind_ext::PARAMETER {
        let param = ctx.arena.get_parameter(decl_node)?;
        return param
            .type_annotation
            .is_some()
            .then_some(param.type_annotation);
    }
    if decl_node.kind == SyntaxKind::Identifier as u16 {
        let parent = ctx.arena.get_extended(decl)?.parent;
        let parent_node = ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::PARAMETER {
            let param = ctx.arena.get_parameter(parent_node)?;
            return (param.name == decl && param.type_annotation.is_some())
                .then_some(param.type_annotation);
        }
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = ctx.arena.get_variable_declaration(parent_node)?;
            return (var_decl.name == decl && var_decl.type_annotation.is_some())
                .then_some(var_decl.type_annotation);
        }
    }
    None
}

fn is_window_type_reference_node(arena: &NodeArena, mut idx: NodeIndex) -> bool {
    loop {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let Some(wrapped) = arena.get_wrapped_type(node) else {
                return false;
            };
            idx = wrapped.type_node;
            continue;
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };
        return arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "Window");
    }
}
