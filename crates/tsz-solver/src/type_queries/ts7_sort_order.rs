//! TypeScript 7 union-member sort ranks.
//!
//! TS7's formatter orders union members by `TypeFlags` rank. This query owns
//! the rank model so consumers (the emitter's type printer, diagnostic
//! formatting) never inspect `TypeData` themselves. Lazy references are
//! resolved through a caller-supplied hook because each consumer has its own
//! `DefId` resolution (printer type cache vs. checker environment).

use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::types::{LiteralValue, TypeData, TypeId};
use crate::visitor;

/// Rank a type for TS7 union-member ordering. Lower ranks sort first.
///
/// Primitive intrinsics use their TS7 `TypeFlags` values; literals, enums,
/// type parameters, object-like types, and type-operator forms use stable
/// bucket ranks above the primitives. Unknown shapes rank last.
pub fn ts7_union_sort_rank(
    interner: &TypeInterner,
    type_id: TypeId,
    resolve_lazy: &dyn Fn(u32) -> Option<TypeId>,
) -> u32 {
    if let Some(rank) = ts7_primitive_rank(type_id) {
        return rank;
    }
    if let Some(literal) = ts7_sort_literal(interner, type_id, resolve_lazy) {
        return match literal {
            LiteralValue::String(_) => 1 << 10,
            LiteralValue::Number(_) => 1 << 11,
            LiteralValue::BigInt(_) => 1 << 12,
            LiteralValue::Boolean(_) => 1 << 13,
        };
    }
    match interner.lookup(type_id) {
        Some(TypeData::UniqueSymbol(_)) => 1 << 14,
        Some(TypeData::Enum(_, _)) => 1 << 15,
        Some(TypeData::TypeParameter(_) | TypeData::BoundParameter(_) | TypeData::Infer(_)) => {
            1 << 19
        }
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Application(_)
            | TypeData::Lazy(_)
            | TypeData::Mapped(_),
        ) => 1 << 20,
        Some(TypeData::KeyOf(_)) => 1 << 21,
        Some(TypeData::TemplateLiteral(_)) => 1 << 22,
        Some(TypeData::StringIntrinsic { .. }) => 1 << 23,
        Some(TypeData::Substitution { .. }) => 1 << 24,
        Some(TypeData::IndexAccess(_, _)) => 1 << 25,
        Some(TypeData::Conditional(_)) => 1 << 26,
        Some(TypeData::Union(_)) => 1 << 27,
        Some(TypeData::Intersection(_)) => 1 << 28,
        _ => u32::MAX,
    }
}

/// TS7 `TypeFlags` ranks for primitive intrinsics; `None` for everything else.
pub const fn ts7_primitive_rank(id: TypeId) -> Option<u32> {
    match id {
        TypeId::ANY => Some(1),
        TypeId::UNKNOWN => Some(2),
        TypeId::VOID => Some(16),
        TypeId::STRING => Some(32),
        TypeId::NUMBER => Some(64),
        TypeId::BIGINT => Some(128),
        TypeId::BOOLEAN => Some(256),
        TypeId::SYMBOL => Some(512),
        // The non-primitive `object` sorts after primitives/literals/enums but
        // BEFORE type parameters and the undefined/null tail (oracle:
        // '"lit" | object', 'string | object', 'object | U',
        // 'object | undefined').
        TypeId::OBJECT => Some(1 << 16),
        _ => None,
    }
}

/// Chase a type to its literal value through `Lazy`/`Substitution` wrappers
/// (bounded, cycle-safe) for literal-tier ranking and tie-breaks.
pub fn ts7_sort_literal(
    interner: &TypeInterner,
    type_id: TypeId,
    resolve_lazy: &dyn Fn(u32) -> Option<TypeId>,
) -> Option<LiteralValue> {
    let mut current = type_id;
    let mut seen = rustc_hash::FxHashSet::default();
    for _ in 0..16 {
        if !seen.insert(current) {
            return None;
        }
        if let Some(literal) = visitor::literal_value(interner, current) {
            return Some(literal);
        }
        match interner.lookup(current)? {
            TypeData::Lazy(def_id) => current = resolve_lazy(def_id.0)?,
            TypeData::Substitution { base_type, .. } => current = base_type,
            _ => return None,
        }
    }
    None
}

/// Neutral handle for deriving a union member's tie-break display name.
///
/// The consumer resolves the handle with its own name tables (printer type
/// cache, binder symbol arena) so this query never touches those layers.
pub enum Ts7SortNameSource {
    /// A `Lazy`/`Enum` semantic reference: resolve via `DefId` name tables.
    Def(DefId),
    /// An object/callable shape's declaring symbol.
    Symbol(tsz_binder::SymbolId),
    /// A type parameter's name atom.
    Atom(tsz_common::interner::Atom),
}

/// Classify where a union member's tie-break name comes from, chasing
/// `Application` bases (bounded).
pub fn ts7_sort_name_source(interner: &TypeInterner, type_id: TypeId) -> Option<Ts7SortNameSource> {
    let mut current = type_id;
    for _ in 0..16 {
        match interner.lookup(current)? {
            TypeData::Application(app_id) => {
                current = interner.type_application(app_id).base;
            }
            TypeData::Lazy(def_id) | TypeData::Enum(def_id, _) => {
                return Some(Ts7SortNameSource::Def(def_id));
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                return interner
                    .object_shape(shape_id)
                    .symbol
                    .map(Ts7SortNameSource::Symbol);
            }
            TypeData::Callable(shape_id) => {
                return interner
                    .callable_shape(shape_id)
                    .symbol
                    .map(Ts7SortNameSource::Symbol);
            }
            TypeData::TypeParameter(param) | TypeData::Infer(param) => {
                return Some(Ts7SortNameSource::Atom(param.name));
            }
            _ => return None,
        }
    }
    None
}
