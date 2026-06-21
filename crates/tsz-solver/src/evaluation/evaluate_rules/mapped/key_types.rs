//! Mapped-key collection types.

use crate::types::TypeId;
use tsz_common::interner::Atom;

/// One iteration step of a mapped type: the property-name atom plus the
/// `TypeId` that should be substituted for the iteration variable.
///
/// `LiteralValue::String("1")` and `LiteralValue::Number(1)` intern to the
/// same atom `"1"`, so the atom alone cannot disambiguate the substitution.
/// Storing the literal `TypeId` keeps that distinction and avoids re-parsing
/// the atom back to `f64` on every iteration: `[K in 1]: K` evaluates with
/// `K -> Literal(Number(1))` instead of `K -> Literal(String("1"))`.
#[derive(Clone, Copy)]
pub(crate) struct MappedKey {
    pub name: Atom,
    pub key_literal: TypeId,
}

pub(crate) struct MappedKeys {
    pub keys: Vec<MappedKey>,
    pub has_string: bool,
    pub has_number: bool,
    /// Whether the constraint's key space includes the broad `symbol` type
    /// (e.g. `{ [P in symbol]: V }`, `Record<PropertyKey, V>`). This produces a
    /// symbol-keyed index signature on the result object — distinct from the
    /// concrete unique-symbol keys tracked by `symbol_keys`. Dropping it makes
    /// `keyof` and indexed access lose the symbol key space, yielding spurious
    /// `TS2536`/`TS2862` on any symbol-bearing access (see #14315).
    pub has_symbol: bool,
    /// Template literal types used as mapped-type key constraints (e.g. `` `on${string}` ``).
    /// When non-empty and `has_string` is false, the object gets a template-literal index
    /// signature instead of a plain string index signature.
    pub template_literals: Vec<TypeId>,
    /// Unique-symbol keys (e.g. `typeof sym1`) that appear in `keyof T` when T has
    /// symbol-keyed properties. Each element is a `TypeData::UniqueSymbol` `TypeId`.
    pub symbol_keys: Vec<TypeId>,
}
