//! Union member ordering: tsc 7 `stableTypeOrdering` for interned unions.
//!
//! `TypeInterner::sort_union_members` fixes the member order of every interned
//! union so union identity is a pure function of the member *set*, independent
//! of the order the members were interned in. The comparator mirrors tsc 7's
//! `stableTypeOrdering`: built-ins by a fixed key, then by `TypeData` rank, then
//! per-kind content — string/number literals by value, objects/callables by
//! symbol, applications and tuples/arrays by their (widened) component/element
//! keys — falling back to allocation order only when two members are otherwise
//! indistinguishable.
//!
//! Split out of `constructors.rs` to keep that module under the 2000-line source
//! ceiling; the pre-cached `CachedUnionMember`/`AppComponentKey` layout lives in
//! the sibling `interner::storage` module.

use super::interner::{
    AppComponentKey, CachedUnionMember, TYPE_LIST_INLINE, TypeInterner, TypeListBuffer,
};
use crate::types::{LiteralValue, TypeData, TypeId};
use smallvec::SmallVec;

impl TypeInterner {
    /// Sort key for union member ordering of built-in/intrinsic types.
    ///
    /// tsc sorts union members by type.id (allocation order). Built-in types get
    /// remapped keys so they sort consistently (e.g., null/undefined last)
    /// regardless of our internal TypeId numbering.
    ///
    /// Returns `Some(key)` for types with fixed sort positions, `None` for
    /// non-built-in types that should use semantic comparison instead.
    const fn builtin_sort_key(id: TypeId) -> Option<u32> {
        match id {
            TypeId::NUMBER => Some(9),
            TypeId::STRING => Some(8),
            TypeId::BIGINT => Some(10),
            TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE => Some(11),
            TypeId::BOOLEAN_FALSE => Some(12),
            TypeId::VOID => Some(13),
            TypeId::UNDEFINED => Some(14),
            TypeId::NULL => Some(15),
            TypeId::SYMBOL => Some(16),
            TypeId::OBJECT => Some(17),
            TypeId::FUNCTION => Some(18),
            _ if id.is_intrinsic() => Some(id.0),
            _ => None,
        }
    }

    /// Pre-compute cached data for a type to avoid repeated lookups during sort.
    ///
    /// This gathers `builtin_sort_key`, `lookup` (TypeData), object/callable symbol,
    /// and `alloc_order` in a single pass per union member.
    fn cache_union_member(&self, id: TypeId) -> CachedUnionMember {
        let builtin_key = Self::builtin_sort_key(id);
        if builtin_key.is_some() {
            // Builtins don't need further lookups
            return CachedUnionMember {
                id,
                builtin_key,
                data: None,
                obj_symbol: None,
                obj_anon_shape: None,
                callable_symbol: None,
                string_literal_text: None,
                app_components: None,
                elem_components: None,
                alloc_order: None,
            };
        }

        let data = self.lookup(id);
        let alloc_order = self.lookup_alloc_order(id);

        let mut obj_symbol = None;
        let mut obj_anon_shape = None;
        let mut callable_symbol = None;
        let mut string_literal_text = None;
        let mut app_components = None;
        let mut elem_components = None;

        if let Some(ref d) = data {
            match d {
                TypeData::Literal(LiteralValue::String(atom)) => {
                    string_literal_text = Some(self.string_interner.resolve(*atom));
                }
                TypeData::Object(s) | TypeData::ObjectWithIndex(s) => {
                    let shape = self.object_shape(*s);
                    if let Some(sym) = shape.symbol {
                        obj_symbol = Some(sym.0);
                    } else {
                        obj_anon_shape = Some(s.0);
                    }
                }
                TypeData::Callable(s) => {
                    let shape = self.callable_shape(*s);
                    if let Some(sym) = shape.symbol {
                        callable_symbol = Some(sym.0);
                    }
                }
                TypeData::Application(app) => {
                    let app = self.type_application(*app);
                    let mut keys = Vec::with_capacity(1 + app.args.len());
                    keys.push(self.app_component_key(app.base));
                    keys.extend(app.args.iter().map(|&arg| self.app_component_key(arg)));
                    app_components = Some(keys.into_boxed_slice());
                }
                TypeData::Tuple(list_id) => {
                    let elems = self.tuple_list(*list_id);
                    let keys: Vec<AppComponentKey> = elems
                        .iter()
                        .map(|elem| self.widened_app_component_key(elem.type_id))
                        .collect();
                    elem_components = Some(keys.into_boxed_slice());
                }
                TypeData::Array(elem) => {
                    elem_components =
                        Some(vec![self.widened_app_component_key(*elem)].into_boxed_slice());
                }
                _ => {}
            }
        }

        CachedUnionMember {
            id,
            builtin_key,
            data,
            obj_symbol,
            obj_anon_shape,
            callable_symbol,
            string_literal_text,
            app_components,
            elem_components,
            alloc_order,
        }
    }

    /// Pre-fetch the ordering key for a single `Application` component (base or
    /// type argument), performing every interner lookup that
    /// `compare_application_component` would otherwise do inside the sort
    /// comparator. The key captures exactly the fields that comparator inspects,
    /// so comparing two keys is order-identical to comparing the resolved ids.
    fn app_component_key(&self, id: TypeId) -> AppComponentKey {
        let builtin_key = Self::builtin_sort_key(id);

        let mut rank = None;
        let mut lazy_or_enum_defid = None;
        // Builtins sort purely by their fixed key, so skip the interner lookup.
        if builtin_key.is_none()
            && let Some(data) = self.lookup(id)
        {
            rank = Some(Self::type_data_rank(&data));
            match &data {
                TypeData::Lazy(def) | TypeData::Enum(def, _) => {
                    lazy_or_enum_defid = Some(def.0);
                }
                _ => {}
            }
        }

        AppComponentKey {
            builtin_key,
            rank,
            lazy_or_enum_defid,
            raw: id.0,
        }
    }

    /// Like [`app_component_key`](Self::app_component_key) but keyed on the
    /// element's *widened* type. Fresh literal elements (`"" `, `true`, `0`)
    /// widen to their primitive (`string`, `boolean`, `number`) before the key
    /// is taken, so tuple/array union members sort by the same widened element
    /// types tsc's `stableTypeOrdering` compares — e.g. `[string, number]`
    /// before `[string, boolean]`. Only scalar literals are widened here (a
    /// cheap `TypeId`-range / single lookup); non-scalar elements keep their own
    /// key so nested structure still orders by rank and identity.
    fn widened_app_component_key(&self, id: TypeId) -> AppComponentKey {
        let widened = if id == TypeId::BOOLEAN_TRUE || id == TypeId::BOOLEAN_FALSE {
            TypeId::BOOLEAN
        } else if id.is_intrinsic() {
            id
        } else if let Some(TypeData::Literal(value)) = self.lookup(id) {
            value.primitive_type_id()
        } else {
            id
        };
        self.app_component_key(widened)
    }

    /// Compare two pre-fetched `Application` component keys.
    ///
    /// This is the lookup-free equivalent of `compare_application_component`:
    /// builtin-key bucket first, then `TypeData` rank, then `DefId` for
    /// `Lazy`/`Enum`, then the raw `TypeId` as a stable tiebreak.
    fn compare_app_component_key(a: &AppComponentKey, b: &AppComponentKey) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        if a.raw == b.raw {
            return Ordering::Equal;
        }

        match (a.builtin_key, b.builtin_key) {
            (Some(ka), Some(kb)) => return ka.cmp(&kb).then_with(|| a.raw.cmp(&b.raw)),
            (Some(ka), None) => return ka.cmp(&100),
            (None, Some(kb)) => return 100u32.cmp(&kb),
            (None, None) => {}
        }

        if let (Some(ra), Some(rb)) = (a.rank, b.rank) {
            let rank_cmp = ra.cmp(&rb);
            if rank_cmp != Ordering::Equal {
                return rank_cmp;
            }
            if let (Some(da), Some(db)) = (a.lazy_or_enum_defid, b.lazy_or_enum_defid) {
                let cmp = da.cmp(&db);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }

        a.raw.cmp(&b.raw)
    }

    /// Compare two cached union members using pre-fetched data.
    ///
    /// This is semantically identical to `compare_union_members` but avoids
    /// all DashMap/arena lookups since data was pre-fetched into `CachedUnionMember`.
    fn compare_cached_members(a: &CachedUnionMember, b: &CachedUnionMember) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Fast path: built-in types have fixed sort positions. Break equal
        // built-in buckets with the raw TypeId so the comparator remains a
        // strict total order even when several intrinsic TypeIds share a bucket.
        match (a.builtin_key, b.builtin_key) {
            (Some(ka), Some(kb)) => return ka.cmp(&kb).then_with(|| a.id.0.cmp(&b.id.0)),
            (Some(ka), None) => {
                return ka.cmp(&100).then(std::cmp::Ordering::Less);
            }
            (None, Some(kb)) => {
                return 100u32.cmp(&kb).then(std::cmp::Ordering::Greater);
            }
            (None, None) => {}
        }

        let rank_a = Self::cached_union_member_rank(a);
        let rank_b = Self::cached_union_member_rank(b);
        let rank_cmp = rank_a.cmp(&rank_b);
        if rank_cmp != Ordering::Equal {
            return rank_cmp;
        }

        // Both are non-built-in types -- use cached type data
        if let (Some(data_a), Some(data_b)) = (&a.data, &b.data) {
            match (data_a, data_b) {
                (
                    TypeData::Literal(LiteralValue::String(_)),
                    TypeData::Literal(LiteralValue::String(_)),
                ) => {
                    let str_a = a
                        .string_literal_text
                        .as_deref()
                        .expect("string literal union member must cache resolved text");
                    let str_b = b
                        .string_literal_text
                        .as_deref()
                        .expect("string literal union member must cache resolved text");
                    // TypeScript 7 runs with `stableTypeOrdering` enabled, so
                    // `compareTypes` orders string literal types by their value
                    // rather than by creation order.
                    let cmp = str_a.cmp(str_b);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                (
                    TypeData::Literal(LiteralValue::Number(na)),
                    TypeData::Literal(LiteralValue::Number(nb)),
                ) => {
                    // TypeScript 7 runs with `stableTypeOrdering` enabled, so
                    // `compareTypes` orders numeric literal types by their value
                    // rather than by creation order.
                    let cmp = na.0.total_cmp(&nb.0);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                (TypeData::Lazy(d1), TypeData::Lazy(d2))
                | (TypeData::Enum(d1, _), TypeData::Enum(d2, _)) => {
                    let cmp = d1.0.cmp(&d2.0);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                (TypeData::Object(_), TypeData::Object(_))
                | (TypeData::ObjectWithIndex(_), TypeData::ObjectWithIndex(_))
                | (TypeData::Object(_), TypeData::ObjectWithIndex(_))
                | (TypeData::ObjectWithIndex(_), TypeData::Object(_)) => {
                    // Use pre-fetched symbol/shape data instead of re-looking up
                    // shapes. Compare option presence as part of the key; falling
                    // through to allocation order when only one side has a symbol
                    // can create non-transitive triples with symbol-keyed pairs.
                    let cmp = Self::compare_optional_u32(a.obj_symbol, b.obj_symbol);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                    let cmp = Self::compare_optional_u32(a.obj_anon_shape, b.obj_anon_shape);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                (TypeData::Callable(_), TypeData::Callable(_)) => {
                    let cmp = Self::compare_optional_u32(a.callable_symbol, b.callable_symbol);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                (TypeData::Application(_), TypeData::Application(_)) => {
                    // Keep application ordering total by comparing the stable raw
                    // component key sequence instead of recursing into union-member
                    // ordering. Components (base followed by each type argument)
                    // were pre-fetched into `app_components`, so this needs no
                    // interner lookups.
                    if let (Some(ca), Some(cb)) = (&a.app_components, &b.app_components) {
                        for (ka, kb) in ca.iter().zip(cb.iter()) {
                            let cmp = Self::compare_app_component_key(ka, kb);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        let cmp = ca.len().cmp(&cb.len());
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                (TypeData::Tuple(_), TypeData::Tuple(_))
                | (TypeData::Array(_), TypeData::Array(_)) => {
                    // Order element-bearing structural members by their widened
                    // element keys, mirroring the `Application` path. This
                    // completes tsc's `stableTypeOrdering` for tuples/arrays:
                    // `[string, number]` sorts before `[string, boolean]` because
                    // `number` precedes `boolean`, rather than by the source order
                    // in which the tuples happened to be interned. Element keys are
                    // pre-fetched into `elem_components`, so no lookups here. Equal
                    // element keys fall through to the allocation-order tiebreak
                    // below, preserving the previous behaviour for members that
                    // share a widened shape.
                    if let (Some(ca), Some(cb)) = (&a.elem_components, &b.elem_components) {
                        for (ka, kb) in ca.iter().zip(cb.iter()) {
                            let cmp = Self::compare_app_component_key(ka, kb);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        let cmp = ca.len().cmp(&cb.len());
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                _ => {}
            }
        }

        // Fallback: use pre-fetched allocation order
        let alloc_cmp = match (a.alloc_order, b.alloc_order) {
            (Some(oa), Some(ob)) => oa.cmp(&ob),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        if alloc_cmp != Ordering::Equal {
            return alloc_cmp;
        }

        a.id.0.cmp(&b.id.0)
    }

    fn compare_optional_u32(a: Option<u32>, b: Option<u32>) -> std::cmp::Ordering {
        match (a, b) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    }

    const fn cached_union_member_rank(member: &CachedUnionMember) -> u8 {
        match member.data.as_ref() {
            Some(data) => Self::type_data_rank(data),
            None => 34,
        }
    }

    const fn type_data_rank(data: &TypeData) -> u8 {
        match data {
            TypeData::Intrinsic(_) => 0,
            TypeData::Literal(LiteralValue::Number(_)) => 1,
            TypeData::Literal(LiteralValue::String(_)) => 2,
            TypeData::Literal(LiteralValue::BigInt(_)) => 3,
            TypeData::Literal(LiteralValue::Boolean(_)) => 4,
            TypeData::Object(_) => 5,
            TypeData::ObjectWithIndex(_) => 6,
            TypeData::Union(_) => 7,
            TypeData::Intersection(_) => 8,
            TypeData::Array(_) => 9,
            TypeData::Tuple(_) => 10,
            TypeData::Function(_) => 11,
            TypeData::Callable(_) => 12,
            TypeData::TypeParameter(_) => 13,
            TypeData::BoundParameter(_) => 14,
            TypeData::Lazy(_) => 15,
            TypeData::Recursive(_) => 16,
            TypeData::Enum(_, _) => 17,
            TypeData::Application(_) => 18,
            TypeData::Conditional(_) => 19,
            TypeData::Mapped(_) => 20,
            TypeData::IndexAccess(_, _) => 21,
            TypeData::TemplateLiteral(_) => 22,
            TypeData::TypeQuery(_) => 23,
            TypeData::KeyOf(_) => 24,
            TypeData::ReadonlyType(_) => 25,
            TypeData::UniqueSymbol(_) => 26,
            TypeData::Infer(_) => 27,
            TypeData::ThisType => 28,
            TypeData::StringIntrinsic { .. } => 29,
            TypeData::ModuleNamespace(_) => 30,
            TypeData::NoInfer(_) => 31,
            TypeData::Error => 32,
            TypeData::UnresolvedTypeName(_) => 33,
            TypeData::Substitution { .. } => 34,
        }
    }

    /// Sort union members using pre-cached lookups to avoid redundant `DashMap` reads.
    ///
    /// Instead of `sort_by(compare_union_members)` which does 4-6 DashMap/arena lookups
    /// per comparison (O(N log N * lookups)), this pre-caches all lookup data for each
    /// member in O(N) reads, then sorts using the cached data with zero further lookups.
    pub(crate) fn sort_union_members(&self, flat: &mut TypeListBuffer) {
        if flat.len() <= 1 {
            return;
        }

        // Pre-cache all lookup data for each member in a single pass: O(N) reads
        let mut cached: SmallVec<[CachedUnionMember; TYPE_LIST_INLINE]> =
            flat.iter().map(|&id| self.cache_union_member(id)).collect();

        // Sort using cached data: O(N log N) comparisons with zero further lookups.
        cached.sort_by(Self::compare_cached_members);

        // Write sorted TypeIds back
        for (i, member) in cached.iter().enumerate() {
            flat[i] = member.id;
        }
    }
}
