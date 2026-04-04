//! Type construction convenience methods for `TypeInterner`.
//!
//! This module contains all the builder/factory methods for creating
//! interned types: literals, unions, intersections, objects, functions, etc.

use super::interner::{
    CachedUnionMember, TYPE_LIST_INLINE, TypeInterner, TypeListBuffer, TypeShard,
};
use crate::def::DefId;
use crate::types::{
    CallableShape, ConditionalType, FunctionShape, IntrinsicKind, LiteralValue, MappedType,
    ObjectFlags, ObjectShape, ObjectShapeId, OrderedFloat, PropertyInfo, SymbolRef, TemplateSpan,
    TupleElement, TypeApplication, TypeData, TypeId, TypeParamInfo,
};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tsz_common::interner::Atom;

impl TypeInterner {
    // =========================================================================
    // Convenience methods for common type constructions
    // =========================================================================

    /// Intern an intrinsic type
    pub const fn intrinsic(&self, kind: IntrinsicKind) -> TypeId {
        kind.to_type_id()
    }

    /// Intern a literal string type
    pub fn literal_string(&self, value: &str) -> TypeId {
        let atom = self.intern_string(value);
        self.intern(TypeData::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal string type from an already-interned Atom
    pub fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::String(atom)))
    }

    /// Intern a literal number type
    pub fn literal_number(&self, value: f64) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::Number(OrderedFloat(value))))
    }

    /// Intern a literal boolean type
    pub fn literal_boolean(&self, value: bool) -> TypeId {
        self.intern(TypeData::Literal(LiteralValue::Boolean(value)))
    }

    /// Intern a literal bigint type
    pub fn literal_bigint(&self, value: &str) -> TypeId {
        let atom = self.intern_string(&self.normalize_bigint_literal(value));
        self.intern(TypeData::Literal(LiteralValue::BigInt(atom)))
    }

    /// Intern a literal bigint type, allowing a sign prefix without extra clones.
    pub fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        let normalized = self.normalize_bigint_literal(digits);
        if normalized == "0" {
            return self.literal_bigint(&normalized);
        }
        if !negative {
            return self.literal_bigint(&normalized);
        }

        let mut value = String::with_capacity(normalized.len() + 1);
        value.push('-');
        value.push_str(&normalized);
        let atom = self.string_interner.intern_owned(value);
        self.intern(TypeData::Literal(LiteralValue::BigInt(atom)))
    }

    fn normalize_bigint_literal(&self, value: &str) -> String {
        let stripped = value.replace('_', "");
        if stripped.is_empty() {
            return "0".to_string();
        }

        let (base, digits) = if stripped.starts_with("0x") || stripped.starts_with("0X") {
            (16, &stripped[2..])
        } else if stripped.starts_with("0o") || stripped.starts_with("0O") {
            (8, &stripped[2..])
        } else if stripped.starts_with("0b") || stripped.starts_with("0B") {
            (2, &stripped[2..])
        } else {
            (10, stripped.as_str())
        };

        if digits.is_empty() {
            return "0".to_string();
        }

        if base == 10 {
            let normalized = digits.trim_start_matches('0');
            return if normalized.is_empty() {
                "0".to_string()
            } else {
                normalized.to_string()
            };
        }

        let mut decimal: Vec<u8> = vec![0];
        for ch in digits.chars() {
            let Some(digit) = ch.to_digit(base) else {
                return "0".to_string();
            };
            let digit = digit as u16;
            let mut carry = digit;
            let base = base as u16;
            for dec in decimal.iter_mut() {
                let value = u16::from(*dec) * base + carry;
                *dec = (value % 10) as u8;
                carry = value / 10;
            }
            while carry > 0 {
                decimal.push((carry % 10) as u8);
                carry /= 10;
            }
        }

        while decimal.len() > 1 && *decimal.last().unwrap_or(&0) == 0 {
            decimal.pop();
        }

        let mut out = String::with_capacity(decimal.len());
        for digit in decimal.iter().rev() {
            out.push(char::from(b'0' + *digit));
        }
        out
    }

    /// Intern a union type, normalizing and deduplicating members.
    /// This performs full normalization including subtype reduction
    /// (matching tsc's `UnionReduction.Subtype` behavior).
    pub fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.union_from_iter(members)
    }

    /// Create a union from a borrowed slice, avoiding allocation when callers
    /// already have an `Arc<[TypeId]>` or `&[TypeId]`.
    pub fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        self.union_from_iter(members.iter().copied())
    }

    /// Intern a union type with literal-only reduction (no subtype reduction).
    ///
    /// This matches tsc's `UnionReduction.Literal` behavior, which is the default
    /// for type annotations. It absorbs literals into primitives (e.g., `"a" | string`
    /// → `string`) but does NOT remove structural subtypes (e.g., `C | D` where
    /// `D extends C` stays as `C | D`).
    ///
    /// Use this for union types from type annotations where the source-level
    /// union structure must be preserved.
    pub fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        self.union_literal_reduce_from_iter(members)
    }

    /// Intern a union type from a vector that is already sorted and deduped.
    /// This is an O(N) operation that avoids redundant sorting.
    pub fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list(flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Intern a union type while preserving member structure.
    ///
    /// This keeps unknown/literal members intact for property access checks.
    pub fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        if members.is_empty() {
            return TypeId::NEVER;
        }

        let mut flat: TypeListBuffer = SmallVec::new();
        for member in members {
            if let Some(TypeData::Union(inner)) = self.lookup(member) {
                let members = self.type_list(inner);
                flat.extend(members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        self.sort_union_members(&mut flat);
        flat.dedup();
        flat.retain(|id| *id != TypeId::NEVER);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Fast path for unions that already fit in registers.
    pub fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        // Fast paths to avoid expensive normalize_union for trivial cases
        if left == right {
            return left;
        }
        if left == TypeId::NEVER {
            return right;
        }
        if right == TypeId::NEVER {
            return left;
        }
        // Fast path: `T | undefined`, `T | null`, `T | void` where T is a union
        // already containing the nullable member.  This avoids the full
        // collect → sort → dedup → absorb → reduce pipeline for the extremely
        // common optional-chain pattern `result_type | undefined`.
        if right.is_nullable() {
            if let Some(TypeData::Union(list_id)) = self.lookup(left) {
                let members = self.type_list(list_id);
                if members.contains(&right) {
                    return left;
                }
            }
        } else if left.is_nullable()
            && let Some(TypeData::Union(list_id)) = self.lookup(right)
        {
            let members = self.type_list(list_id);
            if members.contains(&left) {
                return right;
            }
        }

        // PERF: Fast path for `T | Union(members)` where T is a non-union, non-special type.
        // Instead of full normalize_union (flatten + sort + dedup + absorb + reduce),
        // directly insert T into the existing sorted member list. This turns the
        // O(N log N) sort into O(N) for the common case of accumulating unions
        // (e.g., deeply nested ternary chains where each level adds one type).
        if let Some(result) = self.try_union2_insert(left, right) {
            return result;
        }
        if let Some(result) = self.try_union2_insert(right, left) {
            return result;
        }

        self.union_from_iter([left, right])
    }

    /// Try to insert a single non-union type into an existing union without full normalization.
    ///
    /// Returns `Some(result)` if the fast path applies, `None` otherwise.
    /// The fast path applies when:
    /// - `single` is not a union, not NEVER/ANY/UNKNOWN/ERROR (special types need full handling)
    /// - `existing` is a union
    /// - `single` is not a literal that could be absorbed by a primitive in the union
    /// - `single` is not a type that requires subtype reduction
    fn try_union2_insert(&self, single: TypeId, existing: TypeId) -> Option<TypeId> {
        // single must not be a union or special type
        if single == TypeId::ANY
            || single == TypeId::UNKNOWN
            || single == TypeId::ERROR
            || single == TypeId::NEVER
        {
            return None;
        }

        // Check single is not a union
        if matches!(self.lookup(single), Some(TypeData::Union(_))) {
            return None;
        }

        // existing must be a union
        let Some(TypeData::Union(list_id)) = self.lookup(existing) else {
            return None;
        };

        // Skip if single is a literal that could be absorbed by a primitive in the union.
        // e.g., "hello" | string -> string (literal absorbed)
        if let Some(TypeData::Literal(lit)) = self.lookup(single) {
            let base_primitive = match lit {
                crate::types::LiteralValue::String(_) => TypeId::STRING,
                crate::types::LiteralValue::Number(_) => TypeId::NUMBER,
                crate::types::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
                crate::types::LiteralValue::BigInt(_) => TypeId::BIGINT,
            };
            let members = self.type_list(list_id);
            if members.contains(&base_primitive) {
                // Literal absorbed by primitive - return existing union
                return Some(existing);
            }
        }

        let members = self.type_list(list_id);

        // Check if single is already in the union (dedup)
        if members.contains(&single) {
            return Some(existing);
        }

        // Build new member list with single inserted.
        // The existing members are already sorted and deduped.
        // Use the allocation-order sort key: non-builtin types are sorted by
        // their TypeId (which correlates with allocation order for user types).
        let mut new_members: TypeListBuffer = SmallVec::with_capacity(members.len() + 1);

        // Find insertion point using sort key comparison
        let single_key = Self::builtin_sort_key(single);
        let single_alloc = self.lookup_alloc_order(single);
        let mut inserted = false;

        for &m in members.iter() {
            if !inserted {
                let should_insert_before = {
                    let m_key = Self::builtin_sort_key(m);
                    match (single_key, m_key) {
                        (Some(sk), Some(mk)) => sk < mk,
                        (Some(_), None) => true, // builtins before non-builtins
                        (None, Some(_)) => false, // non-builtins after builtins
                        (None, None) => {
                            // Both non-builtin: compare by allocation order
                            let m_alloc = self.lookup_alloc_order(m);
                            match (single_alloc, m_alloc) {
                                (Some(sa), Some(ma)) => sa < ma,
                                _ => single.0 < m.0,
                            }
                        }
                    }
                };
                if should_insert_before {
                    new_members.push(single);
                    inserted = true;
                }
            }
            new_members.push(m);
        }
        if !inserted {
            new_members.push(single);
        }

        let list_id = self.intern_type_list_from_slice(&new_members);
        Some(self.intern(TypeData::Union(list_id)))
    }

    /// Fast path for three-member unions without heap allocations.
    pub fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.union_from_iter([first, second, third])
    }

    pub(crate) fn union_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let flat = self.collect_union_members(members);
        match flat.len() {
            0 => TypeId::NEVER,
            1 => flat[0],
            _ => self.normalize_union(flat),
        }
    }

    fn union_literal_reduce_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let flat = self.collect_union_members(members);
        match flat.len() {
            0 => TypeId::NEVER,
            1 => flat[0],
            _ => self.normalize_union_literal_only(flat),
        }
    }

    fn collect_union_members<I>(&self, members: I) -> TypeListBuffer
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut iter = members.into_iter();
        let Some(first) = iter.next() else {
            return SmallVec::new();
        };
        let Some(second) = iter.next() else {
            let mut buf = SmallVec::new();
            buf.push(first);
            return buf;
        };

        let mut flat: TypeListBuffer = SmallVec::new();
        self.push_union_member(&mut flat, first);
        self.push_union_member(&mut flat, second);
        for member in iter {
            self.push_union_member(&mut flat, member);
        }
        flat
    }

    pub(super) fn push_union_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeData::Union(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

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
                alloc_order: None,
            };
        }

        let data = self.lookup(id);
        let alloc_order = self.lookup_alloc_order(id);

        let mut obj_symbol = None;
        let mut obj_anon_shape = None;
        let mut callable_symbol = None;

        if let Some(ref d) = data {
            match d {
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
            alloc_order,
        }
    }

    /// Compare two cached union members using pre-fetched data.
    ///
    /// This is semantically identical to `compare_union_members` but avoids
    /// all DashMap/arena lookups since data was pre-fetched into `CachedUnionMember`.
    fn compare_cached_members(
        &self,
        a: &CachedUnionMember,
        b: &CachedUnionMember,
    ) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Fast path: built-in types have fixed sort positions
        match (a.builtin_key, b.builtin_key) {
            (Some(ka), Some(kb)) => return ka.cmp(&kb),
            (Some(ka), None) => return ka.cmp(&100),
            (None, Some(kb)) => return 100u32.cmp(&kb),
            (None, None) => {}
        }

        // Both are non-built-in types -- use cached type data
        if let (Some(data_a), Some(data_b)) = (&a.data, &b.data) {
            match (data_a, data_b) {
                (
                    TypeData::Literal(LiteralValue::String(sa)),
                    TypeData::Literal(LiteralValue::String(sb)),
                ) => {
                    let str_a = self.string_interner.resolve(*sa);
                    let str_b = self.string_interner.resolve(*sb);
                    let a_short = str_a.len() <= 2;
                    let b_short = str_b.len() <= 2;
                    match (a_short, b_short) {
                        (true, true) => {
                            let cmp = str_a.cmp(&str_b);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less,
                        (false, true) => return Ordering::Greater,
                        (false, false) => {}
                    }
                }
                (
                    TypeData::Literal(LiteralValue::Number(na)),
                    TypeData::Literal(LiteralValue::Number(nb)),
                ) => {
                    let a_small = na.0.abs() < 10.0;
                    let b_small = nb.0.abs() < 10.0;
                    match (a_small, b_small) {
                        (true, true) => {
                            let cmp = na.0.partial_cmp(&nb.0).unwrap_or(Ordering::Equal);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less,
                        (false, true) => return Ordering::Greater,
                        (false, false) => {}
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
                    // Use pre-fetched symbol data instead of re-looking up shapes
                    if let (Some(sym1), Some(sym2)) = (a.obj_symbol, b.obj_symbol) {
                        let cmp = sym1.cmp(&sym2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    // Anonymous objects: use pre-fetched ShapeId
                    if let (Some(shape1), Some(shape2)) = (a.obj_anon_shape, b.obj_anon_shape) {
                        let cmp = shape1.cmp(&shape2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                (TypeData::Callable(_), TypeData::Callable(_)) => {
                    // Use pre-fetched symbol data
                    if let (Some(sym1), Some(sym2)) = (a.callable_symbol, b.callable_symbol) {
                        let cmp = sym1.cmp(&sym2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                (TypeData::Application(app1), TypeData::Application(app2)) => {
                    // Application types still need recursive comparison for base/args,
                    // but the top-level union members' lookups are already cached.
                    let a1 = self.type_application(*app1);
                    let a2 = self.type_application(*app2);
                    let cmp = self.compare_union_members(a1.base, a2.base);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                    for (arg1, arg2) in a1.args.iter().zip(a2.args.iter()) {
                        let cmp = self.compare_union_members(*arg1, *arg2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    let cmp = a1.args.len().cmp(&a2.args.len());
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                _ => {}
            }
        }

        // Fallback: use pre-fetched allocation order
        match (a.alloc_order, b.alloc_order) {
            (Some(oa), Some(ob)) => oa.cmp(&ob),
            _ => a.id.0.cmp(&b.id.0),
        }
    }

    /// Sort union members using pre-cached lookups to avoid redundant `DashMap` reads.
    ///
    /// Instead of `sort_by(compare_union_members)` which does 4-6 DashMap/arena lookups
    /// per comparison (O(N log N * lookups)), this pre-caches all lookup data for each
    /// member in O(N) reads, then sorts using the cached data with zero further lookups.
    fn sort_union_members(&self, flat: &mut TypeListBuffer) {
        if flat.len() <= 1 {
            return;
        }

        // Pre-cache all lookup data for each member in a single pass: O(N) reads
        let mut cached: SmallVec<[CachedUnionMember; TYPE_LIST_INLINE]> =
            flat.iter().map(|&id| self.cache_union_member(id)).collect();

        // Sort using cached data: O(N log N) comparisons with zero further lookups
        // (except for Application types which still recurse via compare_union_members).
        // The comparison function has edge cases where transitivity may not hold
        // (mixed type data variants with alloc_order fallback). Catch panics from
        // Rust's total-order validation and fall back to TypeId-based sorting.
        {
            // Suppress panic output from total-order validation
            let prev_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cached.sort_by(|a, b| self.compare_cached_members(a, b));
            }));
            std::panic::set_hook(prev_hook);
            if result.is_err() {
                // Fallback: sort by raw TypeId for deterministic but non-semantic order
                cached.sort_by_key(|m| m.id.0);
            }
        }

        // Write sorted TypeIds back
        for (i, member) in cached.iter().enumerate() {
            flat[i] = member.id;
        }
    }

    /// Compare two union members for ordering.
    ///
    /// For built-in/intrinsic types: uses fixed sort keys for consistent ordering
    /// (e.g., null/undefined always last).
    ///
    /// For non-built-in types of the same category: uses semantic identity
    /// (literal content, DefId, SymbolId) to approximate tsc's source-order
    /// allocation. This ensures e.g. `"A" | "B" | "C"` instead of arbitrary
    /// interning order, and `C | D` for `class C {}; class D extends C {}`.
    ///
    /// Fallback: raw TypeId comparison.
    ///
    /// NOTE: This is kept for the rare recursive Application comparison path.
    /// The primary sort path uses `sort_union_members` with pre-computed keys.
    fn compare_union_members(&self, a: TypeId, b: TypeId) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Fast path: built-in types have fixed sort positions
        let builtin_a = Self::builtin_sort_key(a);
        let builtin_b = Self::builtin_sort_key(b);
        match (builtin_a, builtin_b) {
            (Some(ka), Some(kb)) => return ka.cmp(&kb),
            (Some(ka), None) => {
                // Built-in vs non-built-in: built-in types sort by their
                // fixed key, non-built-in types are at position >= 100
                return ka.cmp(&100);
            }
            (None, Some(kb)) => {
                return 100u32.cmp(&kb);
            }
            (None, None) => {}
        }

        // Both are non-built-in types. Use semantic identity for ordering
        // where TypeId creation order doesn't match tsc's source-order allocation.
        // The sharded interner embeds shard index in TypeId, making raw TypeId
        // comparison hash-dependent. Semantic comparison ensures deterministic order.
        if let (Some(data_a), Some(data_b)) = (self.lookup(a), self.lookup(b)) {
            match (&data_a, &data_b) {
                // Short string literals (1-2 chars): sort by content to match tsc's
                // lib.d.ts pre-allocation order. tsc pre-creates common short string
                // literal types during lib processing, giving them lower type IDs
                // than user-code types. Since these happen to be ordered by content
                // in tsc, content-based sorting matches tsc's display. For longer
                // strings, alloc_order fallback preserves source encounter order.
                //
                // Short strings sort BEFORE long strings (matching tsc where
                // lib-pre-allocated types have lower IDs). This ensures transitivity.
                (
                    TypeData::Literal(LiteralValue::String(sa)),
                    TypeData::Literal(LiteralValue::String(sb)),
                ) => {
                    let str_a = self.string_interner.resolve(*sa);
                    let str_b = self.string_interner.resolve(*sb);
                    let a_short = str_a.len() <= 2;
                    let b_short = str_b.len() <= 2;
                    match (a_short, b_short) {
                        (true, true) => {
                            // Both short: sort by content
                            let cmp = str_a.cmp(&str_b);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less, // short < long
                        (false, true) => return Ordering::Greater, // long > short
                        (false, false) => {
                            // Both long: fall through to allocation order
                        }
                    }
                }
                // Small number literals (0-9): sort numerically to match tsc's
                // lib.d.ts pre-allocation order for common small numbers.
                // Small numbers sort BEFORE large numbers for transitivity.
                (
                    TypeData::Literal(LiteralValue::Number(na)),
                    TypeData::Literal(LiteralValue::Number(nb)),
                ) => {
                    let a_small = na.0.abs() < 10.0;
                    let b_small = nb.0.abs() < 10.0;
                    match (a_small, b_small) {
                        (true, true) => {
                            let cmp = na.0.partial_cmp(&nb.0).unwrap_or(Ordering::Equal);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        (true, false) => return Ordering::Less,
                        (false, true) => return Ordering::Greater,
                        (false, false) => {
                            // Both large: fall through to allocation order
                        }
                    }
                }
                // Lazy type references and Enum types: sort by DefId (source declaration order)
                (TypeData::Lazy(d1), TypeData::Lazy(d2))
                | (TypeData::Enum(d1, _), TypeData::Enum(d2, _)) => {
                    let cmp = d1.0.cmp(&d2.0);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                // Object types: sort by SymbolId (declaration order), then by ShapeId
                (TypeData::Object(s1), TypeData::Object(s2))
                | (TypeData::ObjectWithIndex(s1), TypeData::ObjectWithIndex(s2))
                | (TypeData::Object(s1), TypeData::ObjectWithIndex(s2))
                | (TypeData::ObjectWithIndex(s1), TypeData::Object(s2)) => {
                    let shape1 = self.object_shape(*s1);
                    let shape2 = self.object_shape(*s2);
                    if let (Some(sym1), Some(sym2)) = (shape1.symbol, shape2.symbol) {
                        let cmp = sym1.0.cmp(&sym2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    // For anonymous objects (no symbol), use ShapeId (allocation order,
                    // which follows source encounter order for structurally distinct objects)
                    if shape1.symbol.is_none() && shape2.symbol.is_none() {
                        let cmp = s1.0.cmp(&s2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                // Callable types: sort by SymbolId (declaration order)
                (TypeData::Callable(s1), TypeData::Callable(s2)) => {
                    let shape1 = self.callable_shape(*s1);
                    let shape2 = self.callable_shape(*s2);
                    if let (Some(sym1), Some(sym2)) = (shape1.symbol, shape2.symbol) {
                        let cmp = sym1.0.cmp(&sym2.0);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
                // Application types (generic instantiations like I1<number>):
                // sort by base type first (which typically resolves to a Lazy(DefId)),
                // then by type arguments lexicographically.
                (TypeData::Application(app1), TypeData::Application(app2)) => {
                    let a1 = self.type_application(*app1);
                    let a2 = self.type_application(*app2);
                    let cmp = self.compare_union_members(a1.base, a2.base);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                    // Same base type — compare args lexicographically
                    for (arg1, arg2) in a1.args.iter().zip(a2.args.iter()) {
                        let cmp = self.compare_union_members(*arg1, *arg2);
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    let cmp = a1.args.len().cmp(&a2.args.len());
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                _ => {}
            }
        }
        // Fallback: compare by allocation order (monotonic counter).
        // This approximates tsc's type ID allocation order, unlike raw TypeId
        // comparison which is hash-dependent due to the sharded interner.
        let order_a = self.lookup_alloc_order(a);
        let order_b = self.lookup_alloc_order(b);
        match (order_a, order_b) {
            (Some(oa), Some(ob)) => oa.cmp(&ob),
            // Intrinsic types have no alloc_order entry; use raw TypeId
            _ => a.0.cmp(&b.0),
        }
    }

    pub(super) fn normalize_union(&self, mut flat: TypeListBuffer) -> TypeId {
        // Deduplicate and sort for consistent identity.
        // Sort order uses semantic comparison to match tsc's union display.
        self.sort_union_members(&mut flat);
        flat.dedup();

        // Single-pass scan for special sentinel types instead of multiple contains() calls.
        // Each contains() is O(N); scanning once is O(N) total instead of O(4N).
        let mut has_error = false;
        let mut has_any = false;
        let mut has_unknown = false;
        let mut has_never = false;
        for &id in flat.iter() {
            if id == TypeId::ERROR {
                has_error = true;
                break; // ERROR trumps everything
            }
            if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            } else if id == TypeId::NEVER {
                has_never = true;
            }
        }
        if has_error {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        if has_any {
            return TypeId::ANY;
        }
        if has_unknown {
            return TypeId::UNKNOWN;
        }
        // Remove `never` from unions (only scan if we found any)
        if has_never {
            flat.retain(|id| *id != TypeId::NEVER);
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Absorb literal types into their corresponding primitive types
        // e.g., "a" | string | number => string | number
        // e.g., 1 | 2 | number => number
        // e.g., true | boolean => boolean
        self.absorb_literals_into_primitives(&mut flat);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Large object unions are expensive to subtype-reduce (O(n²)), but they are
        // still valid types. Preserve them and skip subtype reduction instead of
        // collapsing the whole union to `error`, which poisons downstream computed
        // types such as `keyof BigUnion` and `BigUnion["name"]`.
        if flat.len() > 1000 {
            let has_object_types = flat.iter().any(|&id| {
                matches!(
                    self.lookup(id),
                    Some(
                        TypeData::Object(_)
                            | TypeData::ObjectWithIndex(_)
                            | TypeData::Intersection(_)
                    )
                )
            });
            if has_object_types {
                // TS2590: Check the same pairwise complexity threshold as
                // reduce_union_subtypes. tsc bails at 1,000,000 pairwise iterations.
                let len = flat.len() as u64;
                if len.saturating_mul(len.saturating_sub(1)) >= 1_000_000 {
                    self.set_union_too_complex();
                }
                return self.normalize_union_literal_only(flat);
            }
        }

        // Reduce union using subtype checks (e.g., {a: 1} | {a: 1 | number} => {a: 1 | number})
        // Skip reduction if union contains complex types (TypeParameters, Lazy, etc.)
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::TypeParameter(_) | TypeData::Lazy(_))
            )
        });
        if !has_complex {
            self.reduce_union_subtypes(&mut flat);
        }

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Normalize a union with literal-only reduction (no subtype reduction).
    ///
    /// This matches tsc's `UnionReduction.Literal` behavior. It performs all the
    /// same normalization as `normalize_union` (sort, dedup, special cases, literal
    /// absorption) but skips the `reduce_union_subtypes` step.
    fn normalize_union_literal_only(&self, mut flat: TypeListBuffer) -> TypeId {
        self.sort_union_members(&mut flat);
        flat.dedup();

        // Single-pass scan for special sentinel types
        let mut has_error = false;
        let mut has_any = false;
        let mut has_unknown = false;
        let mut has_never = false;
        for &id in flat.iter() {
            if id == TypeId::ERROR {
                has_error = true;
                break;
            }
            if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            } else if id == TypeId::NEVER {
                has_never = true;
            }
        }
        if has_error {
            return TypeId::ERROR;
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }
        if has_any {
            return TypeId::ANY;
        }
        if has_unknown {
            return TypeId::UNKNOWN;
        }
        if has_never {
            flat.retain(|id| *id != TypeId::NEVER);
        }
        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        self.absorb_literals_into_primitives(&mut flat);

        if flat.is_empty() {
            return TypeId::NEVER;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // NOTE: No subtype reduction here — this is the key difference from normalize_union.
        // tsc's UnionReduction.Literal only absorbs literals into primitives.

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Intern an intersection type, normalizing and deduplicating members
    pub fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.intersection_from_iter(members)
    }

    /// Fast path for two-member intersections.
    pub fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.intersection_from_iter([left, right])
    }

    /// Create an intersection type WITHOUT triggering `normalize_intersection`
    ///
    /// This is a low-level operation used by the `SubtypeChecker` to merge
    /// properties from intersection members without causing infinite recursion.
    ///
    /// # Safety
    /// Only use this when you need to synthesize a type for intermediate checking.
    /// Do NOT use for final compiler output (like .d.ts generation) as the
    /// resulting type will be "unsimplified".
    pub fn intersect_types_raw(&self, members: Vec<TypeId>) -> TypeId {
        // Use SmallVec to keep stack allocation benefits
        let mut flat: TypeListBuffer = SmallVec::new();

        for member in members {
            // Structural flattening is safe and cheap
            if let Some(TypeData::Intersection(inner)) = self.lookup(member) {
                let inner_members = self.type_list(inner);
                flat.extend(inner_members.iter().copied());
            } else {
                flat.push(member);
            }
        }

        // Preserve source/declaration order of intersection members to match tsc.
        // Only perform order-preserving dedup.
        {
            let mut seen = FxHashSet::default();
            flat.retain(|id| seen.insert(*id));
        }

        // =========================================================
        // O(1) Fast Paths (Safe to do without recursion)
        // =========================================================

        // 1. If any member is Never, the result is Never
        if flat.contains(&TypeId::NEVER) {
            return TypeId::NEVER;
        }

        // 2. If any member is Any, the result is Any (unless Never is present)
        if flat.contains(&TypeId::ANY) {
            return TypeId::ANY;
        }

        // 3. Remove Unknown (Identity element for intersection)
        flat.retain(|id| *id != TypeId::UNKNOWN);

        // 4. Check for disjoint primitives (e.g., string & number = never)
        // If we have multiple intrinsic primitive types that are disjoint, return never
        if self.has_disjoint_primitives(&flat) {
            return TypeId::NEVER;
        }

        // =========================================================
        // Final Construction
        // =========================================================

        if flat.is_empty() {
            return TypeId::UNKNOWN;
        }
        if flat.len() == 1 {
            return flat[0];
        }

        // Create the intersection directly without calling normalize_intersection
        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Intersection(list_id))
    }

    /// Convenience wrapper for raw intersection of two types
    pub fn intersect_types_raw2(&self, a: TypeId, b: TypeId) -> TypeId {
        self.intersect_types_raw(vec![a, b])
    }

    pub(super) fn intersection_from_iter<I>(&self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut iter = members.into_iter();
        let Some(first) = iter.next() else {
            return TypeId::UNKNOWN;
        };
        let Some(second) = iter.next() else {
            return first;
        };

        let mut flat: TypeListBuffer = SmallVec::new();
        self.push_intersection_member(&mut flat, first);
        self.push_intersection_member(&mut flat, second);
        for member in iter {
            self.push_intersection_member(&mut flat, member);
        }

        self.normalize_intersection(flat)
    }

    pub(super) fn push_intersection_member(&self, flat: &mut TypeListBuffer, member: TypeId) {
        if let Some(TypeData::Intersection(inner)) = self.lookup(member) {
            let members = self.type_list(inner);
            flat.extend(members.iter().copied());
        } else {
            flat.push(member);
        }
    }

    // Intersection normalization, empty object elimination, callable/object
    // merging, and distribution are in `intersection.rs`.

    /// Intern an array type
    pub fn array(&self, element: TypeId) -> TypeId {
        self.intern(TypeData::Array(element))
    }

    /// Canonical `this` type.
    pub fn this_type(&self) -> TypeId {
        self.intern(TypeData::ThisType)
    }

    /// Intern a readonly array type
    /// Returns a distinct type from mutable arrays to enforce readonly semantics
    pub fn readonly_array(&self, element: TypeId) -> TypeId {
        let array_type = self.array(element);
        self.intern(TypeData::ReadonlyType(array_type))
    }

    /// Intern a tuple type.
    ///
    /// Normalizes optional element types: for `optional=true` elements, strips
    /// explicit `undefined` from union types since the optionality already implies
    /// `| undefined`. This ensures `[1, 2?]` and `[1, (2 | undefined)?]` produce
    /// the same interned TypeId, matching tsc's behavior.
    pub fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let elements = self.normalize_optional_tuple_elements(elements);
        let list_id = self.intern_tuple_list(elements);
        self.intern(TypeData::Tuple(list_id))
    }

    /// For optional tuple elements, strip `undefined` from the element type
    /// since the `optional` flag already implies `| undefined`.
    fn normalize_optional_tuple_elements(
        &self,
        mut elements: Vec<TupleElement>,
    ) -> Vec<TupleElement> {
        for elem in &mut elements {
            if elem.optional && !elem.rest {
                elem.type_id = self.strip_undefined_from_type(elem.type_id);
            }
        }
        elements
    }

    /// Remove `undefined` from a union type. If the type is not a union or
    /// doesn't contain `undefined`, returns the type unchanged.
    fn strip_undefined_from_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::UNDEFINED {
            return type_id;
        }
        if let Some(TypeData::Union(list_id)) = self.lookup(type_id) {
            let members = self.type_list(list_id);
            if members.contains(&TypeId::UNDEFINED) {
                let filtered: Vec<TypeId> = members
                    .iter()
                    .copied()
                    .filter(|&m| m != TypeId::UNDEFINED)
                    .collect();
                return match filtered.len() {
                    0 => TypeId::NEVER,
                    1 => filtered[0],
                    _ => self.union_from_sorted_vec(filtered),
                };
            }
        }
        type_id
    }

    /// Intern a readonly tuple type
    /// Returns a distinct type from mutable tuples to enforce readonly semantics
    pub fn readonly_tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let tuple_type = self.tuple(elements);
        self.intern(TypeData::ReadonlyType(tuple_type))
    }

    /// Wrap any type in a `ReadonlyType` marker
    /// This is used for the `readonly` type operator
    pub fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::ReadonlyType(inner))
    }

    /// Wrap a type in a `NoInfer` marker.
    pub fn no_infer(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::NoInfer(inner))
    }

    /// Create a `unique symbol` type for a symbol declaration.
    pub fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::UniqueSymbol(symbol))
    }

    /// Create an `infer` binder with the provided info.
    pub fn infer(&self, info: TypeParamInfo) -> TypeId {
        self.intern(TypeData::Infer(info))
    }

    pub fn bound_parameter(&self, index: u32) -> TypeId {
        self.intern(TypeData::BoundParameter(index))
    }

    pub fn recursive(&self, depth: u32) -> TypeId {
        self.intern(TypeData::Recursive(depth))
    }

    /// Wrap a type in a `KeyOf` marker.
    pub fn keyof(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::KeyOf(inner))
    }

    /// Build an indexed access type (`T[K]`).
    pub fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.intern(TypeData::IndexAccess(object_type, index_type))
    }

    /// Build a nominal enum type that preserves `DefId` identity and carries
    /// structural member information for compatibility with primitive relations.
    pub fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        self.intern(TypeData::Enum(def_id, structural_type))
    }

    /// Intern an object type with properties.
    pub fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::empty())
    }

    /// Intern a fresh object type with properties.
    pub fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::FRESH_LITERAL)
    }

    /// Intern a fresh object type with both widened properties (for type checking)
    /// and display properties (for error messages).
    ///
    /// This implements tsc's "freshness" model where object literal types
    /// preserve literal types for error display but use widened types for
    /// assignability checking.
    pub fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        // Capture display property declaration order before interning
        let mut display_props = display_properties;
        for (i, prop) in display_props.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        display_props.sort_by_key(|a| a.name);

        // Intern the widened properties as the canonical type
        let type_id = self.object_with_flags(widened_properties, ObjectFlags::FRESH_LITERAL);

        // Store display properties keyed by TypeId (not ObjectShapeId)
        self.store_display_properties(type_id, display_props);

        type_id
    }

    /// Intern an object type with properties and custom flags.
    pub fn object_with_flags(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
    ) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        // declaration_order is excluded from Hash/Eq, so it doesn't affect identity.
        for (i, prop) in properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort by property name for consistent hashing
        properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol: None,
        });
        self.intern(TypeData::Object(shape_id))
    }

    /// Intern an object type with properties, custom flags, and optional symbol.
    /// This is used for interfaces that need symbol tracking but no index signatures.
    pub fn object_with_flags_and_symbol(
        &self,
        mut properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<tsz_binder::SymbolId>,
    ) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        for (i, prop) in properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort by property name for consistent hashing
        properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(ObjectShape {
            flags,
            properties,
            string_index: None,
            number_index: None,
            symbol,
        });
        self.intern(TypeData::Object(shape_id))
    }

    /// Intern an object type with index signatures.
    pub fn object_with_index(&self, mut shape: ObjectShape) -> TypeId {
        // Capture declaration order before sorting (for display purposes).
        for (i, prop) in shape.properties.iter_mut().enumerate() {
            if prop.declaration_order == 0 {
                prop.declaration_order = (i + 1) as u32;
            }
        }
        // Sort properties by name for consistent hashing
        shape.properties.sort_by_key(|a| a.name);
        let shape_id = self.intern_object_shape(shape);
        self.intern(TypeData::ObjectWithIndex(shape_id))
    }

    /// Get the TypeId for an already-interned Object shape.
    /// This is O(1) since it's an interner cache hit.
    pub fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.intern(TypeData::Object(shape_id))
    }

    /// Get the TypeId for an already-interned `ObjectWithIndex` shape.
    pub fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.intern(TypeData::ObjectWithIndex(shape_id))
    }

    /// Intern a function type
    pub fn function(&self, shape: FunctionShape) -> TypeId {
        let shape_id = self.intern_function_shape(shape);
        self.intern(TypeData::Function(shape_id))
    }

    /// Intern a callable type with overloaded signatures
    pub fn callable(&self, shape: CallableShape) -> TypeId {
        let shape_id = self.intern_callable_shape(shape);
        self.intern(TypeData::Callable(shape_id))
    }

    /// Intern a conditional type
    pub fn conditional(&self, conditional: ConditionalType) -> TypeId {
        let conditional_id = self.intern_conditional_type(conditional);
        self.intern(TypeData::Conditional(conditional_id))
    }

    /// Intern a mapped type
    pub fn mapped(&self, mapped: MappedType) -> TypeId {
        let mapped_id = self.intern_mapped_type(mapped);
        self.intern(TypeData::Mapped(mapped_id))
    }

    /// Build a string intrinsic (`Uppercase`, `Lowercase`, etc.) marker.
    pub fn string_intrinsic(
        &self,
        kind: crate::types::StringIntrinsicKind,
        type_arg: TypeId,
    ) -> TypeId {
        self.intern(TypeData::StringIntrinsic { kind, type_arg })
    }

    /// Intern a type reference (deprecated - use `lazy()` with `DefId` instead).
    ///
    /// This method is kept for backward compatibility with tests and legacy code.
    /// It converts `SymbolRef` to `DefId` and creates `TypeData::Lazy`.
    ///
    /// Deprecated: new code should use `lazy(def_id)` instead.
    pub fn reference(&self, symbol: SymbolRef) -> TypeId {
        // Convert SymbolRef to DefId by wrapping the raw u32 value
        // This maintains the same identity while using the new TypeData::Lazy variant
        let def_id = DefId(symbol.0);
        self.intern(TypeData::Lazy(def_id))
    }

    /// Intern a lazy type reference (DefId-based).
    ///
    /// This is the replacement for `reference()` that uses Solver-owned
    /// `DefIds` instead of Binder-owned `SymbolRefs`.
    ///
    /// Use this method for all new type references
    /// to enable O(1) type equality across Binder and Solver boundaries.
    pub fn lazy(&self, def_id: DefId) -> TypeId {
        self.intern(TypeData::Lazy(def_id))
    }

    /// Intern a type parameter.
    pub fn type_param(&self, info: TypeParamInfo) -> TypeId {
        self.intern(TypeData::TypeParameter(info))
    }

    /// Intern a type query (`typeof value`) marker.
    pub fn type_query(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::TypeQuery(symbol))
    }

    /// Intern a module namespace type.
    pub fn module_namespace(&self, symbol: SymbolRef) -> TypeId {
        self.intern(TypeData::ModuleNamespace(symbol))
    }

    /// Intern a generic type application
    pub fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        let app_id = self.intern_application(TypeApplication { base, args });
        self.intern(TypeData::Application(app_id))
    }

    /// Estimated in-memory size of the entire type interner in bytes.
    ///
    /// This is a best-effort heuristic for memory pressure tracking and
    /// eviction decisions in the LSP. It reads only atomic counters and
    /// `DashMap::len()` calls — no per-entry iteration.
    ///
    /// The estimate accounts for:
    /// - Per-type overhead in sharded storage (two `DashMap` entries per type)
    /// - Sub-interners for type lists, tuple lists, template lists, shapes
    /// - Auxiliary caches (`identity_comparable`, `alloc_order`, `display_properties`)
    /// - Fixed-size fields (`array_base_type`, `boxed_types`, etc.)
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // --- Sharded type storage ---
        // Each interned type lives in a DashMap (key_to_index) and a flat Vec (index_to_key).
        // DashMap overhead per entry is roughly 64 bytes (bucket + hash + padding).
        // TypeData is Copy and small (~32 bytes), stored inline.
        const DASHMAP_ENTRY_OVERHEAD: usize = 64;
        let type_data_size = std::mem::size_of::<TypeData>();
        // key_to_index: DashMap<TypeData, u32> + index_to_key: Vec<TypeData>
        let per_type_cost = (DASHMAP_ENTRY_OVERHEAD + type_data_size + 4) + type_data_size;

        let type_count = self.len();
        size += type_count * per_type_cost;

        // Shard Vec allocation
        size += self.shards.capacity() * std::mem::size_of::<TypeShard>();

        // --- Slice interners (type_lists, tuple_lists, template_lists) ---
        // Each entry: two DashMap entries (id->Arc<[T]> and Arc<[T]>->id) + Arc heap alloc.
        // Average slice length is ~3 elements for type lists, ~2 for tuples/templates.
        let type_list_count = self.type_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_type_list_elements = 3usize;
        size += type_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TypeId]>>()
                + avg_type_list_elements * std::mem::size_of::<TypeId>());

        let tuple_list_count = self.tuple_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_tuple_elements = 2usize;
        size += tuple_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TupleElement]>>()
                + avg_tuple_elements * std::mem::size_of::<TupleElement>());

        let template_list_count = self.template_lists.next_id.load(Ordering::Relaxed) as usize;
        let avg_template_elements = 2usize;
        size += template_list_count
            * (2 * DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<Arc<[TemplateSpan]>>()
                + avg_template_elements * std::mem::size_of::<TemplateSpan>());

        // --- Value interners (object/function/callable/conditional/mapped/application shapes) ---
        // Each entry: two DashMap entries + Arc<T> heap alloc.
        let value_interner_cost = |count: usize, value_size: usize| -> usize {
            count * (2 * DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<usize>() * 2 + value_size)
        };

        size += value_interner_cost(
            self.object_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ObjectShape>(),
        );
        size += value_interner_cost(
            self.function_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<FunctionShape>(),
        );
        size += value_interner_cost(
            self.callable_shapes.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<CallableShape>(),
        );
        size += value_interner_cost(
            self.conditional_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<ConditionalType>(),
        );
        size += value_interner_cost(
            self.mapped_types.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<MappedType>(),
        );
        size += value_interner_cost(
            self.applications.next_id.load(Ordering::Relaxed) as usize,
            std::mem::size_of::<TypeApplication>(),
        );

        // --- Auxiliary caches ---
        size += self.identity_comparable_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() + 1);
        // alloc_order is now stored per-shard alongside index_to_key (4 bytes per type)
        size += type_count * 4;
        size += self.display_properties.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<Arc<Vec<PropertyInfo>>>());
        size +=
            self.display_alias.len() * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() * 2);
        size += self.boxed_types.len() * (DASHMAP_ENTRY_OVERHEAD + 16);
        size += self.boxed_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 32);
        size += self.this_type_marker_def_ids.len() * (DASHMAP_ENTRY_OVERHEAD + 8);

        // Object property map index (if initialized)
        if let Some(prop_map) = self.object_property_maps.get() {
            size += prop_map.len() * (DASHMAP_ENTRY_OVERHEAD + 128);
        }

        size
    }
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}
