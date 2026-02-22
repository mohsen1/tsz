use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
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

/// Check if an export declaration has any runtime (non-type-only) value that needs to be emitted.
///
/// Returns `false` for `export type { ... }`, re-exports of only type-only specifiers,
/// and exports of declarations that are purely types (interfaces, type aliases, const enums,
/// declare-prefixed classes/functions/variables/modules).
pub(crate) fn export_decl_has_runtime_value(
    arena: &NodeArena,
    export_decl: &tsz_parser::parser::node::ExportDeclData,
) -> bool {
    if export_decl.is_type_only {
        return false;
    }

    if export_decl.is_default_export {
        return true;
    }

    if export_decl.export_clause.is_none() {
        return true;
    }

    let Some(clause_node) = arena.get(export_decl.export_clause) else {
        return false;
    };

    if let Some(named) = arena.get_named_imports(clause_node) {
        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        return false;
    }

    if export_clause_is_type_only(arena, clause_node) {
        return false;
    }

    true
}

/// Check if an export clause (the declaration after `export`) is type-only.
///
/// Returns `true` for interfaces, type aliases, const/declare enums,
/// and declare-prefixed classes, functions, variables, and modules.
pub(crate) fn export_clause_is_type_only(arena: &NodeArena, clause_node: &Node) -> bool {
    match clause_node.kind {
        k if k == syntax_kind_ext::INTERFACE_DECLARATION => true,
        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            let Some(enum_decl) = arena.get_enum(clause_node) else {
                return false;
            };
            arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                || arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
        }
        k if k == syntax_kind_ext::CLASS_DECLARATION => {
            let Some(class_decl) = arena.get_class(clause_node) else {
                return false;
            };
            arena.has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
            let Some(func_decl) = arena.get_function(clause_node) else {
                return false;
            };
            arena.has_modifier(&func_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            let Some(var_decl) = arena.get_variable(clause_node) else {
                return false;
            };
            arena.has_modifier(&var_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            let Some(module_decl) = arena.get_module(clause_node) else {
                return false;
            };
            arena.has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../../tests/emit_utils.rs"]
mod tests;
