use super::interner::{
    CachedUnionMember, TYPE_LIST_INLINE, TypeInterner, TypeListBuffer, TypeShard,
};

use crate::def::DefId;

use crate::types::{
    CallableShape, ConditionalType, FunctionShape, IntrinsicKind, LiteralValue, MappedType,
    ObjectFlags, ObjectShape, ObjectShapeId, OrderedFloat, PropertyInfo, SymbolRef, TemplateSpan,
    TupleElement, TypeApplication, TypeData, TypeId, TypeParamInfo,
    normalize_display_property_order,
};

use rustc_hash::FxHashSet;

use smallvec::SmallVec;

use std::sync::Arc;

use std::sync::atomic::Ordering;

use tsz_common::interner::Atom;

include!("constructors_parts/part1.rs");
include!("constructors_parts/part2.rs");

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}
