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

use crate::transforms::emit_utils::identifier_text as get_identifier_text;
use crate::transforms::ir::{EnumMember, EnumMemberValue, IRNode};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Transform an enum declaration to IR nodes
pub fn transform_enum_to_ir(arena: &NodeArena, enum_idx: NodeIndex) -> Option<IRNode> {
    let enum_data = arena.get_enum_at(enum_idx)?;

    // Check for const enum - erase by default
    if arena.has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword) {
        return None;
    }

    let name = get_identifier_text(arena, enum_data.name)?;
    if name.is_empty() {
        return None;
    }

    // Transform members
    let members = transform_enum_members(arena, &enum_data.members, &name);

    Some(IRNode::EnumIIFE {
        name: name.into(),
        members,
        namespace_export: None,
    })
}

/// Resolved enum member value for cross-referencing during evaluation.
#[derive(Clone)]
enum ResolvedValue {
    Numeric(i64),
    String(std::borrow::Cow<'static, str>),
}

/// Transform enum members to IR
fn transform_enum_members(
    arena: &NodeArena,
    members: &NodeList,
    enum_name: &str,
) -> Vec<EnumMember> {
    let mut result = Vec::new();
    let mut last_value: Option<i64> = None;
    // Track resolved member values for cross-reference evaluation
    let mut resolved: Vec<(String, ResolvedValue)> = Vec::new();

    for &member_idx in &members.nodes {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        let Some(member_data) = arena.get_enum_member(member_node) else {
            continue;
        };

        let member_name = crate::transforms::emit_utils::enum_member_name(arena, member_data.name);
        if member_name.is_empty() {
            continue;
        }

        let value = if member_data.initializer.is_none() {
            // Auto-increment
            let next_val = last_value.map_or(0, |v| v + 1);
            last_value = Some(next_val);
            resolved.push((member_name.clone(), ResolvedValue::Numeric(next_val)));
            EnumMemberValue::Auto(next_val)
        } else if is_string_literal(arena, member_data.initializer) {
            // String enum
            last_value = None; // Reset - can't continue after string
            if let Some(lit) = arena.get_literal_at(member_data.initializer) {
                let s = lit.text.clone();
                resolved.push((member_name.clone(), ResolvedValue::String(s.clone().into())));
                EnumMemberValue::String(s.into())
            } else {
                EnumMemberValue::Auto(0)
            }
        } else {
            // Try to evaluate with cross-reference resolution
            let value = extract_enum_value_with_resolve(
                arena,
                member_data.initializer,
                enum_name,
                &resolved,
            );
            match &value {
                EnumMemberValue::Auto(v) | EnumMemberValue::Numeric(v) => {
                    resolved.push((member_name.clone(), ResolvedValue::Numeric(*v)));
                }
                EnumMemberValue::String(s) => {
                    resolved.push((member_name.clone(), ResolvedValue::String(s.clone())));
                }
                EnumMemberValue::Computed(_) => {}
            }
            last_value = value.as_numeric().cloned();
            value
        };

        result.push(EnumMember {
            name: member_name.into(),
            value,
            leading_comment: None,
            trailing_comment: None,
        });
    }

    result
}

/// Look up a member name in the resolved values list.
fn resolve_member(resolved: &[(String, ResolvedValue)], name: &str) -> Option<EnumMemberValue> {
    for (n, v) in resolved {
        if n == name {
            return Some(match v {
                ResolvedValue::Numeric(val) => EnumMemberValue::Numeric(*val),
                ResolvedValue::String(s) => EnumMemberValue::String(s.clone()),
            });
        }
    }
    None
}

/// Try to evaluate an expression to a numeric value for use in binary operations.
fn try_eval_numeric(
    arena: &NodeArena,
    idx: NodeIndex,
    enum_name: &str,
    resolved: &[(String, ResolvedValue)],
) -> Option<i64> {
    match extract_enum_value_with_resolve(arena, idx, enum_name, resolved) {
        EnumMemberValue::Auto(v) | EnumMemberValue::Numeric(v) => Some(v),
        _ => None,
    }
}

/// Extract enum member value from an expression, resolving cross-references
/// to previously computed members of the same enum.
fn extract_enum_value_with_resolve(
    arena: &NodeArena,
    idx: NodeIndex,
    enum_name: &str,
    resolved: &[(String, ResolvedValue)],
) -> EnumMemberValue {
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
                EnumMemberValue::String(lit.text.clone().into())
            } else {
                EnumMemberValue::Auto(0)
            }
        }
        k if k == SyntaxKind::Identifier as u16 => {
            // Try to resolve as a reference to another member of the same enum
            let name = get_identifier_text(arena, idx).unwrap_or_default();
            if let Some(val) = resolve_member(resolved, &name) {
                return val;
            }
            EnumMemberValue::Computed(Box::new(IRNode::id(name)))
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            // Try to evaluate binary expressions with numeric operands
            if let Some(binary) = arena.get_binary_expr(node) {
                let left = try_eval_numeric(arena, binary.left, enum_name, resolved);
                let right = try_eval_numeric(arena, binary.right, enum_name, resolved);
                if let (Some(l), Some(r)) = (left, right) {
                    let result = match binary.operator_token {
                        t if t == SyntaxKind::BarToken as u16 => Some(l | r),
                        t if t == SyntaxKind::AmpersandToken as u16 => Some(l & r),
                        t if t == SyntaxKind::CaretToken as u16 => Some(l ^ r),
                        t if t == SyntaxKind::LessThanLessThanToken as u16 => Some(l << r),
                        t if t == SyntaxKind::GreaterThanGreaterThanToken as u16 => Some(l >> r),
                        t if t == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                            Some((l as u64 >> r as u64) as i64)
                        }
                        t if t == SyntaxKind::PlusToken as u16 => Some(l + r),
                        t if t == SyntaxKind::MinusToken as u16 => Some(l - r),
                        t if t == SyntaxKind::AsteriskToken as u16 => Some(l * r),
                        t if t == SyntaxKind::SlashToken as u16 => {
                            if r != 0 {
                                Some(l / r)
                            } else {
                                None
                            }
                        }
                        t if t == SyntaxKind::PercentToken as u16 => {
                            if r != 0 {
                                Some(l % r)
                            } else {
                                None
                            }
                        }
                        t if t == SyntaxKind::AsteriskAsteriskToken as u16 => Some(l.pow(r as u32)),
                        _ => None,
                    };
                    if let Some(val) = result {
                        return EnumMemberValue::Numeric(val);
                    }
                }
            }
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            // Try to evaluate unary expressions
            if let Some(unary) = arena.get_unary_expr(node)
                && let Some(operand) = try_eval_numeric(arena, unary.operand, enum_name, resolved)
            {
                let result = match unary.operator {
                    t if t == SyntaxKind::PlusToken as u16 => Some(operand),
                    t if t == SyntaxKind::MinusToken as u16 => Some(-operand),
                    t if t == SyntaxKind::TildeToken as u16 => Some(!operand),
                    _ => None,
                };
                if let Some(val) = result {
                    return EnumMemberValue::Numeric(val);
                }
            }
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            // Parenthesized expression - unwrap
            if let Some(paren) = arena.get_parenthesized(node) {
                extract_enum_value_with_resolve(arena, paren.expression, enum_name, resolved)
            } else {
                EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            // E.A reference - try to resolve if E is our enum
            if let Some(access) = arena.get_access_expr(node) {
                let obj_name = arena
                    .get(access.expression)
                    .and_then(|obj| arena.get_identifier(obj))
                    .map(|ident| ident.escaped_text.as_str());
                let prop_name = arena
                    .get(access.name_or_argument)
                    .and_then(|name| arena.get_identifier(name))
                    .map(|ident| ident.escaped_text.as_str());
                if let (Some(obj), Some(prop)) = (obj_name, prop_name)
                    && obj == enum_name
                    && let Some(val) = resolve_member(resolved, prop)
                {
                    return val;
                }
            }
            EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx)))
        }
        _ => EnumMemberValue::Computed(Box::new(IRNode::ASTRef(idx))),
    }
}

impl EnumMemberValue {
    const fn as_numeric(&self) -> Option<&i64> {
        match self {
            Self::Auto(v) | Self::Numeric(v) => Some(v),
            _ => None,
        }
    }
}

/// Check if a node is a string literal
fn is_string_literal(arena: &NodeArena, idx: NodeIndex) -> bool {
    if let Some(node) = arena.get(idx) {
        node.is_string_literal()
    } else {
        false
    }
}
