//! Public `TypeFlags` bridge for non-solver crates.
//!
//! Wasm and server bindings need to expose tsc-style `TypeFlags` and a
//! `isNullableType` predicate without inspecting `TypeData` directly
//! (the architecture guard forbids it outside `tsz-solver` /
//! `tsz-lowering`). The helpers in this module package the structural
//! match against `TypeData` next to the `TypeFlags` bit constants so
//! callers only need to depend on the public surface.

use crate::query::TypeStore;
use crate::types::{IntrinsicKind, LiteralValue};
use crate::{TypeData, TypeId};

/// Bit values mirroring TypeScript's public `TypeFlags` enum from
/// `compiler/types.ts`. Exposed here so wasm/server bindings can return
/// the same bits a JS caller would see from `tsserver` without inspecting
/// `TypeData` internals from outside the solver.
pub mod flags {
    pub const ANY: u32 = 1 << 0;
    pub const UNKNOWN: u32 = 1 << 1;
    pub const STRING: u32 = 1 << 2;
    pub const NUMBER: u32 = 1 << 3;
    pub const BOOLEAN: u32 = 1 << 4;
    pub const ENUM: u32 = 1 << 5;
    pub const BIG_INT: u32 = 1 << 6;
    pub const STRING_LITERAL: u32 = 1 << 7;
    pub const NUMBER_LITERAL: u32 = 1 << 8;
    pub const BOOLEAN_LITERAL: u32 = 1 << 9;
    pub const BIG_INT_LITERAL: u32 = 1 << 11;
    pub const ES_SYMBOL: u32 = 1 << 12;
    pub const UNIQUE_ES_SYMBOL: u32 = 1 << 13;
    pub const VOID: u32 = 1 << 14;
    pub const UNDEFINED: u32 = 1 << 15;
    pub const NULL: u32 = 1 << 16;
    pub const NEVER: u32 = 1 << 17;
    pub const TYPE_PARAMETER: u32 = 1 << 18;
    pub const OBJECT: u32 = 1 << 19;
    pub const UNION: u32 = 1 << 20;
    pub const INTERSECTION: u32 = 1 << 21;
    pub const INDEX: u32 = 1 << 22;
    pub const INDEXED_ACCESS: u32 = 1 << 23;
    pub const CONDITIONAL: u32 = 1 << 24;
    pub const NON_PRIMITIVE: u32 = 1 << 26;
    pub const TEMPLATE_LITERAL: u32 = 1 << 27;
    pub const STRING_MAPPING: u32 = 1 << 28;
}

/// Compute the public `TypeFlags` bitset for a type id, matching tsc's
/// classification. Use from non-solver crates that need to expose
/// type-shape categories (wasm bindings, server) without inspecting
/// `TypeData` directly.
pub fn type_id_ts_flags(types: &dyn TypeStore, type_id: TypeId) -> u32 {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return flags::BOOLEAN_LITERAL | flags::BOOLEAN;
    }
    let Some(data) = types.lookup(type_id) else {
        return 0;
    };
    match data {
        TypeData::Intrinsic(kind) => match kind {
            IntrinsicKind::Any => flags::ANY,
            IntrinsicKind::Unknown => flags::UNKNOWN,
            IntrinsicKind::Never => flags::NEVER,
            IntrinsicKind::Void => flags::VOID,
            IntrinsicKind::Null => flags::NULL,
            IntrinsicKind::Undefined => flags::UNDEFINED,
            IntrinsicKind::Boolean => flags::BOOLEAN,
            IntrinsicKind::Number => flags::NUMBER,
            IntrinsicKind::String => flags::STRING,
            IntrinsicKind::Bigint => flags::BIG_INT,
            IntrinsicKind::Symbol => flags::ES_SYMBOL,
            IntrinsicKind::Object => flags::NON_PRIMITIVE,
            IntrinsicKind::Function => flags::OBJECT,
        },
        TypeData::Literal(value) => match value {
            LiteralValue::String(_) => flags::STRING_LITERAL,
            LiteralValue::Number(_) => flags::NUMBER_LITERAL,
            LiteralValue::Boolean(_) => flags::BOOLEAN_LITERAL | flags::BOOLEAN,
            LiteralValue::BigInt(_) => flags::BIG_INT_LITERAL,
        },
        TypeData::Union(_) => flags::UNION,
        TypeData::Intersection(_) => flags::INTERSECTION,
        TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_) => {
            flags::TYPE_PARAMETER
        }
        TypeData::Conditional(_) => flags::CONDITIONAL,
        TypeData::IndexAccess(_, _) => flags::INDEXED_ACCESS,
        TypeData::KeyOf(_) => flags::INDEX,
        TypeData::TemplateLiteral(_) => flags::TEMPLATE_LITERAL,
        TypeData::StringIntrinsic { .. } => flags::STRING_MAPPING,
        TypeData::UniqueSymbol(_) => flags::UNIQUE_ES_SYMBOL,
        TypeData::Enum(_, _) => flags::ENUM,
        TypeData::Object(_)
        | TypeData::ObjectWithIndex(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Function(_)
        | TypeData::Callable(_)
        | TypeData::Mapped(_)
        | TypeData::Application(_)
        | TypeData::ReadonlyType(_)
        | TypeData::ModuleNamespace(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::NoInfer(_)
        | TypeData::TypeQuery(_)
        | TypeData::ThisType => flags::OBJECT,
        TypeData::Error | TypeData::UnresolvedTypeName(_) => 0,
    }
}

/// Check whether a type is `null` / `undefined` or a union containing them.
pub fn is_nullable_type(types: &dyn TypeStore, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return true;
    }
    match types.lookup(type_id) {
        Some(TypeData::Union(list_id)) => types
            .type_list(list_id)
            .iter()
            .any(|&member| member == TypeId::NULL || member == TypeId::UNDEFINED),
        _ => false,
    }
}
