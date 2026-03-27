//! Transform utilities for syntax analysis.
//!
//! Common functions used by ES5 transformations.

use crate::parser::{NodeArena, NodeIndex, node::NodeAccess, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceTarget {
    Arguments,
    This,
    Super,
}

impl ReferenceTarget {
    const fn identifier_name(self) -> &'static str {
        match self {
            Self::Arguments => "arguments",
            Self::This => "this",
            Self::Super => "super",
        }
    }

    const fn include_keyword_check(self) -> bool {
        matches!(self, Self::This | Self::Super)
    }
}

/// Check if an AST node contains a reference to `this` or `super`.
#[must_use]
pub fn contains_this_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::This)
}

/// Collect `this` references that appear in computed member names of a class.
///
/// This follows the same scope rules as `contains_this_reference`: nested non-arrow
/// functions stop lexical `this` propagation, while computed member names remain
/// part of class-evaluation semantics.
#[must_use]
pub fn collect_class_computed_name_this_references(
    arena: &NodeArena,
    class_idx: NodeIndex,
) -> Vec<NodeIndex> {
    let Some(class_node) = arena.get(class_idx) else {
        return Vec::new();
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return Vec::new();
    };

    let mut refs = Vec::new();
    for &member_idx in &class_data.members.nodes {
        let Some(member) = arena.get(member_idx) else {
            continue;
        };

        let name_idx = match member.kind {
            kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
                arena.get_property_decl(member).map(|prop| prop.name)
            }
            kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
                arena.get_method_decl(member).map(|method| method.name)
            }
            kind if kind == syntax_kind_ext::GET_ACCESSOR
                || kind == syntax_kind_ext::SET_ACCESSOR =>
            {
                arena.get_accessor(member).map(|accessor| accessor.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && arena
                .get(name_idx)
                .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            collect_target_references(arena, name_idx, ReferenceTarget::This, &mut refs);
        }
    }

    refs
}

/// Check if an AST node contains a reference to `super`.
#[must_use]
pub fn contains_super_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::Super)
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

    if target.include_keyword_check() {
        match target {
            ReferenceTarget::This
                if node.kind == SyntaxKind::ThisKeyword as u16
                    || node.kind == SyntaxKind::SuperKeyword as u16 =>
            {
                return true;
            }
            ReferenceTarget::Super if node.kind == SyntaxKind::SuperKeyword as u16 => {
                return true;
            }
            _ => {}
        }
    }

    if node.kind == SyntaxKind::Identifier as u16
        && let Some(identifier) = arena.get_identifier(node)
    {
        return identifier.escaped_text == target.identifier_name();
    }

    target_reference_children(arena, node_idx, target)
        .into_iter()
        .any(|child_idx| contains_target_reference(arena, child_idx, target))
}

fn collect_target_references(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
    refs: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };

    if target.include_keyword_check() {
        match target {
            ReferenceTarget::This if node.kind == SyntaxKind::ThisKeyword as u16 => {
                refs.push(node_idx);
                return;
            }
            ReferenceTarget::Super if node.kind == SyntaxKind::SuperKeyword as u16 => {
                refs.push(node_idx);
                return;
            }
            _ => {}
        }
    }

    if node.kind == SyntaxKind::Identifier as u16
        && let Some(identifier) = arena.get_identifier(node)
        && identifier.escaped_text == target.identifier_name()
    {
        refs.push(node_idx);
        return;
    }

    for child_idx in target_reference_children(arena, node_idx, target) {
        collect_target_references(arena, child_idx, target, refs);
    }
}

fn target_reference_children(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
) -> Vec<NodeIndex> {
    let Some(node) = arena.get(node_idx) else {
        return Vec::new();
    };

    match node.kind {
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION =>
        {
            Vec::new()
        }
        kind if kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION =>
        {
            if let Some(class_data) = arena.get_class(node) {
                let mut children = Vec::new();
                if let Some(modifiers) = class_data.modifiers.as_ref() {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if let Some(heritage_clauses) = class_data.heritage_clauses.as_ref() {
                    children.extend(heritage_clauses.nodes.iter().copied());
                }
                for &member_idx in &class_data.members.nodes {
                    push_computed_member_name(arena, member_idx, &mut children);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node) {
                let mut children = Vec::new();
                if target == ReferenceTarget::This
                    || target == ReferenceTarget::Super
                    || arena
                        .get(method.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(method.name);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node) {
                let mut children = Vec::new();
                if target == ReferenceTarget::This
                    || target == ReferenceTarget::Super
                    || arena
                        .get(accessor.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(accessor.name);
                }
                children
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
        kind if kind == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            if let Some(data) = arena.get_property_assignment(node) {
                let mut children = Vec::new();
                if arena
                    .get(data.name)
                    .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(data.name);
                }
                children.push(data.initializer);
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

fn push_computed_member_name(
    arena: &NodeArena,
    member_idx: NodeIndex,
    children: &mut Vec<NodeIndex>,
) {
    let Some(member) = arena.get(member_idx) else {
        return;
    };

    let name_idx = match member.kind {
        kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
            arena.get_property_decl(member).map(|prop| prop.name)
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(member).map(|method| method.name)
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(member).map(|accessor| accessor.name)
        }
        _ => None,
    };

    if let Some(name_idx) = name_idx
        && arena
            .get(name_idx)
            .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    {
        children.push(name_idx);
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
