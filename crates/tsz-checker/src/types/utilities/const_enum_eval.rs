//! Standalone functions for evaluating const enum member initializers.
//!
//! These are free functions (not methods on `CheckerState`) so they can be called
//! from both `CheckerState` and `DeclarationChecker` contexts.
//!
//! Cycle detection: A thread-local visited set (`CONST_EVAL_VISITED`) tracks which
//! enum member declarations are currently being evaluated.  This detects both direct
//! self-references (`A = E.A`) and mutual recursion across enums
//! (`enum E { A = F.B }; enum F { B = E.A }`).  A drop guard ensures cleanup even
//! on panic.

use super::cycle_guard::{self, CycleSetId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// Memoization cache for const enum member evaluation results.
// Avoids redundant evaluation when multiple members reference the same target.
thread_local! {
    static CONST_EVAL_MEMO: std::cell::RefCell<rustc_hash::FxHashMap<NodeIndex, Option<f64>>>
        = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

/// Clear the const enum evaluation memo cache.
/// Should be called after completing a top-level evaluation batch
/// (e.g., after checking all members of an enum declaration).
pub(crate) fn clear_const_eval_memo() {
    CONST_EVAL_MEMO.with(|m| m.borrow_mut().clear());
}

/// Evaluate a const enum member's initializer value, resolving references to other members.
///
/// This is a standalone function (not a method) so it can be called from both
/// `CheckerState` and `DeclarationChecker` contexts.
///
/// Handles: numeric literals, bare identifiers (enum member refs), property access
/// on enum (`E.A`), element access with string literal (`E["A"]`), unary/binary ops,
/// and parenthesized expressions.
pub(crate) fn evaluate_const_enum_initializer(
    arena: &tsz_parser::parser::NodeArena,
    expr_idx: NodeIndex,
    enum_data: &tsz_parser::parser::node::EnumData,
    enum_name: Option<&str>,
    depth: u32,
) -> Option<f64> {
    if depth > 100 {
        return None;
    }
    let node = arena.get(expr_idx)?;

    match node.kind {
        k if k == SyntaxKind::NumericLiteral as u16 => {
            let lit = arena.get_literal(node)?;
            lit.value.or_else(|| lit.text.parse::<f64>().ok())
        }
        k if k == SyntaxKind::Identifier as u16 => {
            let name = arena.get_identifier_text(expr_idx)?;
            // Recognize global numeric constants NaN and Infinity
            match name {
                "NaN" => return Some(f64::NAN),
                "Infinity" => return Some(f64::INFINITY),
                _ => {}
            }
            resolve_enum_member_value(arena, name, enum_data, depth)
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            let prop = arena.get_access_expr(node)?;
            if expression_ends_with_identifier(arena, prop.expression, enum_name) {
                let member_name = arena.get_identifier_text(prop.name_or_argument)?;
                return resolve_enum_member_value(arena, member_name, enum_data, depth);
            }
            // Try resolving as a reference to a different enum (e.g., OtherEnum.Member)
            resolve_cross_enum_property_access(arena, enum_data, prop, depth)
        }
        k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
            let elem = arena.get_access_expr(node)?;
            if expression_ends_with_identifier(arena, elem.expression, enum_name) {
                let arg_node = arena.get(elem.name_or_argument)?;
                if arg_node.kind == SyntaxKind::StringLiteral as u16
                    || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    let lit = arena.get_literal(arg_node)?;
                    return resolve_enum_member_value(arena, &lit.text, enum_data, depth);
                }
            }
            // Try resolving as a reference to a different enum (e.g., OtherEnum["Member"])
            resolve_cross_enum_element_access(arena, enum_data, elem, depth)
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            let unary = arena.get_unary_expr(node)?;
            let operand = evaluate_const_enum_initializer(
                arena,
                unary.operand,
                enum_data,
                enum_name,
                depth + 1,
            )?;
            match unary.operator {
                op if op == SyntaxKind::MinusToken as u16 => Some(-operand),
                op if op == SyntaxKind::PlusToken as u16 => Some(operand),
                op if op == SyntaxKind::TildeToken as u16 => Some(!(operand as i32) as f64),
                _ => None,
            }
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            let bin = arena.get_binary_expr(node)?;
            let left =
                evaluate_const_enum_initializer(arena, bin.left, enum_data, enum_name, depth + 1)?;
            let right =
                evaluate_const_enum_initializer(arena, bin.right, enum_data, enum_name, depth + 1)?;
            match bin.operator_token {
                op if op == SyntaxKind::PlusToken as u16 => Some(left + right),
                op if op == SyntaxKind::MinusToken as u16 => Some(left - right),
                op if op == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                op if op == SyntaxKind::SlashToken as u16 => Some(left / right),
                op if op == SyntaxKind::PercentToken as u16 => Some(left % right),
                op if op == SyntaxKind::BarToken as u16 => {
                    Some((left as i32 | right as i32) as f64)
                }
                op if op == SyntaxKind::AmpersandToken as u16 => {
                    Some((left as i32 & right as i32) as f64)
                }
                op if op == SyntaxKind::CaretToken as u16 => {
                    Some((left as i32 ^ right as i32) as f64)
                }
                op if op == SyntaxKind::LessThanLessThanToken as u16 => {
                    Some(((left as i32) << (right as u32 & 0x1f)) as f64)
                }
                op if op == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                    Some(((left as i32) >> (right as u32 & 0x1f)) as f64)
                }
                op if op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                    Some(((left as u32) >> (right as u32 & 0x1f)) as f64)
                }
                op if op == SyntaxKind::AsteriskAsteriskToken as u16 => Some(left.powf(right)),
                _ => None,
            }
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            let paren = arena.get_parenthesized(node)?;
            evaluate_const_enum_initializer(
                arena,
                paren.expression,
                enum_data,
                enum_name,
                depth + 1,
            )
        }
        _ => None,
    }
}

fn expression_ends_with_identifier(
    arena: &tsz_parser::parser::NodeArena,
    expr_idx: NodeIndex,
    expected: Option<&str>,
) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    let Some(node) = arena.get(expr_idx) else {
        return false;
    };

    match node.kind {
        k if k == SyntaxKind::Identifier as u16 => arena
            .get_identifier_text(expr_idx)
            .is_some_and(|name| name == expected),
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => arena
            .get_access_expr(node)
            .and_then(|access| arena.get_identifier_text(access.name_or_argument))
            .is_some_and(|name| name == expected),
        _ => false,
    }
}

/// Resolve a cross-enum property access like `OtherEnum.Member` in a const enum initializer.
///
/// When a const enum member initializer references a member of a different enum
/// (e.g., `const enum A { X = B.Y }`), we need to find enum `B` in the AST and
/// evaluate `Y` in that enum's context.
fn resolve_cross_enum_property_access(
    arena: &tsz_parser::parser::NodeArena,
    current_enum_data: &tsz_parser::parser::node::EnumData,
    prop: &tsz_parser::parser::node::AccessExprData,
    depth: u32,
) -> Option<f64> {
    // The expression should be an identifier (the other enum's name)
    let expr_node = arena.get(prop.expression)?;
    if expr_node.kind != SyntaxKind::Identifier as u16 {
        return None;
    }
    let other_enum_name = arena.get_identifier_text(prop.expression)?;
    let member_name = arena.get_identifier_text(prop.name_or_argument)?;

    resolve_external_enum_member(
        arena,
        current_enum_data,
        other_enum_name,
        member_name,
        depth,
    )
}

/// Resolve a cross-enum element access like `OtherEnum["Member"]` in a const enum initializer.
fn resolve_cross_enum_element_access(
    arena: &tsz_parser::parser::NodeArena,
    current_enum_data: &tsz_parser::parser::node::EnumData,
    elem: &tsz_parser::parser::node::AccessExprData,
    depth: u32,
) -> Option<f64> {
    let expr_node = arena.get(elem.expression)?;
    if expr_node.kind != SyntaxKind::Identifier as u16 {
        return None;
    }
    let other_enum_name = arena.get_identifier_text(elem.expression)?;

    let arg_node = arena.get(elem.name_or_argument)?;
    if arg_node.kind != SyntaxKind::StringLiteral as u16
        && arg_node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
    {
        return None;
    }
    let lit = arena.get_literal(arg_node)?;
    let member_name = &lit.text;

    resolve_external_enum_member(
        arena,
        current_enum_data,
        other_enum_name,
        member_name,
        depth,
    )
}

/// Find an enum declaration by name in the same file and evaluate one of its members.
///
/// Uses `CONST_EVAL_VISITED` to detect cycles across mutually-recursive enums
/// (e.g., `const enum E { A = F.B }; const enum F { B = E.A }`).
fn resolve_external_enum_member(
    arena: &tsz_parser::parser::NodeArena,
    current_enum_data: &tsz_parser::parser::node::EnumData,
    target_enum_name: &str,
    member_name: &str,
    depth: u32,
) -> Option<f64> {
    let current_enum_decl_idx = arena.get_extended(current_enum_data.name)?.parent;
    let namespace_path = enum_namespace_path(arena, current_enum_decl_idx);
    let source_file_idx = source_file_ancestor(arena, current_enum_decl_idx)?;

    // Search the AST for an enum with the target name
    let mut stack = vec![source_file_idx];
    while let Some(node_idx) = stack.pop() {
        if let Some(candidate_enum) = arena.get_enum_at(node_idx)
            && arena.get_identifier_text(candidate_enum.name) == Some(target_enum_name)
            && enum_namespace_path(arena, node_idx) == namespace_path
        {
            // Found the target enum — look up the member
            if let Some(member_idx) = enum_member_index(arena, candidate_enum, member_name) {
                let m_idx = candidate_enum.members.nodes[member_idx];

                // Check memoization cache first.
                if let Some(cached) = CONST_EVAL_MEMO.with(|m| m.borrow().get(&m_idx).copied()) {
                    return cached;
                }

                let m_node = arena.get(m_idx)?;
                let m_data = arena.get_enum_member(m_node)?;

                // Cycle detection: if we're already evaluating this member, bail out.
                let _guard = cycle_guard::try_enter(m_idx, CycleSetId::ConstEnum)?;

                let result = if m_data.initializer.is_some() {
                    evaluate_const_enum_initializer(
                        arena,
                        m_data.initializer,
                        candidate_enum,
                        Some(target_enum_name),
                        depth + 1,
                    )
                } else {
                    // Auto-incremented: find base and add offset
                    let mut auto_result = Some(member_idx as f64);
                    let mut offset = 1u32;
                    for i in (0..member_idx).rev() {
                        let prev_idx = candidate_enum.members.nodes[i];
                        let prev_node = arena.get(prev_idx)?;
                        let prev_data = arena.get_enum_member(prev_node)?;
                        if prev_data.initializer.is_some() {
                            auto_result = evaluate_const_enum_initializer(
                                arena,
                                prev_data.initializer,
                                candidate_enum,
                                Some(target_enum_name),
                                depth + 1,
                            )
                            .map(|base| base + offset as f64);
                            break;
                        }
                        offset += 1;
                    }
                    auto_result
                };

                // Cache the result.
                CONST_EVAL_MEMO.with(|m| m.borrow_mut().insert(m_idx, result));
                return result;
            }
        }
        for child_idx in arena.get_children(node_idx) {
            stack.push(child_idx);
        }
    }

    None
}

/// Resolve a member name to its computed value within an enum.
///
/// For members with explicit initializers, evaluates the initializer directly.
/// For auto-incremented members, finds the nearest prior explicit initializer,
/// evaluates it, then adds the offset.
///
/// Uses the shared `CycleGuard` to detect self-referencing initializers
/// (e.g., `const enum E { A = A }`).
fn resolve_enum_member_value(
    arena: &tsz_parser::parser::NodeArena,
    name: &str,
    enum_data: &tsz_parser::parser::node::EnumData,
    depth: u32,
) -> Option<f64> {
    let enum_name = arena.get_identifier_text(enum_data.name)?;
    let (target_enum_decl_idx, target_idx) =
        find_enum_member_decl(arena, enum_data, enum_name, name)?;
    let target_enum_data = arena.get_enum_at(target_enum_decl_idx)?;
    let m_idx = target_enum_data.members.nodes[target_idx];

    // Check memoization cache first.
    if let Some(cached) = CONST_EVAL_MEMO.with(|m| m.borrow().get(&m_idx).copied()) {
        return cached;
    }

    let m_node = arena.get(m_idx)?;
    let m_data = arena.get_enum_member(m_node)?;

    // Cycle detection: if we're already evaluating this member, bail out.
    let _guard = cycle_guard::try_enter(m_idx, CycleSetId::ConstEnum)?;

    let result = if m_data.initializer.is_some() {
        // Explicit initializer — evaluate it directly
        evaluate_const_enum_initializer(
            arena,
            m_data.initializer,
            target_enum_data,
            Some(enum_name),
            depth + 1,
        )
    } else {
        // Auto-incremented member: find the nearest prior member with an initializer
        // and count the offset
        let mut auto_result = Some(target_idx as f64);
        let mut offset = 1u32;
        for i in (0..target_idx).rev() {
            let prev_idx = target_enum_data.members.nodes[i];
            let prev_node = arena.get(prev_idx)?;
            let prev_data = arena.get_enum_member(prev_node)?;
            if prev_data.initializer.is_some() {
                auto_result = evaluate_const_enum_initializer(
                    arena,
                    prev_data.initializer,
                    target_enum_data,
                    Some(enum_name),
                    depth + 1,
                )
                .map(|base| base + offset as f64);
                break;
            }
            offset += 1;
        }
        auto_result
    };

    // Cache the result.
    CONST_EVAL_MEMO.with(|m| m.borrow_mut().insert(m_idx, result));
    result
}

fn find_enum_member_decl(
    arena: &tsz_parser::parser::NodeArena,
    enum_data: &tsz_parser::parser::node::EnumData,
    enum_name: &str,
    member_name: &str,
) -> Option<(NodeIndex, usize)> {
    let current_enum_decl_idx = arena.get_extended(enum_data.name)?.parent;

    if let Some(target_idx) = enum_member_index(arena, enum_data, member_name) {
        return Some((current_enum_decl_idx, target_idx));
    }

    let namespace_path = enum_namespace_path(arena, current_enum_decl_idx);
    let source_file_idx = source_file_ancestor(arena, current_enum_decl_idx)?;
    let mut stack = vec![source_file_idx];
    while let Some(node_idx) = stack.pop() {
        if node_idx != current_enum_decl_idx
            && let Some(candidate_enum) = arena.get_enum_at(node_idx)
            && arena.get_identifier_text(candidate_enum.name) == Some(enum_name)
            && enum_namespace_path(arena, node_idx) == namespace_path
            && let Some(target_idx) = enum_member_index(arena, candidate_enum, member_name)
        {
            return Some((node_idx, target_idx));
        }
        for child_idx in arena.get_children(node_idx) {
            stack.push(child_idx);
        }
    }

    None
}

fn source_file_ancestor(
    arena: &tsz_parser::parser::NodeArena,
    mut node_idx: NodeIndex,
) -> Option<NodeIndex> {
    loop {
        let node = arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::SOURCE_FILE {
            return Some(node_idx);
        }
        node_idx = arena.get_extended(node_idx)?.parent;
    }
}

fn enum_namespace_path(
    arena: &tsz_parser::parser::NodeArena,
    mut enum_decl_idx: NodeIndex,
) -> Vec<String> {
    let mut path = Vec::new();
    while let Some(parent_idx) = arena.get_extended(enum_decl_idx).map(|ext| ext.parent) {
        enum_decl_idx = parent_idx;
        let Some(module_decl) = arena.get_module_at(enum_decl_idx) else {
            continue;
        };
        if let Some(name) = arena.get_identifier_text(module_decl.name) {
            path.push(name.to_string());
        }
    }
    path.reverse();
    path
}

fn enum_member_index(
    arena: &tsz_parser::parser::NodeArena,
    enum_data: &tsz_parser::parser::node::EnumData,
    member_name: &str,
) -> Option<usize> {
    enum_data.members.nodes.iter().position(|&m_idx| {
        arena
            .get(m_idx)
            .and_then(|m_node| arena.get_enum_member(m_node))
            .and_then(|m_data| arena.get_identifier_text(m_data.name))
            .is_some_and(|m_name| m_name == member_name)
    })
}
