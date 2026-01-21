//! ES5 Enum Transform (IR-based)
//!
//! Transforms TypeScript enums to ES5 IIFE patterns, producing IR nodes.
//!
//! # Patterns
//!
//! ## Numeric Enum (with Reverse Mapping)
//! ```typescript
//! enum E { A, B = 2 }
//! ```
//! Becomes IR that prints as:
//! ```javascript
//! var E;
//! (function (E) {
//!     E[E["A"] = 0] = "A";
//!     E[E["B"] = 2] = "B";
//! })(E || (E = {}));
//! ```
//!
//! ## String Enum (No Reverse Mapping)
//! ```typescript
//! enum S { A = "a" }
//! ```
//! Becomes IR that prints as:
//! ```javascript
//! var S;
//! (function (S) {
//!     S["A"] = "a";
//! })(S || (S = {}));
//! ```
//!
//! ## Const Enum (Erased by Default)
//! ```typescript
//! const enum CE { A = 0 }
//! // usages are inlined
//! ```

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// Transform an enum declaration to IR nodes
pub fn transform_enum_to_ir(arena: &NodeArena, enum_idx: NodeIndex) -> Option<IRNode> {
    let enum_node = arena.get(enum_idx)?;
    let enum_data = arena.get_enum(enum_node)?;

    // Check for const enum - erase by default
    if is_const_enum(arena, &enum_data.modifiers) {
        return None;
    }

    let name = get_identifier_text(arena, enum_data.name)?;
    if name.is_empty() {
        return None;
    }

    // Transform members
    let members = transform_enum_members(arena, &enum_data.members, &name);

    Some(IRNode::EnumIIFE { name, members })
}

/// Transform enum members to IR
fn transform_enum_members(
    arena: &NodeArena,
    members: &NodeList,
    _enum_name: &str,
) -> Vec<EnumMember> {
    let mut result = Vec::new();
    let mut last_value: Option<i64> = None;

    for &member_idx in &members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        let Some(member_data) = arena.get_enum_member(member_node) else {
            continue;
        };

        let member_name = get_member_name(arena, member_data.name);
        if member_name.is_empty() {
            continue;
        }

        let value = if member_data.initializer.is_none() {
            // Auto-increment
            let next_val = last_value.map(|v| v + 1).unwrap_or(0);
            last_value = Some(next_val);
            EnumMemberValue::Auto(next_val)
        } else if is_string_literal(arena, member_data.initializer) {
            // String enum
            last_value = None; // Reset - can't continue after string
            let lit_node = arena.get(member_data.initializer);
            if let Some(lit) = lit_node.and_then(|n| arena.get_literal(n)) {
                EnumMemberValue::String(lit.text.clone())
            } else {
                EnumMemberValue::Auto(0)
            }
        } else {
            // Numeric or computed
            let value = extract_enum_value(arena, member_data.initializer);
            last_value = value.as_numeric().cloned();
            value
        };

        result.push(EnumMember {
            name: member_name,
            value,
        });
    }

    result
}

/// Extract enum member value from an expression
fn extract_enum_value(arena: &NodeArena, idx: NodeIndex) -> EnumMemberValue {
    let Some(node) = arena.get(idx) else {
        return EnumMemberValue::Auto(0);
    };

    match node.kind {
        k if k == SyntaxKind::NumericLiteral as u16 => {
            if let Some(lit) = arena.get_literal(node)
                && let Ok(val) = lit.text.parse::<i64>()
            {
                return EnumMemberValue::Numeric(val);
            }
            EnumMemberValue::Auto(0)
        }
        k if k == SyntaxKind::StringLiteral as u16 => {
            if let Some(lit) = arena.get_literal(node) {
                EnumMemberValue::String(lit.text.clone())
            } else {
                EnumMemberValue::Auto(0)
            }
        }
        k if k == SyntaxKind::Identifier as u16 => {
            // Reference to another enum member - treat as computed
            EnumMemberValue::Computed(Box::new(IRNode::id(
                get_identifier_text(arena, idx).unwrap_or_default(),
            )))
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            // Complex expression - treat as computed
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            // Unary expression - treat as computed
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            // Parenthesized expression - unwrap
            if let Some(paren) = arena.get_parenthesized(node) {
                extract_enum_value(arena, paren.expression)
            } else {
                EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            // E.A reference - treat as computed
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        _ => EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx))),
    }
}

impl EnumMemberValue {
    fn as_numeric(&self) -> Option<&i64> {
        match self {
            EnumMemberValue::Auto(v) => Some(v),
            EnumMemberValue::Numeric(v) => Some(v),
            _ => None,
        }
    }
}

/// Check if an enum is a const enum
fn is_const_enum(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    if let Some(mods) = modifiers {
        for &idx in &mods.nodes {
            if let Some(node) = arena.get(idx)
                && node.kind == SyntaxKind::ConstKeyword as u16
            {
                return true;
            }
        }
    }
    false
}

/// Check if a node is a string literal
fn is_string_literal(arena: &NodeArena, idx: NodeIndex) -> bool {
    if let Some(node) = arena.get(idx) {
        node.kind == SyntaxKind::StringLiteral as u16
    } else {
        false
    }
}

/// Get identifier text from a node index
fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

/// Get member name from a node index (identifier or string literal)
fn get_member_name(arena: &NodeArena, idx: NodeIndex) -> String {
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
