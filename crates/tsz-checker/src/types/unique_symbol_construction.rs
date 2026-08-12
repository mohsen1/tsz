//! Construction of `unique symbol` types for `unique symbol` type-operator
//! annotations, shared by the two `get_type_from_type_operator` entry points
//! (`TypeNodeChecker` and `CheckerState`).

use super::unique_symbol_arena::{
    has_declared_unique_symbol_owner, is_readonly_unique_symbol_property_signature,
};
use crate::context::CheckerContext;
use crate::query_boundaries::type_construction::unique_symbol_ref_from_source_span;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::{SymbolRef, TypeId};

/// Resolve the type of a `readonly` type-operator annotation, shared by the two
/// `get_type_from_type_operator` entry points (`TypeNodeChecker` and
/// `CheckerState`).
///
/// tsc's `getTypeFromTypeOperatorNode` for the `readonly` keyword is
/// transparent: it returns `getTypeFromTypeNode(node.type)`. The readonly-ness
/// of an array or tuple is a property of that array/tuple type, which tsc bakes
/// in — by inspecting the parent operator — only when the operand is
/// *syntactically* an array or tuple literal type (`T[]` / `[T, U]`). On any
/// other operand (a primitive, union, object, parenthesized type, or a type
/// reference that merely aliases an array) `readonly` is a no-op: the annotation
/// resolves to the operand type unchanged, and a separate grammar check reports
/// TS1354.
///
/// Wrapping such an operand in a `ReadonlyType` marker — as tsz did
/// unconditionally — minted a distinct type that then spuriously failed
/// assignability (`let a: readonly number = 1` reported a bogus TS2322) and
/// rendered the operand as `readonly T` where tsc renders the bare `T`. Gating
/// the wrapper on the syntactic operand kind (`is_array_or_tuple_type`) mirrors
/// the grammar check that lives beside the checker call sites and the lowering
/// path, keeping all three consistent.
///
/// Callers pass the operand node's `kind` (already in hand) rather than its
/// `NodeIndex`, so this shares the lookup the grammar check already performs.
pub(crate) fn readonly_operator_result(
    ctx: &CheckerContext<'_>,
    operand_kind: Option<u16>,
    inner_type: TypeId,
) -> TypeId {
    if operand_kind.is_some_and(syntax_kind_ext::is_array_or_tuple_type) {
        ctx.types.factory().readonly_type(inner_type)
    } else {
        inner_type
    }
}

/// Resolve the type of a `unique symbol` type-operator annotation at `idx`,
/// whose inner operand already resolved to plain `symbol`.
///
/// Mirrors tsc's `getESSymbolLikeTypeForNode`:
/// - A `readonly` property signature (interface *or* object-type-literal
///   member) is a valid ES-symbol declaration, so it owns a distinct
///   `unique symbol`. Object-type literals previously missed this — they widened
///   to plain `symbol`, dropping the identity recovered by `typeof obj.prop`.
/// - A `const` variable / `static readonly` class property is also a declared
///   owner, but its identity is recovered through the `typeof` read path keyed
///   on the declaration symbol; the annotation lowers to plain `symbol` here so
///   `symbol`-typed initializers are accepted at the declaration site.
/// - Any other position synthesizes a source-position-stable identity.
pub(crate) fn unique_symbol_type_for_operator(
    ctx: &CheckerContext<'_>,
    idx: NodeIndex,
    pos: u32,
    end: u32,
) -> TypeId {
    if is_readonly_unique_symbol_property_signature(ctx.arena, idx)
        || !has_declared_unique_symbol_owner(ctx.arena, idx)
    {
        return ctx
            .types
            .unique_symbol(synthetic_unique_symbol_ref(&ctx.file_name, pos, end));
    }
    TypeId::SYMBOL
}

/// A source-position-stable `unique symbol` identity for a member whose
/// declaration is not itself a binder symbol (interface and object-type-literal
/// members are resolved structurally, not via `node_symbols`). The position
/// hash is unique per declaration, so distinct members stay distinct.
///
/// Delegates to the solver-owned [`unique_symbol_ref_from_source_span`] so the
/// checker and the lowering pass mint the same ref for the same declaration.
pub(crate) fn synthetic_unique_symbol_ref(file_name: &str, pos: u32, end: u32) -> SymbolRef {
    unique_symbol_ref_from_source_span(file_name, pos, end)
}
