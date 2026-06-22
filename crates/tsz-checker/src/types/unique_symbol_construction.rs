//! Construction of `unique symbol` types for `unique symbol` type-operator
//! annotations, shared by the two `get_type_from_type_operator` entry points
//! (`TypeNodeChecker` and `CheckerState`).

use super::unique_symbol_arena::{
    has_declared_unique_symbol_owner, is_readonly_unique_symbol_property_signature,
};
use crate::context::CheckerContext;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{SymbolRef, TypeId};

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
pub(crate) fn synthetic_unique_symbol_ref(file_name: &str, pos: u32, end: u32) -> SymbolRef {
    let mut hash = 0x811c_9dc5u32;
    for byte in file_name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    for value in [pos, end] {
        hash ^= value;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    SymbolRef(hash | 0x8000_0000)
}
