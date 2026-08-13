//! Type construction convenience methods for `TypeInterner`.
//!
//! This module contains all the builder/factory methods for creating
//! interned types: literals, unions, intersections, objects, functions, etc.

use super::interner::{PredicateCacheEntry, TypeInterner, TypeListBuffer, TypeShard};
use crate::def::DefId;
use crate::types::{
    CallableShape, ConditionalType, FunctionShape, FunctionShapeId, IntrinsicKind, LiteralValue,
    MappedType, ObjectFlags, ObjectShape, ObjectShapeId, OrderedFloat, PropertyInfo, SymbolRef,
    TemplateSpan, TupleElement, TypeApplication, TypeData, TypeId, TypeParamInfo,
    normalize_display_property_order,
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

    /// Intern a literal number type.
    ///
    /// Number literal types are keyed by `SameValueZero`, matching tsc's
    /// `getNumberLiteralType` map: `-0` and `0` intern to the same type
    /// (`const a: -0 = 0` is clean and `-0` displays as `0`), and every NaN
    /// bit pattern collapses to the canonical NaN. `OrderedFloat` compares by
    /// raw bits, so both must be normalized before interning.
    pub fn literal_number(&self, value: f64) -> TypeId {
        let value = OrderedFloat::same_value_zero_canonical(value);
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
    /// Full normalization including construction-time subtype reduction (tsc's
    /// `UnionReduction.Subtype`); literal-mode under the default-OFF
    /// `TSZ_UNION_LITERAL_DEFAULT` flag (#15809, see `intern::union_mode`).
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

        // Non-strict absorption applies at every union seam, not just the
        // normalizing ones, so a sorted-and-deduped list built by narrowing or
        // object-literal construction reaches the same canonical identity as the
        // `normalize_union` path. `retain` preserves the sort order. Gated on the
        // mode so a strict run keeps its byte-identical fast path.
        if !self.strict_null_checks() {
            let mut reduced: TypeListBuffer = flat.iter().copied().collect();
            if let Some(collapsed) = self.reduce_and_collapse_nonstrict(&mut reduced) {
                return collapsed;
            }
            let list_id = self.intern_type_list_from_slice(&reduced);
            return self.intern(TypeData::Union(list_id));
        }

        let list_id = self.intern_type_list(flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Intern a union type while preserving member structure.
    ///
    /// Unlike [`union`](Self::union), this skips structural subtype reduction, so
    /// distinct members that merely share an interface (e.g. `Derived1 | Derived2`)
    /// stay separate — narrowing and property-access need that structure.
    ///
    /// It still applies the *universal* absorptions that hold for every union
    /// regardless of member structure: the top/bottom sentinels. A union that
    /// contains `error`, `any`, or `unknown` collapses to that type (precedence
    /// `error` > `any` > `unknown`), exactly as `normalize_union` does, because
    /// `unknown | T` is `unknown`, `any | T` is `any`, and `error | T` is `error`
    /// for all `T`. Without this, flow narrowing combiners (e.g. the false branch
    /// of `typeof x === "object" && x !== null` on an `unknown`) produced a
    /// non-normalized `unknown | null`, which then mis-narrowed under a later
    /// `typeof` guard. `never` members are dropped (the identity of union).
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

        // Absorb the universal sentinels before structural interning. These hold
        // for any member set, so scanning the flattened list (which has already
        // expanded nested unions) catches a sentinel nested inside a member union.
        let mut has_any = false;
        let mut has_unknown = false;
        for &id in &flat {
            if id == TypeId::ERROR {
                return TypeId::ERROR; // error trumps everything
            }
            if id == TypeId::ANY {
                has_any = true;
            } else if id == TypeId::UNKNOWN {
                has_unknown = true;
            }
        }
        if has_any {
            return TypeId::ANY;
        }
        if has_unknown {
            return TypeId::UNKNOWN;
        }

        self.sort_union_members(&mut flat);
        flat.dedup();
        flat.retain(|id| *id != TypeId::NEVER);

        // The structure this path preserves is *subtype* structure
        // (`Derived1 | Derived2` stays split for narrowing); non-strict nullish
        // absorption is orthogonal and still applies (tsc's narrowing unions run
        // the same `addTypeToUnion` exclusion), holding one-universe identity
        // against `normalize_union`. The shared helper also folds the post-`never`
        // length collapse this path already needed.
        if let Some(collapsed) = self.reduce_and_collapse_nonstrict(&mut flat) {
            return collapsed;
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
        // With `strictNullChecks` off, union construction drops `null`/`undefined`,
        // so the identity fast paths below — which assume both operands survive —
        // do not hold; route the pair through the normalizing constructor, which
        // applies the drop (`reduce_nonstrict_nullish_members`). Strict is
        // unchanged.
        if !self.strict_null_checks() {
            return self.union_from_iter([left, right]);
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

    /// Fast path for `union2` that resolves *only* the cases where adding
    /// `single` to `existing` leaves the union unchanged, so the already-interned
    /// `existing` `TypeId` can be returned without re-normalizing.
    ///
    /// Returns `Some(existing)` for those no-op cases and `None` otherwise, which
    /// routes the pair through `union2`'s documented contract — `union_from_iter`
    /// → `normalize_union` (the same path the non-fast branch takes).
    ///
    /// It deliberately does **not** try to compute an inserted-member result
    /// itself. Doing so requires reproducing `normalize_union`'s content-canonical
    /// member ordering *and* its literal/subtype reduction, and the previous
    /// hand-rolled insertion did neither: it ordered non-builtin members by
    /// allocation order (thread- and interning-order dependent, not content), and
    /// it skipped both primitive-absorbs-literal and structural subtype reduction.
    /// That minted a different `TypeId` than `union` for the same member set —
    /// breaking the one-semantic-universe identity invariant. Only the two
    /// no-change cases below are canonical by construction, so only they stay on
    /// the fast path.
    fn try_union2_insert(&self, single: TypeId, existing: TypeId) -> Option<TypeId> {
        // single must not be a special type
        if single == TypeId::ANY
            || single == TypeId::UNKNOWN
            || single == TypeId::ERROR
            || single == TypeId::NEVER
        {
            return None;
        }

        // single must not be a union.
        let single_data = self.lookup(single);
        if matches!(&single_data, Some(TypeData::Union(_))) {
            return None;
        }

        // existing must be a union.
        let Some(TypeData::Union(list_id)) = self.lookup(existing) else {
            return None;
        };
        let members = self.type_list(list_id);

        // No-change case 1: `single` is a literal absorbed by a primitive already
        // in the union (`"hello" | string` → `string`). The primitive is present,
        // so the normalized member set is exactly `existing`.
        if let Some(TypeData::Literal(lit)) = single_data
            && members.contains(&lit.primitive_type_id())
        {
            return Some(existing);
        }

        // No-change case 2: `single` is already a member (dedup).
        if members.contains(&single) {
            return Some(existing);
        }

        // Any genuine insertion may reorder or reduce the member set; defer to the
        // canonical normalizer so `union2 == union` holds.
        None
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

    /// Memoized union normalization.
    ///
    /// The full pipeline (callable-order probe, semantic sort, dedup, literal
    /// absorption, enum merge, intersection absorption, subtype reduction,
    /// interning) is deterministic in the flattened input list over immutable
    /// interned types, and evaluation hot paths rebuild the same unions
    /// constantly (the interner sees ~97% repeat hits on type-level-heavy
    /// projects). Key is the exact pre-normalization member list, so repeats
    /// skip straight to the previously interned result.
    pub(super) fn normalize_union(&self, mut flat: TypeListBuffer) -> TypeId {
        // Non-strict nullish absorption runs before the length gate and the result
        // memo, so the cache key is the *reduced* member set: a `strictNullChecks`-off
        // run keys `[number]` where a strict run keys `[number, null]`, never colliding.
        if let Some(collapsed) = self.reduce_and_collapse_nonstrict(&mut flat) {
            return collapsed;
        }
        // Union construction-mode campaign (#15809): under `TSZ_UNION_LITERAL_DEFAULT`
        // the constructor is literal-mode and skips its construction-time pairwise
        // subtype sweep (rationale in `intern::union_mode`). Read once here — the flag
        // is a process-global constant, so the member-keyed `union_normalize_cache`
        // never mixes modes; default-OFF keeps the historical reduction byte-identical.
        let literal_only = crate::intern::union_literal_default_enabled();
        if flat.len() > Self::UNION_NORMALIZE_CACHE_MAX_LEN {
            return self.normalize_union_uncached(flat, literal_only);
        }
        if let Some(hit) = self.union_normalize_cache.get(flat.as_slice()) {
            return *hit;
        }
        let key: Box<[TypeId]> = flat.as_slice().into();
        let result = self.normalize_union_uncached(flat, literal_only);
        self.insert_union_normalize_cache(key, result);
        result
    }

    fn normalize_union_uncached(&self, mut flat: TypeListBuffer, literal_only: bool) -> TypeId {
        // Callable unions feed signature-combining diagnostics, where tsc preserves
        // the declaration/indexed-access order for intersected parameter display.
        // The normal semantic union sort can invert class-backed function members
        // such as `Node | Mark`, producing `Mark & Node` fingerprints.
        let preserve_callable_order = self.should_preserve_callable_union_order(&flat);
        if preserve_callable_order {
            let mut seen = FxHashSet::default();
            flat.retain(|id| seen.insert(*id));
        } else {
            // Deduplicate and sort for consistent identity.
            // Sort order uses semantic comparison to match tsc's union display.
            self.sort_union_members(&mut flat);
            flat.dedup();
        }

        // Literal ladder (tsc `UnionReduction.Literal`): sentinel handling,
        // `never` removal, and literal→primitive/enum/intersection absorption.
        // Shared with `normalize_union_literal_only`; the `enriched` flag keeps
        // the two absorptions that the literal-only path historically omitted.
        if let Some(collapsed) = self.apply_union_literal_ladder(&mut flat, true) {
            return collapsed;
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
                // Skipping subtype reduction here is an internal representation
                // choice, not a TS2590 condition. Explicit large discriminated
                // unions such as `BigUnion` remain representable and are used by
                // indexed access and conditional-type helpers downstream.
                return self.normalize_union_literal_only(flat);
            }
        }

        // Reduce union using subtype checks (e.g., {a: 1} | {a: 1 | number} => {a: 1 | number})
        // Skip reduction wholesale only when the union contains a `TypeParameter`
        // or `Lazy` member: those interact with reduction non-locally (an
        // unresolved type parameter can stand for a supertype/subtype of any
        // concrete peer, and a `Lazy` ref names a symbol whose shape is not yet
        // available), so reducing the surrounding concrete members against an
        // arbitrary instantiation is unsound. Unevaluated *deferred operations*
        // (`Conditional` / `IndexAccess`) are NOT skipped here: they are inert in
        // the shallow engine (never relate as source or target except by
        // identity), so `reduce_union_subtypes` short-circuits the pairwise work
        // over them internally while still reducing the concrete members beside
        // them — which is what tsc does when distributing `Exclude`/`Extract`
        // over a wide union (the deferred arms stay lazy; the concrete arms still
        // collapse). Folding them into a whole-union skip dropped legitimate
        // concrete-vs-concrete reduction (e.g. JSX intrinsic-element props unions
        // carrying an `IndexAccess` member), changing the union result.
        let has_complex = flat.iter().any(|&id| {
            matches!(
                self.lookup(id),
                Some(TypeData::TypeParameter(_) | TypeData::Lazy(_))
            )
        });
        // `literal_only` (#15809, tsc `UnionReduction.Literal`) skips the pairwise sweep.
        if !has_complex && !preserve_callable_order && !literal_only {
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

    fn should_preserve_callable_union_order(&self, flat: &TypeListBuffer) -> bool {
        let mut callable_count = 0;
        for &id in flat.iter() {
            if id == TypeId::NULL || id == TypeId::UNDEFINED || id == TypeId::NEVER {
                continue;
            }
            match self.lookup(id) {
                Some(TypeData::Function(func_id)) => {
                    if !self.function_shape(func_id).type_params.is_empty() {
                        return false;
                    }
                    callable_count += 1;
                }
                Some(TypeData::Callable(callable_id)) => {
                    let shape = self.callable_shape(callable_id);
                    if shape.call_signatures.len() != 1
                        || !shape.construct_signatures.is_empty()
                        || !shape.call_signatures[0].type_params.is_empty()
                    {
                        return false;
                    }
                    callable_count += 1;
                }
                _ => return false,
            }
        }
        callable_count > 1
    }

    fn absorb_intersections_with_union_constituents(&self, flat: &mut TypeListBuffer) {
        if flat.len() <= 1 {
            return;
        }

        let present: FxHashSet<TypeId> = flat.iter().copied().collect();
        flat.retain(|id| {
            let Some(TypeData::Intersection(list_id)) = self.lookup(*id) else {
                return true;
            };
            let parts = self.type_list(list_id);
            !parts.iter().any(|part| present.contains(part))
        });
    }

    /// Shared literal ladder for tsc's `UnionReduction.Literal` normalization,
    /// applied by both `normalize_union` (the default path, before optional
    /// subtype reduction) and `normalize_union_literal_only`.
    ///
    /// Callers pass `flat` already deduplicated (and, for the default path,
    /// sorted). The ladder scans for sentinel members (`error`/`any`/`unknown`),
    /// strips `never`, and absorbs literals into their primitives
    /// (`"a" | string` → `string`). When `enriched` is set it additionally
    /// merges split enum parts (`E.a | E.b` → `E`) and drops intersections whose
    /// parts already appear as union constituents (`A | (A & B)` → `A | (A & B)`
    /// keeps `A`; `A | (A & …)` with `A` present drops the intersection) — the
    /// two steps `normalize_union_literal_only` historically omitted. Returns
    /// `Some(id)` when the union collapses to a single terminal type, otherwise
    /// `None` with `flat` holding the ≥2 normalized members.
    fn apply_union_literal_ladder(
        &self,
        flat: &mut TypeListBuffer,
        enriched: bool,
    ) -> Option<TypeId> {
        // Single-pass scan for special sentinel types instead of multiple
        // contains() calls. Each contains() is O(N); scanning once is O(N).
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
            return Some(TypeId::ERROR);
        }
        if flat.is_empty() {
            return Some(TypeId::NEVER);
        }
        if flat.len() == 1 {
            return Some(flat[0]);
        }
        if has_any {
            return Some(TypeId::ANY);
        }
        if has_unknown {
            return Some(TypeId::UNKNOWN);
        }
        // Remove `never` from unions (only scan if we found any)
        if has_never {
            flat.retain(|id| *id != TypeId::NEVER);
        }
        if flat.is_empty() {
            return Some(TypeId::NEVER);
        }
        if flat.len() == 1 {
            return Some(flat[0]);
        }

        // Absorb literal types into their corresponding primitive types
        // e.g., "a" | string | number => string | number
        // e.g., 1 | 2 | number => number
        // e.g., true | boolean => boolean
        self.absorb_literals_into_primitives(flat);
        if enriched {
            // Merge Enum(D, X) | Enum(D, Y) → Enum(D, X | Y) so that split-then-
            // rejoined enum members (e.g., E1.a | E1.b) display as E1, not E1 | E1.
            self.merge_same_enum_parts(flat);
            self.absorb_intersections_with_union_constituents(flat);
        }

        if flat.is_empty() {
            return Some(TypeId::NEVER);
        }
        if flat.len() == 1 {
            return Some(flat[0]);
        }
        None
    }

    /// Normalize a union with literal-only reduction (no subtype reduction).
    ///
    /// This matches tsc's `UnionReduction.Literal` behavior. It performs all the
    /// same normalization as `normalize_union` (sort, dedup, special cases, literal
    /// absorption) but skips the `reduce_union_subtypes` step. The enum-merge and
    /// intersection-absorption steps of the shared ladder are gated behind the
    /// `TSZ_UNION_LITERAL_DEFAULT` flag so this path (reached from the >1000-member
    /// object-union guard in `normalize_union`) stays byte-identical when OFF.
    fn normalize_union_literal_only(&self, mut flat: TypeListBuffer) -> TypeId {
        // The `.Literal`-equivalent path drops non-strict nullish too: tsc's
        // `addTypeToUnion` exclusion is independent of the reduction mode.
        if let Some(collapsed) = self.reduce_and_collapse_nonstrict(&mut flat) {
            return collapsed;
        }
        self.sort_union_members(&mut flat);
        flat.dedup();

        let enriched = crate::intern::union_literal_default_enabled();
        if let Some(collapsed) = self.apply_union_literal_ladder(&mut flat, enriched) {
            return collapsed;
        }

        // NOTE: No subtype reduction here — this is the key difference from normalize_union.
        // tsc's UnionReduction.Literal only absorbs literals into primitives.

        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Test hook: run the shared literal ladder directly with an explicit
    /// `enriched` toggle, bypassing the process-global `TSZ_UNION_LITERAL_DEFAULT`
    /// `OnceLock` so both the legacy (literal-absorb-only) and enriched
    /// (enum-merge + intersection-absorb) paths are exercised deterministically.
    #[cfg(test)]
    pub(crate) fn union_literal_ladder_for_test(
        &self,
        members: Vec<TypeId>,
        enriched: bool,
    ) -> TypeId {
        let mut flat: TypeListBuffer = members.into_iter().collect();
        self.sort_union_members(&mut flat);
        flat.dedup();
        if let Some(collapsed) = self.apply_union_literal_ladder(&mut flat, enriched) {
            return collapsed;
        }
        let list_id = self.intern_type_list_from_slice(&flat);
        self.intern(TypeData::Union(list_id))
    }

    /// Test hook: normalize a union with an explicit reduction mode, bypassing the
    /// process-global `TSZ_UNION_LITERAL_DEFAULT` `OnceLock` so both the historical
    /// (`literal_only = false`) and campaign (`true`) #15809 paths run deterministically.
    #[cfg(test)]
    pub(crate) fn normalize_union_for_test(
        &self,
        members: Vec<TypeId>,
        literal_only: bool,
    ) -> TypeId {
        let mut flat: TypeListBuffer = members.into_iter().collect();
        if let Some(collapsed) = self.reduce_and_collapse_nonstrict(&mut flat) {
            return collapsed;
        }
        self.normalize_union_uncached(flat, literal_only)
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
        self.intersect_types_raw_impl(members, true)
    }

    /// Replay an existing raw intersection without emitting a new complexity
    /// signal.
    ///
    /// Exact identity substitution preserves already-admitted structure; it is
    /// not a new semantic intersection request. This constructor therefore has
    /// the same `O(N)` flattening, order-preserving deduplication, sentinel
    /// handling, and interning behavior as [`Self::intersect_types_raw`], but it
    /// never sets the interner-wide `union_too_complex` diagnostic flag.
    pub fn intersect_types_raw_for_replay(&self, members: Vec<TypeId>) -> TypeId {
        self.intersect_types_raw_impl(members, false)
    }

    fn intersect_types_raw_impl(
        &self,
        members: Vec<TypeId>,
        signal_cross_product_complexity: bool,
    ) -> TypeId {
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

        // 0. If any member is the `error` sentinel, the result is `error`.
        // `error` is contagious and absorbs every other member (precedence
        // `error` > `never` > `any`), exactly as the full `normalize_intersection`
        // path does. Skipping this here let `error` survive *inside* an
        // `Intersection(error, T)` node, which the top-level error predicates
        // (`is_error_type`, the `== TypeId::ERROR` fast paths) do not see — so the
        // leaked `error` silently defeated error suppression and seeded cascading
        // false positives downstream.
        if flat.contains(&TypeId::ERROR) {
            return TypeId::ERROR;
        }

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

        // TS2590: Cross-product union size check for raw intersections.
        // When an intersection contains union members, the cross-product
        // can grow exponentially. tsc bails at 100,000 constituents.
        if signal_cross_product_complexity {
            let mut cross_product_size: u64 = 1;
            for &id in flat.iter() {
                if let Some(TypeData::Union(members)) = self.lookup(id) {
                    cross_product_size =
                        cross_product_size.saturating_mul(self.type_list(members).len() as u64);
                    if cross_product_size >= 100_000 {
                        self.set_union_too_complex();
                        break;
                    }
                }
            }
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

    /// Write type for a property produced by intersecting two object members.
    ///
    /// A writable merged property keeps `write == read` (`read_type`), so it is
    /// never mistaken for a split accessor — otherwise `has_split_accessor()`
    /// would fire a spurious contravariant write check (issue #11323). Only a
    /// fully-readonly property intersects its stored setter types. Shared by all
    /// object/callable property-merge sites to keep them consistent.
    #[inline]
    pub fn merged_property_write_type(
        &self,
        readonly: bool,
        read_type: TypeId,
        existing_write: TypeId,
        prop_write: TypeId,
    ) -> TypeId {
        if readonly {
            self.intersect_types_raw2(existing_write, prop_write)
        } else {
            read_type
        }
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
        self.readonly_type(self.array(element))
    }

    /// Splice spread elements whose type is a *concrete tuple* inline, matching
    /// tsc's `createNormalizedTupleType`.
    ///
    /// A rest element `...X` where `X` resolves (through `readonly`) to a
    /// `Tuple` contributes a statically known run of elements, so `[A, ...[B, C]]`
    /// is exactly `[A, B, C]`. tsc always flattens these — at construction and
    /// after instantiation — so leaving the spread un-inlined makes the
    /// represented length, element-by-index access, relations, and display all
    /// disagree with the flattened form (the value position would index the
    /// whole inner tuple instead of its member).
    ///
    /// A **fixed** inner tuple (no rest of its own) is always spliced. A
    /// **variadic** inner tuple (carrying its own rest, e.g. `[B, ...C[]]`) is
    /// spliced only when it is the parent's sole rest element, so the result
    /// still carries at most one rest — the inner rest simply replaces the
    /// parent's. This covers recursive list utilities (`[H, ...Util<R>]`) and
    /// middle-position spreads with a fixed suffix (`[...S, number]` with `S`
    /// instantiated to `[string, ...string[]]`, which tsc normalizes to
    /// `[string, ...string[], number]`). A middle-position inner tuple that
    /// itself carries more than one rest (parser-recovered shapes) is left as
    /// written. Rest *arrays* (`...X[]`) and generic
    /// spreads (`...T`, `...Application`, `...Lazy`) are left as written; a
    /// self-referential `type T = [x, ...T]` never resolves its spread to a
    /// concrete tuple, so this cannot recurse. Optional outer spreads are left
    /// untouched. Inner elements are already normalized because every tuple is
    /// built through this same path, so a single pass is idempotent.
    fn splice_concrete_tuple_spreads(&self, elements: Vec<TupleElement>) -> Vec<TupleElement> {
        // A lone `[...X]` (sole element, a rest) is left compressed: it already
        // denotes exactly `X`, and inlining it would eagerly expand a possibly
        // huge instantiated spread (`[...T]` with `T` a large tuple) with no
        // structural benefit — index/length/relation queries already descend
        // through the single rest. Splicing targets `[head, ..., ...inner]`
        // shapes (recursive list utilities), which always carry a fixed prefix.
        if elements.len() == 1 && elements[0].rest {
            return elements;
        }
        // Single pass over the elements: count rests (needed to keep the result
        // single-rest) and learn whether any spread is a concrete tuple worth
        // inlining. The common case (a plain fixed tuple with no rest) bails
        // here without a second scan.
        let mut parent_rest_count = 0usize;
        let mut has_spliceable = false;
        for elem in &elements {
            if elem.rest {
                parent_rest_count += 1;
                if !elem.optional && self.concrete_tuple_list(elem.type_id).is_some() {
                    has_spliceable = true;
                }
            }
        }
        if !has_spliceable {
            return elements;
        }
        let last_index = elements.len().saturating_sub(1);
        let mut out: Vec<TupleElement> = Vec::with_capacity(elements.len());
        for (index, elem) in elements.into_iter().enumerate() {
            if elem.rest
                && !elem.optional
                && let Some(inner_list) = self.concrete_tuple_list(elem.type_id)
            {
                let inner = self.tuple_list(inner_list);
                let (inner_rest_count, inner_has_optional) =
                    inner.iter().fold((0usize, false), |(rests, optional), e| {
                        (rests + usize::from(e.rest), optional || e.optional)
                    });
                // Splicing into a middle position must not move an optional
                // element in front of the parent's required suffix (an illegal
                // tuple shape), so only rest+required inner shapes splice there.
                let splice_middle = inner_rest_count == 1 && !inner_has_optional;
                let splice_variadic =
                    parent_rest_count == 1 && (index == last_index || splice_middle);
                if inner_rest_count == 0 || splice_variadic {
                    out.extend(inner.iter().copied());
                    continue;
                }
            }
            out.push(elem);
        }
        out
    }

    /// If `type_id` (after unwrapping `readonly`) is a `Tuple`, return its
    /// element-list id; otherwise `None`. The list may itself be variadic — the
    /// caller decides whether splicing it is safe.
    fn concrete_tuple_list(&self, type_id: TypeId) -> Option<crate::types::TupleListId> {
        let unwrapped = match self.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner)) => inner,
            _ => type_id,
        };
        match self.lookup(unwrapped) {
            Some(TypeData::Tuple(list_id)) => Some(list_id),
            _ => None,
        }
    }

    /// Intern a tuple type.
    ///
    /// Normalizes optional element types: when exact optional properties are
    /// disabled, strips explicit `undefined` from `optional=true` union types
    /// since optionality already implies `| undefined`.
    pub fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        let elements = self.splice_concrete_tuple_spreads(elements);
        let elements = self.normalize_optional_tuple_elements(elements);
        // A single anonymous rest element wrapping an Array collapses to a plain array type.
        if elements.len() == 1
            && elements[0].rest
            && elements[0].name.is_none()
            && !elements[0].optional
            && let Some(TypeData::Array(elem)) = self.lookup(elements[0].type_id)
        {
            return self.array(elem);
        }
        let list_id = self.intern_tuple_list(elements);
        self.intern(TypeData::Tuple(list_id))
    }

    /// Like [`tuple`], but also merges consecutive concrete rest elements:
    /// `[...X[], ...Y[]]` → `(X | Y)[]`.
    ///
    /// Use this variant in the **instantiation path** where type parameters have already been
    /// substituted and adjacent rest arrays must be collapsed (matching tsc's normalization of
    /// instantiated variadic tuples).  Do **not** use this from checker code that constructs
    /// types from explicit type annotations — tsc keeps those un-normalized even when TS1265
    /// is emitted, so using `tuple()` there is the correct matching behaviour.
    pub fn tuple_normalized(&self, elements: Vec<TupleElement>) -> TypeId {
        crate::intern::tuple_normalized(self, elements)
    }

    /// For optional tuple elements, strip `undefined` from the element type
    /// unless exact optional properties require preserving a present undefined.
    fn normalize_optional_tuple_elements(
        &self,
        mut elements: Vec<TupleElement>,
    ) -> Vec<TupleElement> {
        if self.exact_optional_property_types() {
            return elements;
        }
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
            let Some(undefined_index) = members.iter().position(|&m| m == TypeId::UNDEFINED) else {
                return type_id;
            };

            let mut filtered = Vec::with_capacity(members.len() - 1);
            filtered.extend_from_slice(&members[..undefined_index]);
            filtered.extend(
                members[undefined_index + 1..]
                    .iter()
                    .copied()
                    .filter(|&m| m != TypeId::UNDEFINED),
            );
            return match filtered.len() {
                0 => TypeId::NEVER,
                1 => filtered[0],
                _ => self.union_from_sorted_vec(filtered),
            };
        }
        type_id
    }

    /// Intern a readonly tuple type
    /// Returns a distinct type from mutable tuples to enforce readonly semantics
    pub fn readonly_tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.readonly_type(self.tuple(elements))
    }

    /// Wrap any type in a `ReadonlyType` marker
    ///
    /// Invariant: at most one `ReadonlyType` layer. Callers that compose
    /// readonly wrapping (e.g. the const-assertion visitor unwrapping and
    /// re-wrapping after recursing into a Tuple/Array arm) rely on this so
    /// that subtype/display paths can peel exactly one layer.
    pub fn readonly_type(&self, inner: TypeId) -> TypeId {
        if matches!(self.lookup(inner), Some(TypeData::ReadonlyType(_))) {
            return inner;
        }
        self.intern(TypeData::ReadonlyType(inner))
    }

    /// Wrap a type in a `NoInfer` marker.
    pub fn no_infer(&self, inner: TypeId) -> TypeId {
        self.intern(TypeData::NoInfer(inner))
    }

    /// Create a substitution type `base_type` narrowed by `constraint`,
    /// mirroring `tsc`'s `getSubstitutionType`.
    ///
    /// The substitution is only retained while the narrowing is still
    /// meaningful: a trivial constraint (`any`/`unknown`/the base itself) or a
    /// fully concrete base-and-constraint pair collapses back to `base_type`, so
    /// substitution types never leak into non-generic positions where they would
    /// perturb identity or display.
    pub fn substitution(&self, base_type: TypeId, constraint: TypeId) -> TypeId {
        if constraint == base_type
            || constraint == TypeId::ANY
            || constraint == TypeId::UNKNOWN
            || constraint == TypeId::ERROR
            || base_type == TypeId::ERROR
        {
            return base_type;
        }
        // Avoid stacking substitutions: a substitution over a substitution
        // composes the constraints (the intersection observed by relations is
        // associative), keeping a single layer over the underlying variable.
        if let Some(TypeData::Substitution {
            base_type: inner_base,
            constraint: inner_constraint,
        }) = self.lookup(base_type)
        {
            let combined = self.intersection2(inner_constraint, constraint);
            return self.substitution(inner_base, combined);
        }
        // Keep the substitution only while at least one side is still generic;
        // otherwise the narrowing is fully determined (`tsc`: return baseType
        // when neither baseType nor constraint is generic).
        let keep = crate::type_queries::contains_type_parameters_db(self, base_type)
            || crate::type_queries::contains_type_parameters_db(self, constraint);
        if !keep {
            return base_type;
        }
        self.intern(TypeData::Substitution {
            base_type,
            constraint,
        })
    }

    /// The "substitution intersection" `base_type & constraint` observed by
    /// relations and base-constraint computation for a substitution type.
    /// Returns `None` when `type_id` is not a substitution type.
    pub fn substitution_intersection(&self, type_id: TypeId) -> Option<TypeId> {
        match self.lookup(type_id) {
            Some(TypeData::Substitution {
                base_type,
                constraint,
            }) => Some(self.intersection2(base_type, constraint)),
            _ => None,
        }
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
        let mut display_props = display_properties;
        normalize_display_property_order(&mut display_props);

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
            symbol_index: None,
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
            symbol_index: None,
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

    /// Get the `TypeId` for an already-interned function shape.
    pub(in crate::intern) fn function_type_from_shape_id(
        &self,
        shape_id: FunctionShapeId,
    ) -> TypeId {
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
    ///
    /// Same-kind nesting is collapsed: `Uppercase<Uppercase<T>>` → `Uppercase<T>`
    /// because each intrinsic is idempotent on its own output.
    pub fn string_intrinsic(
        &self,
        kind: crate::types::StringIntrinsicKind,
        type_arg: TypeId,
    ) -> TypeId {
        if let Some(crate::types::TypeData::StringIntrinsic {
            kind: inner_kind, ..
        }) = self.lookup(type_arg)
            && kind == inner_kind
        {
            return type_arg;
        }
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

    /// Allocate a fresh declaration-scoped type parameter.
    pub fn fresh_type_param(&self, info: TypeParamInfo) -> TypeId {
        self.intern_fresh(TypeData::TypeParameter(info))
    }

    /// Intern an unresolved type name that should behave like an error type
    /// while preserving its source spelling for diagnostics.
    pub fn unresolved_type_name(&self, name: Atom) -> TypeId {
        self.intern(TypeData::UnresolvedTypeName(name))
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
        size += self.widen_type_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() * 2);
        size += self
            .extract_type_params_cache
            .iter()
            .map(|entry| {
                DASHMAP_ENTRY_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Arc<[TypeParamInfo]>>()
                    + entry.value().len() * std::mem::size_of::<TypeParamInfo>()
            })
            .sum::<usize>();
        size += self.proto_instantiation_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<crate::caches::instantiation_cache::InstantiationCacheKey>()
                + std::mem::size_of::<TypeId>());
        let mut seen_instantiation_identity_domains = rustc_hash::FxHashSet::default();
        for entry in &self.proto_instantiation_cache {
            size += entry.key().estimated_heap_bytes(
                &mut seen_instantiation_identity_domains,
                DASHMAP_ENTRY_OVERHEAD,
            );
        }
        size += self
            .contravariant_infer_names_cache
            .iter()
            .map(|entry| {
                DASHMAP_ENTRY_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<Arc<[Atom]>>()
                    + entry.value().len() * std::mem::size_of::<Atom>()
            })
            .sum::<usize>();
        size += self.contains_type_by_id_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<(TypeId, TypeId)>() + 1);
        size += self.prune_union_members_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD + std::mem::size_of::<TypeId>() * 2);
        size += self.predicate_cache.len()
            * (DASHMAP_ENTRY_OVERHEAD
                + std::mem::size_of::<TypeId>()
                + std::mem::size_of::<PredicateCacheEntry>());
        size += self
            .union_normalize_cache
            .iter()
            .map(|entry| {
                DASHMAP_ENTRY_OVERHEAD
                    + std::mem::size_of::<TypeId>() * (entry.key().len() + 1)
                    + std::mem::size_of::<Box<[TypeId]>>()
            })
            .sum::<usize>();
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
