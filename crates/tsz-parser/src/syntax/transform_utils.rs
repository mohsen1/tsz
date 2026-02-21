//! Transform utilities for syntax analysis.
//!
//! Common functions used by ES5 transformations.

use crate::parser::{NodeArena, NodeIndex, node::NodeAccess, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy)]
enum ReferenceTarget {
    Arguments,
    This,
}

impl ReferenceTarget {
    const fn identifier_name(self) -> &'static str {
        match self {
            Self::Arguments => "arguments",
            Self::This => "this",
        }
    }

    const fn include_keyword_check(self) -> bool {
        matches!(self, Self::This)
    }
}

/// Check if an AST node contains a reference to `this` or `super`.
#[must_use]
pub fn contains_this_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::This)
}

/// Check if a node contains a reference to `arguments`.
///
/// This is used to determine if an arrow function needs to capture the parent's
/// `arguments` object for ES5 downleveling.
///
/// Important: Regular functions have their own `arguments`, so we don't recurse
/// into them. Only arrow functions inherit the parent's `arguments`.
#[must_use]
pub fn contains_arguments_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::Arguments)
}

fn contains_target_reference(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    if target.include_keyword_check()
        && (node.kind == SyntaxKind::ThisKeyword as u16
            || node.kind == SyntaxKind::SuperKeyword as u16)
    {
        return true;
    }

    if node.kind == SyntaxKind::Identifier as u16
        && let Some(identifier) = arena.get_identifier(node)
    {
        return identifier.escaped_text == target.identifier_name();
    }

    target_reference_children(arena, node_idx)
        .into_iter()
        .any(|child_idx| contains_target_reference(arena, child_idx, target))
}

fn target_reference_children(arena: &NodeArena, node_idx: NodeIndex) -> Vec<NodeIndex> {
    let Some(node) = arena.get(node_idx) else {
        return Vec::new();
    };

    match node.kind {
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION =>
        {
            Vec::new()
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node) {
                Vec::from([method.name])
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node) {
                Vec::from([accessor.name])
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::ARROW_FUNCTION => {
            if let Some(func) = arena.get_function(node) {
                let mut children = Vec::new();
                for &param_idx in &func.parameters.nodes {
                    let Some(param_node) = arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = arena.get_parameter(param_node) else {
                        continue;
                    };
                    if param.initializer.is_some() {
                        children.push(param.initializer);
                    }
                }
                if func.body.is_some() {
                    children.push(func.body);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
        {
            if let Some(var_stmt) = arena.get_variable(node) {
                var_stmt.declarations.nodes.clone()
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node) {
                if decl.initializer.is_none() {
                    Vec::new()
                } else {
                    vec![decl.initializer]
                }
            } else {
                Vec::new()
            }
        }
        _ => arena.get_children(node_idx),
    }
}

/// Check if a node is a private identifier (#field)
#[must_use]
pub fn is_private_identifier(arena: &NodeArena, name_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(name_idx) else {
        return false;
    };
    node.kind == SyntaxKind::PrivateIdentifier as u16
}
