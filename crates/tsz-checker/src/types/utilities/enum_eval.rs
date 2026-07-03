//! Enum initializer evaluation and classification helpers.
//!
//! These methods remain inherent methods on `CheckerState`; this module only
//! owns the memo/depth state and expression walker used by regular enum member
//! values plus declaration-time enum initializer classification.
//! `const_enum_eval` stays separate because declaration checking calls it
//! without a `CheckerState`.

use crate::context::CheckerContext;
use crate::state::{CheckerState, EnumKind};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::numeric::{to_int32, to_uint32};
use tsz_parser::parser::node::{EnumData, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::cycle_guard::{self, CycleSetId};

// Thread-local memoization cache for evaluated enum member values.
//
// Caches successful and failed evaluation results to avoid redundant work when
// multiple enum members reference the same target in complex dependency graphs.
// Cleared at the start of each top-level evaluation via `EVAL_DEPTH`.
thread_local! {
    static EVAL_MEMO: std::cell::RefCell<rustc_hash::FxHashMap<NodeIndex, Option<f64>>>
        = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

// Thread-local recursion depth counter for `evaluate_constant_expression`.
//
// Prevents stack overflow from deeply nested (but non-cyclic) expression trees.
// The depth limit matches `evaluate_const_enum_initializer` (100).
thread_local! {
    static EVAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

const MAX_EVAL_DEPTH: u32 = 100;

/// Clear the enum evaluation memo cache and reset depth.
/// Called between compilation sessions to prevent stale `NodeIndex`-keyed results.
pub(crate) fn clear_enum_eval_memo() {
    EVAL_MEMO.with(|m| m.borrow_mut().clear());
    EVAL_DEPTH.with(|d| d.set(0));
}

// RAII guard that decrements `EVAL_DEPTH` on drop, and clears the memo cache
// when depth returns to 0 (end of top-level evaluation).
struct DepthGuard;
impl Drop for DepthGuard {
    fn drop(&mut self) {
        EVAL_DEPTH.with(|d| {
            let new_depth = d.get().saturating_sub(1);
            d.set(new_depth);
            if new_depth == 0 {
                // Clear memoization cache at the end of the top-level evaluation
                // to avoid stale results across unrelated evaluation chains.
                EVAL_MEMO.with(|m| m.borrow_mut().clear());
            }
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EnumMemberConstValue {
    Number(f64),
    String(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IsolatedEnumInitializerKind {
    LiteralNumeric,
    NonLiteralNumeric,
    LiteralString,
    NonLiteralString,
    Other,
}

pub(crate) fn is_numeric_constant_enum_expr(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
    enum_data: &EnumData,
) -> bool {
    is_numeric_constant_enum_expr_inner(ctx, expr_idx, enum_data, 0)
}

pub(crate) fn classify_isolated_enum_initializer(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
    enum_data: &EnumData,
) -> IsolatedEnumInitializerKind {
    classify_isolated_enum_initializer_inner(ctx, expr_idx, enum_data, 0)
}

pub(crate) fn isolated_decl_enum_initializer_is_computable(
    ctx: &CheckerContext<'_>,
    init_idx: NodeIndex,
    enum_idx: NodeIndex,
) -> bool {
    let mut seen_members = FxHashSet::default();
    isolated_decl_enum_value_is_computable(ctx, init_idx, enum_idx, &mut seen_members)
}

fn isolated_decl_enum_value_is_computable(
    ctx: &CheckerContext<'_>,
    init_idx: NodeIndex,
    enum_idx: NodeIndex,
    seen_members: &mut FxHashSet<NodeIndex>,
) -> bool {
    let Some(init_node) = ctx.arena.get(init_idx) else {
        return true;
    };
    match init_node.kind {
        k if k == SyntaxKind::NumericLiteral as u16 || k == SyntaxKind::StringLiteral as u16 => {
            true
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            ctx.arena.get_unary_expr(init_node).is_some_and(|unary| {
                isolated_decl_enum_value_is_computable(ctx, unary.operand, enum_idx, seen_members)
            })
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            ctx.arena.get_binary_expr(init_node).is_some_and(|bin| {
                isolated_decl_enum_value_is_computable(ctx, bin.left, enum_idx, seen_members)
                    && isolated_decl_enum_value_is_computable(
                        ctx,
                        bin.right,
                        enum_idx,
                        seen_members,
                    )
            })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            ctx.arena.get_parenthesized(init_node).is_some_and(|paren| {
                isolated_decl_enum_value_is_computable(
                    ctx,
                    paren.expression,
                    enum_idx,
                    seen_members,
                )
            })
        }
        // Runtime-computed enum members are allowed for isolated declarations.
        // TS9020 is about references to external symbols, not arbitrary computations.
        k if k == syntax_kind_ext::CALL_EXPRESSION => true,
        k if k == SyntaxKind::Identifier as u16 => {
            same_isolated_decl_enum_member_reference_is_computable(
                ctx,
                init_idx,
                enum_idx,
                seen_members,
            )
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
            let Some(access) = ctx.arena.get_access_expr(init_node) else {
                return false;
            };
            if !is_same_isolated_decl_enum_access(ctx, access.expression, enum_idx) {
                return false;
            }
            ctx.arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|member_ident| {
                    same_isolated_decl_enum_member_named_is_computable(
                        ctx,
                        &member_ident.escaped_text,
                        enum_idx,
                        seen_members,
                    )
                })
        }
        k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
            let Some(access) = ctx.arena.get_access_expr(init_node) else {
                return false;
            };
            if !is_same_isolated_decl_enum_access(ctx, access.expression, enum_idx) {
                return false;
            }
            ctx.arena
                .get_literal_at(access.name_or_argument)
                .is_some_and(|literal| {
                    same_isolated_decl_enum_member_named_is_computable(
                        ctx,
                        &literal.text,
                        enum_idx,
                        seen_members,
                    )
                })
        }
        _ => false,
    }
}

fn same_isolated_decl_enum_member_reference_is_computable(
    ctx: &CheckerContext<'_>,
    id_idx: NodeIndex,
    enum_idx: NodeIndex,
    seen_members: &mut FxHashSet<NodeIndex>,
) -> bool {
    let Some(ident) = ctx.arena.get_identifier_at(id_idx) else {
        return false;
    };
    same_isolated_decl_enum_member_named_is_computable(
        ctx,
        &ident.escaped_text,
        enum_idx,
        seen_members,
    )
}

fn same_isolated_decl_enum_member_named_is_computable(
    ctx: &CheckerContext<'_>,
    name: &str,
    enum_idx: NodeIndex,
    seen_members: &mut FxHashSet<NodeIndex>,
) -> bool {
    let Some(enum_node) = ctx.arena.get(enum_idx) else {
        return false;
    };
    let Some(enum_data) = ctx.arena.get_enum(enum_node) else {
        return false;
    };
    for &member_idx in &enum_data.members.nodes {
        if let Some(member_node) = ctx.arena.get(member_idx)
            && let Some(member) = ctx.arena.get_enum_member(member_node)
            && let Some(member_ident) = ctx.arena.get_identifier_at(member.name)
            && member_ident.escaped_text == name
        {
            if !seen_members.insert(member_idx) {
                return true;
            }

            let result = member.initializer.is_none()
                || isolated_decl_enum_value_is_computable(
                    ctx,
                    member.initializer,
                    enum_idx,
                    seen_members,
                );

            seen_members.remove(&member_idx);
            return result;
        }
    }
    false
}

fn is_same_isolated_decl_enum_access(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
    enum_idx: NodeIndex,
) -> bool {
    let Some(expr_node) = ctx.arena.get(expr_idx) else {
        return false;
    };
    if expr_node.kind != SyntaxKind::Identifier as u16 {
        return false;
    }
    let Some(ident) = ctx.arena.get_identifier_at(expr_idx) else {
        return false;
    };
    let Some(enum_node) = ctx.arena.get(enum_idx) else {
        return false;
    };
    let Some(enum_data) = ctx.arena.get_enum(enum_node) else {
        return false;
    };
    ctx.arena
        .get_identifier_at(enum_data.name)
        .is_some_and(|enum_ident| enum_ident.escaped_text == ident.escaped_text)
}

fn is_numeric_constant_enum_expr_inner(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
    enum_data: &EnumData,
    depth: u32,
) -> bool {
    if depth > MAX_EVAL_DEPTH {
        return false;
    }
    if expr_idx.is_none() {
        return true;
    }

    let Some(node) = ctx.arena.get(expr_idx) else {
        return false;
    };

    match node.kind {
        k if k == SyntaxKind::NumericLiteral as u16 => true,
        k if k == SyntaxKind::Identifier as u16 => {
            if let Some(name_text) = ctx.arena.get_identifier_text(expr_idx) {
                for &member_idx in &enum_data.members.nodes {
                    if let Some(member_node) = ctx.arena.get(member_idx)
                        && let Some(member_data) = ctx.arena.get_enum_member(member_node)
                        && let Some(member_name_text) =
                            ctx.arena.get_identifier_text(member_data.name)
                        && member_name_text == name_text
                    {
                        if member_data.initializer.is_none() {
                            return true;
                        }
                        return is_numeric_constant_enum_expr_inner(
                            ctx,
                            member_data.initializer,
                            enum_data,
                            depth + 1,
                        );
                    }
                }
                if matches!(name_text, "NaN" | "Infinity") {
                    return true;
                }
            }
            false
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            ctx.arena.get_unary_expr(node).is_some_and(|unary| {
                is_numeric_constant_enum_expr_inner(ctx, unary.operand, enum_data, depth + 1)
            })
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            ctx.arena.get_binary_expr(node).is_some_and(|binary| {
                is_numeric_constant_enum_expr_inner(ctx, binary.left, enum_data, depth + 1)
                    && is_numeric_constant_enum_expr_inner(ctx, binary.right, enum_data, depth + 1)
            })
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            ctx.arena.get_parenthesized(node).is_some_and(|paren| {
                is_numeric_constant_enum_expr_inner(ctx, paren.expression, enum_data, depth + 1)
            })
        }
        _ => false,
    }
}

fn classify_isolated_enum_initializer_inner(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
    enum_data: &EnumData,
    depth: u32,
) -> IsolatedEnumInitializerKind {
    if depth > MAX_EVAL_DEPTH || expr_idx.is_none() {
        return IsolatedEnumInitializerKind::Other;
    }

    let Some(node) = ctx.arena.get(expr_idx) else {
        return IsolatedEnumInitializerKind::Other;
    };

    if is_numeric_constant_enum_expr(ctx, expr_idx, enum_data) {
        return IsolatedEnumInitializerKind::LiteralNumeric;
    }

    match node.kind {
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
        {
            IsolatedEnumInitializerKind::LiteralString
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => ctx
            .arena
            .get_parenthesized(node)
            .map_or(IsolatedEnumInitializerKind::Other, |paren| {
                classify_isolated_enum_initializer_inner(
                    ctx,
                    paren.expression,
                    enum_data,
                    depth + 1,
                )
            }),
        k if k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION
            || k == syntax_kind_ext::TYPE_ASSERTION =>
        {
            ctx.arena.get_type_assertion(node).map_or(
                IsolatedEnumInitializerKind::Other,
                |assertion| {
                    classify_isolated_enum_initializer_inner(
                        ctx,
                        assertion.expression,
                        enum_data,
                        depth + 1,
                    )
                },
            )
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            ctx.arena
                .get_binary_expr(node)
                .map_or(IsolatedEnumInitializerKind::Other, |binary| {
                    if binary.operator_token == SyntaxKind::PlusToken as u16
                        && (is_syntactically_recognizable_string_initializer(
                            ctx.arena,
                            binary.left,
                        ) || is_syntactically_recognizable_string_initializer(
                            ctx.arena,
                            binary.right,
                        ))
                    {
                        IsolatedEnumInitializerKind::LiteralString
                    } else {
                        IsolatedEnumInitializerKind::Other
                    }
                })
        }
        k if k == SyntaxKind::Identifier as u16 => {
            let resolved = resolve_identifier_like_symbol(ctx, expr_idx)
                .and_then(|sym_id| resolve_imported_const_target(ctx, sym_id))
                .or_else(|| resolve_identifier_like_symbol(ctx, expr_idx));
            resolved.map_or(IsolatedEnumInitializerKind::Other, |sym_id| {
                classify_symbol_backed_enum_initializer(ctx, sym_id, enum_data, depth + 1)
            })
        }
        _ => IsolatedEnumInitializerKind::Other,
    }
}

fn resolve_identifier_like_symbol(
    ctx: &CheckerContext<'_>,
    expr_idx: NodeIndex,
) -> Option<SymbolId> {
    ctx.binder
        .get_node_symbol(expr_idx)
        .or_else(|| ctx.binder.resolve_identifier(ctx.arena, expr_idx))
}

fn resolve_imported_const_target(ctx: &CheckerContext<'_>, sym_id: SymbolId) -> Option<SymbolId> {
    let symbol = ctx.binder.get_symbol(sym_id)?;
    if !symbol.has_any_flags(symbol_flags::ALIAS) {
        return Some(sym_id);
    }
    let module_specifier = symbol.import_module()?;
    let target_name = symbol.import_name().unwrap_or(symbol.escaped_name.as_str());
    let source_file_idx = if symbol.decl_file_idx == u32::MAX {
        ctx.current_file_idx
    } else {
        symbol.decl_file_idx as usize
    };
    if !ctx.has_symbol_file_index(sym_id) {
        ctx.register_symbol_file_target(sym_id, source_file_idx);
    }
    ctx.resolve_alias_import_member(sym_id, module_specifier, target_name)
}

fn classify_symbol_backed_enum_initializer(
    ctx: &CheckerContext<'_>,
    sym_id: SymbolId,
    enum_data: &EnumData,
    depth: u32,
) -> IsolatedEnumInitializerKind {
    let cross_file_idx = ctx.resolve_symbol_file_index(sym_id);
    let is_cross_file = cross_file_idx.is_some_and(|idx| idx != ctx.current_file_idx);
    let (symbol, arena) = if let Some(file_idx) = cross_file_idx {
        let Some(binder) = ctx.get_binder_for_file(file_idx) else {
            return IsolatedEnumInitializerKind::Other;
        };
        let Some(symbol) = binder.get_symbol(sym_id) else {
            return IsolatedEnumInitializerKind::Other;
        };
        (symbol, ctx.get_arena_for_file(file_idx as u32))
    } else {
        let Some(symbol) = ctx.binder.get_symbol(sym_id) else {
            return IsolatedEnumInitializerKind::Other;
        };
        (symbol, ctx.arena)
    };

    let decl_idx = if symbol.value_declaration.is_none() {
        symbol
            .declarations
            .first()
            .copied()
            .unwrap_or(NodeIndex::NONE)
    } else {
        symbol.value_declaration
    };
    let Some(decl_node) = arena.get(decl_idx) else {
        return IsolatedEnumInitializerKind::Other;
    };
    let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
        return IsolatedEnumInitializerKind::Other;
    };
    if var_decl.initializer.is_none() {
        return declared_type_annotation_kind_in_arena(arena, var_decl.type_annotation)
            .unwrap_or(IsolatedEnumInitializerKind::Other);
    }
    if is_cross_file {
        match classify_initializer_kind_in_arena(arena, var_decl.initializer, depth) {
            IsolatedEnumInitializerKind::LiteralNumeric
            | IsolatedEnumInitializerKind::NonLiteralNumeric => {
                IsolatedEnumInitializerKind::NonLiteralNumeric
            }
            IsolatedEnumInitializerKind::LiteralString
            | IsolatedEnumInitializerKind::NonLiteralString => {
                IsolatedEnumInitializerKind::NonLiteralString
            }
            IsolatedEnumInitializerKind::Other => IsolatedEnumInitializerKind::Other,
        }
    } else {
        classify_isolated_enum_initializer_inner(ctx, var_decl.initializer, enum_data, depth)
    }
}

fn classify_initializer_kind_in_arena(
    arena: &NodeArena,
    expr_idx: NodeIndex,
    depth: u32,
) -> IsolatedEnumInitializerKind {
    if depth > MAX_EVAL_DEPTH || expr_idx.is_none() {
        return IsolatedEnumInitializerKind::Other;
    }

    let Some(node) = arena.get(expr_idx) else {
        return IsolatedEnumInitializerKind::Other;
    };

    match node.kind {
        k if k == SyntaxKind::NumericLiteral as u16 => IsolatedEnumInitializerKind::LiteralNumeric,
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
        {
            IsolatedEnumInitializerKind::LiteralString
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => arena
            .get_parenthesized(node)
            .map_or(IsolatedEnumInitializerKind::Other, |paren| {
                classify_initializer_kind_in_arena(arena, paren.expression, depth + 1)
            }),
        k if k == syntax_kind_ext::AS_EXPRESSION
            || k == syntax_kind_ext::SATISFIES_EXPRESSION
            || k == syntax_kind_ext::TYPE_ASSERTION =>
        {
            arena
                .get_type_assertion(node)
                .map_or(IsolatedEnumInitializerKind::Other, |assertion| {
                    classify_initializer_kind_in_arena(arena, assertion.expression, depth + 1)
                })
        }
        k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
            arena
                .get_unary_expr(node)
                .map_or(IsolatedEnumInitializerKind::Other, |unary| {
                    match classify_initializer_kind_in_arena(arena, unary.operand, depth + 1) {
                        IsolatedEnumInitializerKind::LiteralNumeric
                        | IsolatedEnumInitializerKind::NonLiteralNumeric => {
                            IsolatedEnumInitializerKind::NonLiteralNumeric
                        }
                        _ => IsolatedEnumInitializerKind::Other,
                    }
                })
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            arena
                .get_binary_expr(node)
                .map_or(IsolatedEnumInitializerKind::Other, |binary| {
                    if binary.operator_token == SyntaxKind::PlusToken as u16
                        && (is_syntactically_recognizable_string_initializer(arena, binary.left)
                            || is_syntactically_recognizable_string_initializer(
                                arena,
                                binary.right,
                            ))
                    {
                        IsolatedEnumInitializerKind::LiteralString
                    } else {
                        match (
                            classify_initializer_kind_in_arena(arena, binary.left, depth + 1),
                            classify_initializer_kind_in_arena(arena, binary.right, depth + 1),
                        ) {
                            (
                                IsolatedEnumInitializerKind::LiteralNumeric
                                | IsolatedEnumInitializerKind::NonLiteralNumeric,
                                IsolatedEnumInitializerKind::LiteralNumeric
                                | IsolatedEnumInitializerKind::NonLiteralNumeric,
                            ) => IsolatedEnumInitializerKind::NonLiteralNumeric,
                            _ => IsolatedEnumInitializerKind::Other,
                        }
                    }
                })
        }
        _ => IsolatedEnumInitializerKind::Other,
    }
}

fn declared_type_annotation_kind_in_arena(
    arena: &NodeArena,
    type_annotation: NodeIndex,
) -> Option<IsolatedEnumInitializerKind> {
    let type_node = arena.get(type_annotation)?;
    match type_node.kind {
        k if k == SyntaxKind::StringKeyword as u16 => {
            Some(IsolatedEnumInitializerKind::NonLiteralString)
        }
        k if k == SyntaxKind::NumberKeyword as u16 => {
            Some(IsolatedEnumInitializerKind::NonLiteralNumeric)
        }
        _ => None,
    }
}

fn is_syntactically_recognizable_string_initializer(
    arena: &NodeArena,
    expr_idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(expr_idx) else {
        return false;
    };
    match node.kind {
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
        {
            true
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            arena.get_parenthesized(node).is_some_and(|paren| {
                is_syntactically_recognizable_string_initializer(arena, paren.expression)
            })
        }
        _ => false,
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn enum_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let source_sym = self.enum_symbol_from_full_enum_type(source)?;
        let target_sym = self.enum_symbol_from_full_enum_type(target)?;

        if source_sym == target_sym {
            return None;
        }

        let source_name = self.ctx.binder.get_symbol(source_sym)?.escaped_name.clone();
        let target_name = self.ctx.binder.get_symbol(target_sym)?.escaped_name.clone();
        if source_name != target_name {
            return Some(false);
        }

        if self.is_const_enum_symbol(source_sym) || self.is_const_enum_symbol(target_sym) {
            return Some(false);
        }

        if self.enum_kind(source_sym) != Some(EnumKind::Numeric)
            || self.enum_kind(target_sym) != Some(EnumKind::Numeric)
        {
            return None;
        }

        let source_members = self.enum_member_compat_map(source_sym)?;
        let target_members = self.enum_member_compat_map(target_sym)?;
        let is_subset = source_members
            .iter()
            .all(|(name, value)| target_members.get(name) == Some(value));
        Some(is_subset)
    }

    fn enum_member_compat_map(
        &self,
        sym_id: SymbolId,
    ) -> Option<FxHashMap<String, EnumMemberConstValue>> {
        let mut result = FxHashMap::default();
        let mut failed = false;

        let saw_enum_decl =
            self.visit_enum_member_declared_const_values(sym_id, |_, member_name, value| {
                let (Some(member_name), Some(value)) = (member_name, value) else {
                    failed = true;
                    return true;
                };
                result.insert(member_name, value);
                false
            })?;

        if failed {
            return None;
        }

        saw_enum_decl.then_some(result)
    }

    fn visit_enum_member_declared_const_values(
        &self,
        enum_sym_id: SymbolId,
        mut visit: impl FnMut(NodeIndex, Option<String>, Option<EnumMemberConstValue>) -> bool,
    ) -> Option<bool> {
        let enum_symbol = self.ctx.binder.get_symbol(enum_sym_id)?;
        let mut saw_enum_decl = false;

        for &decl_idx in &enum_symbol.declarations {
            let enum_decl = self.ctx.arena.get_enum_at(decl_idx)?;
            saw_enum_decl = true;
            let mut auto_value = Some(0.0);

            for &member_idx in &enum_decl.members.nodes {
                let member_node = self.ctx.arena.get(member_idx)?;
                let member_data = self.ctx.arena.get_enum_member(member_node)?;
                let value = if let Some(initializer) = member_data.initializer.into_option() {
                    self.enum_member_initializer_const_value(initializer)
                } else {
                    auto_value.map(EnumMemberConstValue::Number)
                };

                let should_stop = visit(
                    member_idx,
                    self.get_property_name(member_data.name),
                    value.clone(),
                );

                if member_data.initializer.is_some() {
                    auto_value = match value {
                        Some(EnumMemberConstValue::Number(value)) => Some(value + 1.0),
                        Some(EnumMemberConstValue::String(_)) | None => None,
                    };
                } else {
                    auto_value = auto_value.map(|value| value + 1.0);
                }

                if should_stop {
                    return Some(saw_enum_decl);
                }
            }
        }

        Some(saw_enum_decl)
    }

    /// Get the literal type of an enum member from its initializer.
    ///
    /// Returns the literal type (e.g., Literal(0), Literal("a")) of the enum member.
    /// This is used to create `TypeData::Enum(member_def_id`, `literal_type`) for nominal typing.
    pub(crate) fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        // Get the member node
        let Some(member_node) = self.ctx.arena.get(member_decl) else {
            return TypeId::ERROR;
        };
        let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
            return TypeId::ERROR;
        };

        // Check if member has an explicit initializer.
        if let Some(initializer) = member.initializer.into_option() {
            match self.enum_member_initializer_const_value(initializer) {
                Some(EnumMemberConstValue::String(value)) => return factory.literal_string(&value),
                Some(EnumMemberConstValue::Number(value)) => return factory.literal_number(value),
                None => {}
            }
        }

        // No explicit initializer or computed value.
        // For numeric enums, recover the auto-incremented literal value so
        // callers can preserve member identity (`E.B` -> `0`).
        if let Some(member_sym) = self
            .ctx
            .binder
            .get_node_symbol(member_decl)
            .or_else(|| self.ctx.binder.get_node_symbol(member.name))
            && let Some(symbol) = self.ctx.binder.get_symbol(member_sym)
            && symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
            && symbol.parent.is_some()
            && let Some(auto_value) = self.compute_auto_increment_value(symbol.parent, member_decl)
        {
            return factory.literal_number(auto_value);
        }

        // Fall back to NUMBER type when we cannot compute a specific literal.
        TypeId::NUMBER
    }

    /// Evaluate a constant numeric expression (for enum member initializers).
    ///
    /// Handles: numeric literals, unary +/-/~, binary +/-/*/ // /%/|/&/^/<</>>/>>>,
    /// parenthesized expressions, and references to other enum members via
    /// property access (E.V1) or element access (E["V1"], E[`V1`]).
    /// Returns None if the expression cannot be evaluated at compile time.
    pub(crate) fn evaluate_constant_expression(&self, expr_idx: NodeIndex) -> Option<f64> {
        // Depth guard: prevent stack overflow from deeply nested expressions.
        let current_depth = EVAL_DEPTH.with(|d| {
            let depth = d.get() + 1;
            d.set(depth);
            depth
        });
        if current_depth > MAX_EVAL_DEPTH {
            EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return None;
        }
        let _depth_guard = DepthGuard;

        let node = self.ctx.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value
                    .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                match unary.operator {
                    op if op == SyntaxKind::MinusToken as u16 => Some(-operand),
                    op if op == SyntaxKind::PlusToken as u16 => Some(operand),
                    op if op == SyntaxKind::TildeToken as u16 => {
                        Some(f64::from(!to_int32(operand)))
                    }
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.ctx.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                match bin.operator_token {
                    op if op == SyntaxKind::PlusToken as u16 => Some(left + right),
                    op if op == SyntaxKind::MinusToken as u16 => Some(left - right),
                    op if op == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                    op if op == SyntaxKind::SlashToken as u16 => Some(left / right),
                    op if op == SyntaxKind::PercentToken as u16 => Some(left % right),
                    op if op == SyntaxKind::BarToken as u16 => {
                        Some(f64::from(to_int32(left) | to_int32(right)))
                    }
                    op if op == SyntaxKind::AmpersandToken as u16 => {
                        Some(f64::from(to_int32(left) & to_int32(right)))
                    }
                    op if op == SyntaxKind::CaretToken as u16 => {
                        Some(f64::from(to_int32(left) ^ to_int32(right)))
                    }
                    op if op == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(f64::from(to_int32(left) << (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(f64::from(to_int32(left) >> (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some(f64::from(to_uint32(left) >> (to_uint32(right) & 0x1f)))
                    }
                    op if op == SyntaxKind::AsteriskAsteriskToken as u16 => Some(left.powf(right)),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                self.evaluate_enum_member_access(expr_idx)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                // Bare identifier: resolve as enum member reference (e.g., `B = A` in same enum),
                // or recognize global numeric constants `NaN` and `Infinity`.
                let name = self.ctx.arena.get_identifier_text(expr_idx)?;
                match name {
                    "NaN" => return Some(f64::NAN),
                    "Infinity" => return Some(f64::INFINITY),
                    _ => {}
                }
                // Use the binder's scope-based resolution to find the symbol, since enum members
                // are bound in the enum's block scope, not in file_locals.
                let lib_binders = self.get_lib_binders();
                let sym_id = self.ctx.binder.resolve_name_with_filter(
                    name,
                    self.ctx.arena,
                    expr_idx,
                    &lib_binders,
                    |_| true,
                )?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                    let member_decl = symbol.value_declaration;

                    // Check memoization cache first.
                    if let Some(cached) = EVAL_MEMO.with(|m| m.borrow().get(&member_decl).copied())
                    {
                        return cached;
                    }

                    let member_node = self.ctx.arena.get(member_decl)?;
                    let member_data = self.ctx.arena.get_enum_member(member_node)?;

                    // Cycle detection via shared CycleGuard
                    let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;

                    let result = if member_data.initializer.is_some() {
                        self.evaluate_constant_expression(member_data.initializer)
                    } else {
                        // Auto-incremented: find parent enum symbol and compute.
                        let parent_sym = symbol.parent;
                        if parent_sym.is_none() {
                            return None;
                        }
                        self.compute_auto_increment_value(parent_sym, member_decl)
                    };

                    // Cache the result.
                    EVAL_MEMO.with(|m| m.borrow_mut().insert(member_decl, result));
                    result
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Model whether tsc's `evaluate()` would succeed for a non-const enum
    /// member initializer before TS18033 falls back to type assignability.
    pub(crate) fn enum_initializer_evaluation_status(&self, expr_idx: NodeIndex) -> Option<bool> {
        if expr_idx.is_none() {
            return Some(false);
        }
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return Some(false);
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                Some(true)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                self.identifier_evaluates_as_enum_initializer(expr_idx)
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                let Some(tmpl) = self.ctx.arena.get_template_expr(node) else {
                    return Some(false);
                };

                let mut result = Some(true);
                for &span_idx in &tmpl.template_spans.nodes {
                    let Some(span_node) = self.ctx.arena.get(span_idx) else {
                        return Some(false);
                    };
                    let Some(span_data) = self.ctx.arena.get_template_span(span_node) else {
                        return Some(false);
                    };
                    match self.enum_initializer_evaluation_status(span_data.expression) {
                        Some(false) => return Some(false),
                        None => result = None,
                        Some(true) => {}
                    }
                }
                result
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                None
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                    return Some(false);
                };
                let left = self.enum_initializer_evaluation_status(binary.left);
                let right = self.enum_initializer_evaluation_status(binary.right);
                match (left, right) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
                    return Some(false);
                };
                self.enum_initializer_evaluation_status(unary.operand)
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                    return Some(false);
                };
                self.enum_initializer_evaluation_status(paren.expression)
            }
            _ => Some(false),
        }
    }

    fn identifier_evaluates_as_enum_initializer(&self, ident_idx: NodeIndex) -> Option<bool> {
        let Some(sym_id) = self.resolve_identifier_symbol(ident_idx) else {
            return Some(false);
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return Some(false);
        };

        if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return Some(true);
        }

        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return None;
        }

        let decl_node = self.ctx.arena.get(value_decl)?;
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        if !self.ctx.arena.is_const_variable_declaration(value_decl) {
            return Some(false);
        }

        let Some(var_data) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return Some(false);
        };
        let init = var_data.initializer;
        if init.is_none() {
            return Some(false);
        }

        self.enum_initializer_evaluation_status(init)
    }

    /// Resolve a property access or element access expression that references
    /// an enum member, and evaluate its numeric value.
    ///
    /// Handles patterns like:
    /// - `E.V1` (property access on enum)
    /// - `A.B.C.E.V1` (qualified namespace chain)
    /// - `E["V1"]` (element access with string literal)
    /// - `E[`V1`]` (element access with template literal)
    fn evaluate_enum_member_access(&self, expr_idx: NodeIndex) -> Option<f64> {
        // Collect the chain of identifiers and the final member name.
        // For `A.B.C.E.V1`: segments = ["A", "B", "C", "E"], member_name = "V1"
        let (segments, member_name) = self.collect_access_chain(expr_idx)?;
        if segments.is_empty() {
            return None;
        }

        // Walk the binder's symbol table to find the enum symbol.
        let root_name = &segments[0];
        let mut current_sym_id = self.ctx.binder.file_locals.get(root_name)?;

        for segment in &segments[1..] {
            let symbol = self.ctx.binder.get_symbol(current_sym_id)?;
            // Try exports first (for namespaces), then members (for enums/classes)
            current_sym_id = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })?;
        }

        // `current_sym_id` should now point to the enum symbol.
        // Look up the member in its exports (enum members are stored as exports).
        let enum_symbol = self.ctx.binder.get_symbol(current_sym_id)?;
        let member_sym_id = enum_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(&member_name))
            .or_else(|| {
                enum_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(&member_name))
            })?;

        let member_symbol = self.ctx.binder.get_symbol(member_sym_id)?;
        if !member_symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            return None;
        }

        // Get the member's value declaration and evaluate its initializer.
        let member_decl = member_symbol.value_declaration;

        // Check memoization cache first: if we've already evaluated this member
        // (successfully or not), return the cached result immediately.
        if let Some(cached) = EVAL_MEMO.with(|m| m.borrow().get(&member_decl).copied()) {
            return cached;
        }

        let member_node = self.ctx.arena.get(member_decl)?;
        let member_data = self.ctx.arena.get_enum_member(member_node)?;

        let result = if member_data.initializer.is_none() {
            // Auto-incremented member: we need to compute its position value.
            // Walk through all declarations of the parent enum to find this member's
            // auto-incremented value.
            //
            // Guard against cycles through auto-increment:
            // e.g., `enum E { A = F.C }; enum F { B = E.A, C }`
            // E.A -> F.C (auto-inc) -> compute_auto_inc walks F.B -> E.A -> F.C -> ...
            let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;
            self.compute_auto_increment_value(current_sym_id, member_decl)
        } else {
            // Guard against self-referencing and mutually-recursive enum initializers
            // (e.g., `B = E.B` or `enum E { A = F.B }; enum F { B = E.A }`).
            let _guard = cycle_guard::try_enter(member_decl, CycleSetId::NonConstEnum)?;
            self.evaluate_constant_expression(member_data.initializer)
        };

        // Cache the result for future lookups of the same member.
        EVAL_MEMO.with(|m| m.borrow_mut().insert(member_decl, result));
        result
    }

    /// Collect the identifier chain from a property/element access expression.
    /// Returns `(object_segments, member_name)`.
    /// For `A.B.C.E.V1`: `(["A", "B", "C", "E"], "V1")`
    /// For `E["V1"]`: (["E"], "V1")
    fn collect_access_chain(&self, expr_idx: NodeIndex) -> Option<(Vec<String>, String)> {
        let node = self.ctx.arena.get(expr_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let prop_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = self
                    .ctx
                    .arena
                    .get_identifier(prop_node)?
                    .escaped_text
                    .clone();

                // Recursively collect the object chain.
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                // Get the string key from the element access argument.
                let arg_node = self.ctx.arena.get(access.name_or_argument)?;
                let member_name = if arg_node.kind == SyntaxKind::StringLiteral as u16
                    || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                {
                    self.ctx.arena.get_literal(arg_node)?.text.clone()
                } else {
                    return None;
                };

                // Collect the object chain.
                let obj_node = self.ctx.arena.get(access.expression)?;
                if obj_node.kind == SyntaxKind::Identifier as u16 {
                    let ident = self.ctx.arena.get_identifier(obj_node)?;
                    Some((vec![ident.escaped_text.clone()], member_name))
                } else if obj_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    let (mut segments, last_segment) =
                        self.collect_access_chain(access.expression)?;
                    segments.push(last_segment);
                    Some((segments, member_name))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn enum_member_declared_const_value(
        &self,
        enum_sym_id: SymbolId,
        target_member_decl: NodeIndex,
    ) -> Option<EnumMemberConstValue> {
        let mut found = false;
        let mut result = None;
        self.visit_enum_member_declared_const_values(enum_sym_id, |member_idx, _, value| {
            if member_idx == target_member_decl {
                found = true;
                result = value;
                true
            } else {
                false
            }
        })?;

        found.then_some(result).flatten()
    }

    pub(crate) fn enum_member_initializer_const_value(
        &self,
        initializer: NodeIndex,
    ) -> Option<EnumMemberConstValue> {
        let init_node = self.ctx.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(init_node)?;
                Some(EnumMemberConstValue::String(lit.text.clone()))
            }
            _ => self
                .evaluate_constant_expression(initializer)
                .map(EnumMemberConstValue::Number),
        }
    }

    /// Compute the auto-incremented value for an enum member without an initializer.
    /// Walks through all declarations of the parent enum up to the target member,
    /// tracking the auto-increment counter.
    fn compute_auto_increment_value(
        &self,
        enum_sym_id: tsz_binder::SymbolId,
        target_member_decl: NodeIndex,
    ) -> Option<f64> {
        match self.enum_member_declared_const_value(enum_sym_id, target_member_decl)? {
            EnumMemberConstValue::Number(value) => Some(value),
            EnumMemberConstValue::String(_) => None,
        }
    }
}
