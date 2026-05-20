//! Stateless helpers for `narrowing.rs`.
//!
//! Maps AST kinds and primitive identifier names to `TypeId` intrinsics, plus
//! resolution of a const variable's type annotation to a primitive/object-like intrinsic.
//! Lifted out of `narrowing.rs` to keep that file under the 2000-LOC ceiling.

use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::node::{NodeArena, VariableDeclarationData};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use crate::query_boundaries::common::QueryDatabase;
use crate::query_boundaries::flow_analysis as flow_query;

const fn primitive_keyword_intrinsic(kind: u16) -> Option<TypeId> {
    match kind {
        k if k == SyntaxKind::NullKeyword as u16 => Some(TypeId::NULL),
        k if k == SyntaxKind::UndefinedKeyword as u16 => Some(TypeId::UNDEFINED),
        k if k == SyntaxKind::StringKeyword as u16 => Some(TypeId::STRING),
        k if k == SyntaxKind::NumberKeyword as u16 => Some(TypeId::NUMBER),
        k if k == SyntaxKind::BooleanKeyword as u16 => Some(TypeId::BOOLEAN),
        k if k == SyntaxKind::BigIntKeyword as u16 => Some(TypeId::BIGINT),
        k if k == SyntaxKind::SymbolKeyword as u16 => Some(TypeId::SYMBOL),
        k if k == SyntaxKind::ObjectKeyword as u16 => Some(TypeId::OBJECT),
        _ => None,
    }
}

fn primitive_name_intrinsic(name: &str) -> Option<TypeId> {
    match name {
        "string" => Some(TypeId::STRING),
        "number" => Some(TypeId::NUMBER),
        "boolean" => Some(TypeId::BOOLEAN),
        "bigint" => Some(TypeId::BIGINT),
        "symbol" => Some(TypeId::SYMBOL),
        "object" => Some(TypeId::OBJECT),
        _ => None,
    }
}

/// Returns true when the identifier at `idx` spells `undefined` AND resolves
/// to the global `undefined` (a lib symbol or unresolved). A user-declared
/// local named `undefined` (parameter, variable, etc.) shadows the global
/// and must not be treated as the literal `undefined` sentinel.
pub(super) fn is_global_undefined_identifier(
    arena: &NodeArena,
    binder: &BinderState,
    idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };
    let Some(ident) = arena.get_identifier(node) else {
        return false;
    };
    if ident.escaped_text != "undefined" {
        return false;
    }
    if let Some(sym_id) = binder
        .get_node_symbol(idx)
        .or_else(|| binder.resolve_identifier(arena, idx))
        && !binder.lib_symbol_ids.contains(&sym_id)
    {
        return false;
    }
    true
}

/// Resolve a const variable's `type_annotation` node to a primitive/object-like
/// `TypeId` when the annotation can act as an `unknown` equality comparand.
pub(super) fn const_annotation_intrinsic_type(
    arena: &NodeArena,
    ann_idx: NodeIndex,
) -> Option<TypeId> {
    let ann_node = arena.get(ann_idx)?;
    if let Some(intrinsic) = primitive_keyword_intrinsic(ann_node.kind) {
        return Some(intrinsic);
    }
    if matches!(
        ann_node.kind,
        k if k == tsz_parser::parser::syntax_kind_ext::TYPE_LITERAL
            || k == tsz_parser::parser::syntax_kind_ext::FUNCTION_TYPE
            || k == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR_TYPE
    ) {
        return Some(TypeId::OBJECT);
    }
    if ann_node.kind != tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE {
        return None;
    }
    let ty_ref = arena.get_type_ref(ann_node)?;
    if ty_ref
        .type_arguments
        .as_ref()
        .is_some_and(|args| !args.nodes.is_empty())
    {
        return None;
    }
    let name_node = arena.get(ty_ref.type_name)?;
    let ident = arena.get_identifier(name_node)?;
    primitive_name_intrinsic(ident.escaped_text.as_str())
}

/// Returns true if the callable type's return type is exclusively `false` or
/// `never`. Used to validate non-predicate members in a union of callables:
/// tsc permits a union to act as a type guard only when non-predicate members
/// can never return a truthy value.
pub(super) fn callable_returns_only_false_or_never(
    interner: &dyn QueryDatabase,
    callable_type: TypeId,
) -> bool {
    flow_query::function_return_type(interner, callable_type)
        .is_some_and(|rt| flow_query::is_only_false_or_never(interner, rt))
}

/// Return the `VariableDeclarationData` for a const block-scoped variable
/// identified by `sym_id`, or `None` for any other symbol shape.
///
/// `value_declaration` may point at the declaration's name identifier (for
/// destructuring aliases); this helper normalizes to the
/// `VARIABLE_DECLARATION` parent and verifies the `const` modifier.
pub(super) fn block_scoped_const_var_decl<'a>(
    arena: &'a NodeArena,
    binder: &BinderState,
    sym_id: SymbolId,
) -> Option<&'a VariableDeclarationData> {
    let symbol = binder.get_symbol(sym_id)?;
    if !symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE) {
        return None;
    }
    let mut decl_id = symbol.value_declaration;
    if let Some(decl_node_check) = arena.get(decl_id)
        && decl_node_check.kind == SyntaxKind::Identifier as u16
        && let Some(ext) = arena.get_extended(decl_id)
        && ext.parent.is_some()
        && let Some(parent_node) = arena.get(ext.parent)
        && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
    {
        decl_id = ext.parent;
    }
    if !arena.is_const_variable_declaration(decl_id) {
        return None;
    }
    arena.get_variable_declaration_at(decl_id)
}

/// Resolve a `unique symbol`-annotated const binding to its singleton type.
///
/// `const sym: unique symbol = Symbol()` binds `sym` to
/// `UniqueSymbol(SymbolRef(sym))`. Property types written `typeof sym`
/// resolve to the same singleton, so the RHS of `x === sym` is a valid
/// discriminant value at narrowing time even though the initializer
/// (`Symbol()`) is not a literal expression.
pub(super) fn unique_symbol_const_decl_type(
    arena: &NodeArena,
    binder: &BinderState,
    interner: &dyn QueryDatabase,
    sym_id: SymbolId,
) -> Option<TypeId> {
    let decl_data = block_scoped_const_var_decl(arena, binder, sym_id)?;
    if !crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped(
        arena,
        decl_data.type_annotation,
    ) {
        return None;
    }
    Some(interner.unique_symbol(tsz_solver::SymbolRef(sym_id.0)))
}
