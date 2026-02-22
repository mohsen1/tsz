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

/// Get enum member name from a node index (identifier or string literal).
///
/// Returns the identifier's escaped text or the literal's text, or an empty string
/// if the node is neither.
pub(crate) fn enum_member_name(arena: &NodeArena, idx: NodeIndex) -> String {
    let Some(node) = arena.get(idx) else {
        return String::new();
    };
    if let Some(ident) = arena.get_identifier(node) {
        return ident.escaped_text.clone();
    }
    if let Some(lit) = arena.get_literal(node) {
        return lit.text.clone();
    }
    String::new()
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

/// Check if a module body contains any runtime (non-type-only) statements,
/// meaning the module is "instantiated" and needs to be emitted.
///
/// Recursively walks dotted namespaces (e.g., `namespace Foo.Bar`) to find
/// the innermost `MODULE_BLOCK` and checks each statement.
pub(crate) fn is_instantiated_module(arena: &NodeArena, module_body: NodeIndex) -> bool {
    let Some(body_node) = arena.get(module_body) else {
        return false;
    };

    // If body is another MODULE_DECLARATION (dotted namespace like Foo.Bar),
    // recurse into the inner module
    if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
        if let Some(inner_module) = arena.get_module(body_node) {
            return is_instantiated_module(arena, inner_module.body);
        }
        return false;
    }

    // MODULE_BLOCK: check if any statement is a value declaration
    if let Some(block) = arena.get_module_block(body_node)
        && let Some(ref stmts) = block.statements
    {
        for &stmt_idx in &stmts.nodes {
            if let Some(stmt_node) = arena.get(stmt_idx)
                && !is_type_only_module_statement(arena, stmt_node)
            {
                return true;
            }
        }
    }

    false
}

/// Check if a statement inside a module body is purely a type declaration
/// (interface, type alias, type-only import/export, const/declare enum,
/// declare/non-instantiated module).
pub(crate) fn is_type_only_module_statement(arena: &NodeArena, node: &Node) -> bool {
    match node.kind {
        k if k == syntax_kind_ext::INTERFACE_DECLARATION
            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION =>
        {
            true
        }
        k if k == syntax_kind_ext::IMPORT_DECLARATION => {
            if let Some(import_decl) = arena.get_import_decl(node)
                && let Some(clause_node) = arena.get(import_decl.import_clause)
                && let Some(clause) = arena.get_import_clause(clause_node)
            {
                return clause.is_type_only;
            }
            false
        }
        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
            if let Some(export_decl) = arena.get_export_decl(node) {
                if export_decl.is_type_only {
                    return true;
                }
                if let Some(inner_node) = arena.get(export_decl.export_clause) {
                    return is_type_only_module_statement(arena, inner_node);
                }
            }
            false
        }
        k if k == syntax_kind_ext::ENUM_DECLARATION => {
            if let Some(enum_decl) = arena.get_enum(node) {
                return arena.has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                    || arena.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword);
            }
            false
        }
        k if k == syntax_kind_ext::MODULE_DECLARATION => {
            if let Some(module_decl) = arena.get_module(node) {
                return arena.has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
                    || !is_instantiated_module(arena, module_decl.body);
            }
            true
        }
        _ => false,
    }
}

/// Get the text of a module specifier (string literal) node.
///
/// Returns `None` if the index is null or the node is not a string literal.
pub(crate) fn module_specifier_text(arena: &NodeArena, specifier: NodeIndex) -> Option<String> {
    if specifier.is_none() {
        return None;
    }
    let node = arena.get(specifier)?;
    let literal = arena.get_literal(node)?;
    Some(literal.text.clone())
}

/// Check if a property member (property assignment, method, or accessor) has a computed name.
pub(crate) fn is_computed_property_member(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    let name_idx = match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            arena.get_property_assignment(node).map(|p| p.name)
        }
        k if k == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(node).map(|m| m.name)
        }
        k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(node).map(|a| a.name)
        }
        _ => None,
    };

    if let Some(name_idx) = name_idx
        && let Some(name_node) = arena.get(name_idx)
    {
        return name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME;
    }
    false
}

/// Check if a node is a spread element or spread assignment.
pub(crate) fn is_spread_element(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };
    node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT || node.kind == syntax_kind_ext::SPREAD_ELEMENT
}

#[cfg(test)]
#[path = "../../tests/emit_utils.rs"]
mod tests;
