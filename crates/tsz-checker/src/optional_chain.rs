//! Optional Chaining Type Checking
//!
//! Provides AST-level utilities for detecting optional chaining expressions (`?.`).

use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

/// Checks if a node is an optional chain expression
pub fn is_optional_chain(arena: &NodeArena, idx: NodeIndex) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    match node.kind {
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                access.question_dot_token
            } else {
                false
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            // Check if this call is part of an optional chain.
            // A call can be optional in two ways:
            // 1. The callee itself is optional: `o?.b()` -> callee `o?.b` has question_dot_token
            // 2. The call has an optional token: `o.b?.()` -> call node has OPTIONAL_CHAIN flag
            if (node.flags as u32) & tsz_parser::parser::node_flags::OPTIONAL_CHAIN != 0 {
                return true;
            }
            if let Some(call) = arena.get_call_expr(node) {
                is_optional_chain(arena, call.expression)
            } else {
                false
            }
        }
        _ => false,
    }
}
