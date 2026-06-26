//! Union/intersection reduction and disjointness checking.
//!
//! This module contains helper methods for:
//! - Primitive disjointness detection
//! - Object literal disjointness detection
//! - Union subtype reduction
//! - Intersection subtype reduction
//! - Intersection-over-union distribution
//! - Literal absorption into primitives

use super::shallow_subtype::ShallowReduceKind;
use super::{TypeInterner, TypeListBuffer};
use crate::types::{
    IntrinsicKind, LiteralValue, PropertyInfo, TemplateLiteralId, TypeData, TypeId, Visibility,
};
use crate::visitor::is_literal_type;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tsz_common::interner::Atom;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LiteralDomain {
    String,
    Number,
    Boolean,
    Bigint,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum UnitValueKey {
    Null,
    Undefined,
    String(Atom),
    Number(u64),
    Boolean(bool),
    BigInt(Atom),
    Enum(crate::def::DefId, Box<UnitValueKey>),
}

/// Primitive kind for disjoint intersection checking.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum PrimitiveKind {
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
}

impl PrimitiveKind {
    const fn from_literal(literal: &LiteralValue) -> Self {
        match literal {
            LiteralValue::String(_) => Self::String,
            LiteralValue::Number(_) => Self::Number,
            LiteralValue::Boolean(_) => Self::Boolean,
            LiteralValue::BigInt(_) => Self::BigInt,
        }
    }
}

#[derive(Clone, Debug)]
enum LiteralKind {
    Single(LiteralValue),
    Union(LiteralDomain, FxHashSet<LiteralValue>),
}

impl LiteralKind {
    const fn domain(&self) -> LiteralDomain {
        match self {
            Self::Single(lit) => literal_domain(lit),
            Self::Union(domain, _) => *domain,
        }
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        if self.domain() != other.domain() {
            return true;
        }
        match (self, other) {
            (Self::Single(s), Self::Single(o)) => s != o,
            (Self::Single(s), Self::Union(_, set)) => !set.contains(s),
            (Self::Union(_, set), Self::Single(o)) => !set.contains(o),
            (Self::Union(_, s_set), Self::Union(_, o_set)) => {
                !s_set.iter().any(|v| o_set.contains(v))
            }
        }
    }
}

const fn literal_domain(literal: &LiteralValue) -> LiteralDomain {
    match literal {
        LiteralValue::String(_) => LiteralDomain::String,
        LiteralValue::Number(_) => LiteralDomain::Number,
        LiteralValue::Boolean(_) => LiteralDomain::Boolean,
        LiteralValue::BigInt(_) => LiteralDomain::Bigint,
    }
}

/// A single occurrence of a property name within one member of an intersection,
/// reduced to the fields the never-reduction conflict test reads.
struct PropOccurrence {
    type_id: TypeId,
    optional: bool,
    visibility: Visibility,
}

/// A literal-typed property occurrence, grouped by [`LiteralDomain`] for the
/// value-set disjointness test.
struct LiteralOccurrence {
    kind: LiteralKind,
    optional: bool,
}

/// Literal-typed occurrences of one property name, partitioned by domain. There
/// are only four literal domains, so a linear-probed `SmallVec` of `(domain,
/// group)` pairs is cheaper than a hash map.
type LiteralOccurrencesByDomain = SmallVec<[(LiteralDomain, SmallVec<[LiteralOccurrence; 4]>); 4]>;

impl TypeInterner {
    pub(crate) fn intersection_has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        let mut class: Option<PrimitiveClass> = None;
        let mut has_non_primitive = false;
        let mut literals: smallvec::SmallVec<[TypeId; 4]> = SmallVec::new();

        for &member in members {
            // If the member is an empty object type (no props or indexes), it does not conflict
            // with primitives. In TypeScript, `string & {}` is just `string`, so we must not
            // mark this as disjoint.
            let mut mark_non_primitive = false;
            // Intrinsics never resolve to Object/Function/Array/Tuple/etc.
            // — skip the dyn-dispatched lookup and leave mark_non_primitive
            // false (matches the existing `_ => {}` fall-through).
            if !member.is_intrinsic() {
                match self.lookup(member) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                        let shape = self.object_shape(shape_id);
                        if !(shape.properties.is_empty()
                            && shape.string_index.is_none()
                            && shape.number_index.is_none())
                        {
                            mark_non_primitive = true;
                        }
                    }
                    Some(
                        TypeData::Function(_)
                        | TypeData::Callable(_)
                        | TypeData::Array(_)
                        | TypeData::Tuple(_),
                    ) => {
                        mark_non_primitive = true;
                    }
                    _ => {}
                }
            }
            let Some(member_class) = self.primitive_class_for(member) else {
                has_non_primitive = has_non_primitive || mark_non_primitive;
                continue;
            };
            if let Some(existing) = class {
                if existing != member_class {
                    return true;
                }
            } else {
                class = Some(member_class);
            }

            // Track literals to detect different values of the same primitive type
            if self.is_literal(member) {
                literals.push(member);
            }
        }

        // Check if we have multiple different literals of the same primitive class
        // e.g., "hello" & "world" = never, 1 & 2 = never
        if literals.len() > 1 {
            // Check if all literals are the same value
            let first = literals[0];
            if !literals.iter().all(|&lit| lit == first) {
                return true;
            }
        }

        // NOTE: We do NOT check `has_primitive && has_non_primitive` here.
        // TypeScript allows branded types like `string & { __brand: "UserId" }`.
        // This pattern is used for nominal typing and should NOT reduce to never.
        // The check was removed because it incorrectly broke valid branded types.

        false
    }

    /// Check if null or undefined intersects with any object type.
    ///
    /// In TypeScript, `null & object` and `undefined & object` reduce to `never`
    /// because null/undefined are disjoint from all object types.
    ///
    /// This is different from branded types like `string & { __brand: "UserId" }`
    /// which are valid and should NOT reduce to never.
    pub(crate) fn intersection_has_null_undefined_with_object(&self, members: &[TypeId]) -> bool {
        let mut has_null_or_undefined = false;
        let mut has_object_type = false;

        for &member in members {
            // Check for null or undefined
            if member.is_nullable() {
                has_null_or_undefined = true;
            } else if member == TypeId::OBJECT {
                // The `object` intrinsic is itself an object type
                has_object_type = true;
            } else if !member.is_intrinsic() {
                // Check if this is a structural object type
                // Task #48: Empty objects ARE object types and are disjoint from null/undefined
                // null & {} = never (null is not a non-nullish value)
                if let Some(
                    TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_),
                ) = self.lookup(member)
                {
                    has_object_type = true;
                }
            }

            // Early exit: if we have both, the intersection is never
            if has_null_or_undefined && has_object_type {
                return true;
            }
        }

        false
    }

    /// Check if the `object` intrinsic type (non-primitive) is intersected with any primitive type.
    ///
    /// In TypeScript, `object` represents ALL non-primitive types. It is disjoint from every
    /// primitive type: string, number, boolean, bigint, symbol, null, undefined.
    /// So `object & string = never`, `object & number = never`, etc.
    ///
    /// This is different from structural object types like `{ __brand: T }` which CAN
    /// intersect with primitives (branded types). The distinction is:
    /// - `object & string → never` (the `object` keyword excludes primitives)
    /// - `{} & string → string` (empty structural object is compatible)
    /// - `{ __brand: T } & string → string & { __brand: T }` (branded type)
    pub(crate) fn intersection_has_object_intrinsic_with_primitive(
        &self,
        members: &[TypeId],
    ) -> bool {
        let mut has_object_intrinsic = false;
        let mut has_primitive = false;

        for &member in members {
            // TypeId::OBJECT and PROMISE_BASE both resolve to
            // TypeData::Intrinsic(IntrinsicKind::Object); check both directly
            // to avoid the dyn-dispatched lookup. No other TypeId resolves
            // to Intrinsic(Object).
            if member == TypeId::OBJECT || member == TypeId::PROMISE_BASE {
                has_object_intrinsic = true;
            } else if self.primitive_class_for(member).is_some() {
                has_primitive = true;
            }

            if has_object_intrinsic && has_primitive {
                return true;
            }
        }

        false
    }

    /// Check if a `TypeParameter` with a non-nullable constraint is intersected with
    /// null, undefined, or void.
    ///
    /// For example, `T & undefined` where `T extends string` is `never` because
    /// `string` is disjoint from `undefined`. This follows tsc's behavior where
    /// type parameters are treated as their constraint for disjointness purposes.
    ///
    /// We only handle constraints that are known non-nullable types: primitives
    /// (string, number, boolean, bigint, symbol), the `object` intrinsic, and
    /// structural object types. For union constraints (e.g., `T extends string | null`),
    /// we conservatively skip the check since the constraint may include nullable types.
    pub(crate) fn intersection_has_type_param_disjoint_with_nullish(
        &self,
        members: &[TypeId],
    ) -> bool {
        let mut has_nullish = false;
        let mut has_non_nullable_type_param = false;

        for &member in members {
            if member.is_nullable() {
                has_nullish = true;
            } else if !member.is_intrinsic()
                && let Some(TypeData::TypeParameter(ref info)) = self.lookup(member)
                && let Some(constraint) = info.constraint
                && self.is_clearly_non_nullable_constraint(constraint)
            {
                has_non_nullable_type_param = true;
            }

            if has_nullish && has_non_nullable_type_param {
                return true;
            }
        }

        false
    }

    /// Merge same-named type parameters in an intersection, preferring constrained ones.
    ///
    /// When type predicate narrowing produces an intersection like
    /// `(T_constrained | undefined) & T_unconstrained` (where `T_constrained` has
    /// `T extends string` from a class and `T_unconstrained` is plain `T` from an interface),
    /// distribution would produce `(undefined & T_uncon) | (T_con & T_uncon)`.
    /// Since `T_uncon` has no constraint, `undefined & T_uncon` doesn't reduce to `never`,
    /// causing a false TS2532.
    ///
    /// This method replaces unconstrained type parameters with their constrained
    /// counterparts (same name) found among direct members or inside union sub-members.
    /// After replacement, `(T_con | undefined) & T_con` distributes to
    /// `(undefined & T_con) | (T_con & T_con)` → `never | T_con` → `T_con`.
    pub(crate) fn merge_same_name_type_params(&self, flat: &mut TypeListBuffer) {
        // First pass: collect constrained type parameter names → TypeId
        // from both direct members and union sub-members.
        let mut constrained: SmallVec<[(Atom, TypeId); 4]> = SmallVec::new();

        for &member in flat.iter() {
            match self.lookup(member) {
                Some(TypeData::TypeParameter(ref info))
                    if info.constraint.is_some()
                        && !constrained.iter().any(|(n, _)| *n == info.name) =>
                {
                    constrained.push((info.name, member));
                }
                Some(TypeData::Union(list_id)) => {
                    let union_members = self.type_list(list_id);
                    for &um in union_members.iter() {
                        if let Some(TypeData::TypeParameter(ref um_info)) = self.lookup(um)
                            && um_info.constraint.is_some()
                            && !constrained.iter().any(|(n, _)| *n == um_info.name)
                        {
                            constrained.push((um_info.name, um));
                        }
                    }
                }
                _ => {}
            }
        }

        if constrained.is_empty() {
            return;
        }

        // Second pass: replace unconstrained type params with constrained ones (same name).
        let mut changed = false;
        for slot in flat.iter_mut() {
            if let Some(TypeData::TypeParameter(ref info)) = self.lookup(*slot)
                && info.constraint.is_none()
                && let Some((_, replacement)) = constrained.iter().find(|(n, _)| *n == info.name)
                && *slot != *replacement
            {
                *slot = *replacement;
                changed = true;
            }
        }

        // Order-preserving dedup after replacement (may have introduced
        // non-adjacent duplicates).
        if changed {
            let mut seen = FxHashSet::default();
            flat.retain(|id| seen.insert(*id));
        }
    }

    /// Check if a type is clearly non-nullable (cannot include null/undefined).
    ///
    /// Returns true for:
    /// - Primitive types: string, number, boolean, bigint, symbol
    /// - The `object` intrinsic
    /// - Structural object types, arrays, tuples, functions, callables
    /// - Literal types (string/number/boolean/bigint literals)
    /// - Unions where every member is clearly non-nullable (e.g., `"A" | "B"`)
    /// - Intersections containing at least one clearly non-nullable member
    ///
    /// Returns false for:
    /// - null, undefined, void, any, unknown, never
    /// - Type parameters (constraint may be nullable)
    /// - Lazy/Application/Mapped (unresolved, can't determine)
    fn is_clearly_non_nullable_constraint(&self, id: TypeId) -> bool {
        self.is_clearly_non_nullable_constraint_with_depth(id, 0)
    }

    /// Recursive helper for `is_clearly_non_nullable_constraint`.
    ///
    /// `depth` guards against pathological/cyclic structures by capping recursion
    /// at a small constant. Union/Intersection bodies are still typically shallow.
    fn is_clearly_non_nullable_constraint_with_depth(&self, id: TypeId, depth: u32) -> bool {
        const MAX_DEPTH: u32 = 4;
        match id {
            TypeId::STRING
            | TypeId::NUMBER
            | TypeId::BOOLEAN
            | TypeId::BIGINT
            | TypeId::SYMBOL
            | TypeId::OBJECT => true,
            TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::VOID
            | TypeId::ANY
            | TypeId::UNKNOWN
            | TypeId::NEVER
            | TypeId::ERROR => false,
            _ => match self.lookup(id) {
                Some(
                    TypeData::Literal(_)
                    | TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::UniqueSymbol(_),
                ) => true,
                Some(TypeData::Union(list_id)) if depth < MAX_DEPTH => {
                    // A union is clearly non-nullable iff every member is.
                    // E.g., `"A" | "B"` is non-nullable; `string | undefined` is not.
                    let members = self.type_list(list_id);
                    !members.is_empty()
                        && members.iter().all(|&m| {
                            self.is_clearly_non_nullable_constraint_with_depth(m, depth + 1)
                        })
                }
                Some(TypeData::Intersection(list_id)) if depth < MAX_DEPTH => {
                    // An intersection is clearly non-nullable if ANY member is non-nullable
                    // (the non-nullable member forces the result to exclude null/undefined).
                    let members = self.type_list(list_id);
                    members
                        .iter()
                        .any(|&m| self.is_clearly_non_nullable_constraint_with_depth(m, depth + 1))
                }
                _ => false,
            },
        }
    }

    /// Check if an intersection contains disjoint primitive types (e.g., string & number = never).
    ///
    /// In TypeScript, certain primitive types are disjoint and their intersection is never:
    /// - string & number = never
    /// - string & boolean = never
    /// - number & boolean = never
    /// - bigint & number = never
    /// - bigint & string = never
    /// - symbol & (any other primitive except itself) = never
    ///
    /// Note: Literals of the same primitive type are NOT disjoint (e.g., "a" & "b" is valid).
    pub(crate) fn has_disjoint_primitives(&self, members: &[TypeId]) -> bool {
        use rustc_hash::FxHashSet;

        let mut primitive_kinds: FxHashSet<PrimitiveKind> = FxHashSet::default();

        for &member in members {
            let kind = self.get_primitive_kind(member);
            if let Some(k) = kind {
                // Check for disjoint with existing primitives
                for &existing_kind in &primitive_kinds {
                    if Self::are_primitives_disjoint(k, existing_kind) {
                        return true;
                    }
                }
                primitive_kinds.insert(k);
            }
        }

        false
    }

    /// Get the primitive kind of a type (if it's a primitive or literal of a primitive).
    fn get_primitive_kind(&self, type_id: TypeId) -> Option<PrimitiveKind> {
        match self.lookup(type_id) {
            // Direct primitives
            Some(TypeData::Intrinsic(IntrinsicKind::String) | TypeData::TemplateLiteral(_)) => {
                Some(PrimitiveKind::String)
            }
            Some(TypeData::Intrinsic(IntrinsicKind::Number)) => Some(PrimitiveKind::Number),
            Some(TypeData::Intrinsic(IntrinsicKind::Boolean)) => Some(PrimitiveKind::Boolean),
            Some(TypeData::Intrinsic(IntrinsicKind::Bigint)) => Some(PrimitiveKind::BigInt),
            Some(TypeData::Intrinsic(IntrinsicKind::Symbol)) => Some(PrimitiveKind::Symbol),
            // Literals - they inherit the kind of their base type
            Some(TypeData::Literal(lit)) => Some(PrimitiveKind::from_literal(&lit)),
            // Template literals are string-like
            _ => None,
        }
    }

    fn get_unit_value_key(&self, type_id: TypeId) -> Option<UnitValueKey> {
        // Fast path: intrinsic IDs are unit values only for null/undefined/void/true/false.
        // Other intrinsics (string, number, boolean, bigint, symbol, object, any, unknown,
        // never, error, ...) have no unit value; skip the dyn-dispatched lookup.
        if type_id.is_intrinsic() {
            return match type_id {
                TypeId::NULL => Some(UnitValueKey::Null),
                TypeId::UNDEFINED | TypeId::VOID => Some(UnitValueKey::Undefined),
                TypeId::BOOLEAN_TRUE => Some(UnitValueKey::Boolean(true)),
                TypeId::BOOLEAN_FALSE => Some(UnitValueKey::Boolean(false)),
                _ => None,
            };
        }
        match self.lookup(type_id) {
            Some(TypeData::Literal(LiteralValue::String(atom))) => Some(UnitValueKey::String(atom)),
            Some(TypeData::Literal(LiteralValue::Number(num))) => {
                Some(UnitValueKey::Number(num.0.to_bits()))
            }
            Some(TypeData::Literal(LiteralValue::Boolean(value))) => {
                Some(UnitValueKey::Boolean(value))
            }
            Some(TypeData::Literal(LiteralValue::BigInt(atom))) => Some(UnitValueKey::BigInt(atom)),
            Some(TypeData::Enum(def_id, member_type)) => self
                .get_unit_value_key(member_type)
                .map(|key| UnitValueKey::Enum(def_id, Box::new(key))),
            Some(TypeData::Intrinsic(IntrinsicKind::Null)) => Some(UnitValueKey::Null),
            Some(TypeData::Intrinsic(IntrinsicKind::Undefined | IntrinsicKind::Void)) => {
                Some(UnitValueKey::Undefined)
            }
            _ => None,
        }
    }

    fn unit_values_are_disjoint(left: &UnitValueKey, right: &UnitValueKey) -> bool {
        use UnitValueKey::*;

        match (left, right) {
            (Null, Null) | (Undefined, Undefined) => false,
            (Null, _) | (_, Null) | (Undefined, _) | (_, Undefined) => true,
            (String(a), String(b)) | (BigInt(a), BigInt(b)) => a != b,
            (Number(a), Number(b)) => a != b,
            (Boolean(a), Boolean(b)) => a != b,
            (Enum(def_a, key_a), Enum(def_b, key_b)) => {
                if def_a != def_b {
                    true
                } else {
                    Self::unit_values_are_disjoint(key_a, key_b)
                }
            }
            (Enum(_, key), other) | (other, Enum(_, key)) => {
                Self::unit_values_are_disjoint(key, other)
            }
            _ => true,
        }
    }

    pub(crate) fn intersection_has_disjoint_unit_values(&self, members: &[TypeId]) -> bool {
        let mut seen = Vec::with_capacity(members.len());

        for &member in members {
            let Some(key) = self.get_unit_value_key(member) else {
                continue;
            };
            if seen
                .iter()
                .any(|existing| Self::unit_values_are_disjoint(existing, &key))
            {
                return true;
            }
            if !seen.contains(&key) {
                seen.push(key);
            }
        }

        false
    }

    /// Check if two primitive kinds are disjoint (their intersection is never).
    const fn are_primitives_disjoint(a: PrimitiveKind, b: PrimitiveKind) -> bool {
        use PrimitiveKind::*;
        match (a, b) {
            // Same kind is never disjoint
            (String, String)
            | (Number, Number)
            | (Boolean, Boolean)
            | (BigInt, BigInt)
            | (Symbol, Symbol) => false,
            // String is disjoint from number, boolean, bigint, symbol
            (String, Number | Boolean | BigInt | Symbol)
            | (Number, String | Boolean | BigInt | Symbol)
            | (Boolean, String | Number | BigInt | Symbol)
            | (BigInt, String | Number | Boolean | Symbol)
            | (Symbol, String | Number | Boolean | BigInt) => true,
        }
    }

    /// Check if a type is a literal type.
    /// Uses the visitor pattern from `solver::visitor`.
    fn is_literal(&self, type_id: TypeId) -> bool {
        is_literal_type(self, type_id)
    }

    pub(crate) fn intersection_has_disjoint_object_literals(&self, members: &[TypeId]) -> bool {
        // A conflict that reduces an object intersection to `never` always surfaces
        // through a property NAME shared by two members: a discriminant mismatch
        // (`{ kind: "a" } & { kind: "b" }`), a private/public collision, or a
        // cross-domain literal clash (`{ a: "" } & { a: number }`). So index every
        // member's properties by name once — O(sum of property counts) — then run a
        // bounded per-name conflict test.
        //
        // This replaces the previous O(N²) pairwise scan over whole shapes, which
        // carried a hard `MAX_DISJOINT_CHECK_SIZE = 25` cap that silently dropped the
        // reduction for larger intersections (e.g. a 26-member conflicting-discriminant
        // chain, producing a false-positive `TS2322`). The per-name test runs in O(k)
        // for the common single-literal discriminant shape, so the never-reduction
        // stays correct for arbitrarily large intersections without quadratic cost.
        //
        // Property-bearing shapes come from both Object AND Callable types — a
        // callable's named members (e.g. `{ (x: string): number; a: "" }`) can
        // conflict with an object property of the same name.
        let mut by_name: FxHashMap<Atom, SmallVec<[PropOccurrence; 4]>> = FxHashMap::default();
        let mut ingest = |props: &[PropertyInfo]| {
            for prop in props {
                by_name.entry(prop.name).or_default().push(PropOccurrence {
                    type_id: prop.type_id,
                    optional: prop.optional,
                    visibility: prop.visibility,
                });
            }
        };

        for &member in members {
            if member.is_intrinsic() {
                continue;
            }
            let Some(key) = self.lookup(member) else {
                continue;
            };
            match key {
                TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                    ingest(&self.object_shape(shape_id).properties);
                }
                TypeData::Callable(callable_id) => {
                    ingest(&self.callable_shape(callable_id).properties);
                }
                _ => {}
            }
        }

        by_name
            .values()
            .any(|occurrences| self.name_occurrences_disjoint(occurrences))
    }

    /// Returns true when the occurrences of a single property name across the
    /// members of an intersection are mutually unsatisfiable, forcing the whole
    /// intersection to `never`. Mirrors the per-pair predicate of the former
    /// `object_literals_disjoint` scan, but as a set of existence tests over all
    /// occurrences of the name (O(k) for the common single-literal case) rather
    /// than an O(k²) pairwise comparison.
    fn name_occurrences_disjoint(&self, occurrences: &[PropOccurrence]) -> bool {
        if occurrences.len() < 2 {
            return false;
        }

        // Visibility: a `private` member's declaring-class identity cannot be
        // satisfied by a public or protected member of the same name, so a private
        // occurrence alongside any differently-visible occurrence reduces to `never`.
        // Protected/public mixes stay on the merge path — they can still expose a
        // public property.
        let mut has_private = false;
        let mut has_non_private = false;
        for occ in occurrences {
            if occ.visibility == Visibility::Private {
                has_private = true;
            } else {
                has_non_private = true;
            }
        }
        if has_private && has_non_private {
            return true;
        }

        // Cross-domain: a literal occurrence whose primitive class differs from
        // another occurrence's class is disjoint (e.g. `a: ""` (string) & `a: number`).
        // As with the literal value-set check below, two *optional* occurrences only
        // make the property itself `never`, so a conflicting pair needs at least one
        // required side. A pair (literal `L` of class `C_L`, occurrence `X` of class
        // `C_X != C_L`) is a not-both-optional conflict when either `L` is required
        // (and any differing class exists) or some required occurrence has a class
        // distinct from a literal's class.
        let mut classes: SmallVec<[PrimitiveClass; 8]> = SmallVec::new();
        let mut literal_classes: SmallVec<[PrimitiveClass; 8]> = SmallVec::new();
        let mut required_classes: SmallVec<[PrimitiveClass; 8]> = SmallVec::new();
        let mut has_required_literal = false;
        for occ in occurrences {
            let Some(class) = self.primitive_class_for(occ.type_id) else {
                continue;
            };
            if !classes.contains(&class) {
                classes.push(class);
            }
            if !occ.optional && !required_classes.contains(&class) {
                required_classes.push(class);
            }
            if self.is_literal(occ.type_id) {
                if !literal_classes.contains(&class) {
                    literal_classes.push(class);
                }
                has_required_literal |= !occ.optional;
            }
        }
        let required_literal_cross = has_required_literal && classes.len() >= 2;
        let literal_vs_required_cross = literal_classes
            .iter()
            .any(|lit| required_classes.iter().any(|req| req != lit));
        if required_literal_cross || literal_vs_required_cross {
            return true;
        }

        // Literal value-sets: group literal-typed occurrences by domain. A required
        // occurrence whose value-set is disjoint from any other occurrence's forces
        // `never`; two *optional* occurrences only make the property itself `never`,
        // so a conflicting pair needs at least one required side.
        let mut by_domain: LiteralOccurrencesByDomain = SmallVec::new();
        let mut has_required_literal = false;
        for occ in occurrences {
            let Some(kind) = self.literal_kind_from_type(occ.type_id) else {
                continue;
            };
            has_required_literal |= !occ.optional;
            let domain = kind.domain();
            let lit = LiteralOccurrence {
                kind,
                optional: occ.optional,
            };
            match by_domain.iter_mut().find(|(d, _)| *d == domain) {
                Some((_, group)) => group.push(lit),
                None => {
                    let mut group = SmallVec::new();
                    group.push(lit);
                    by_domain.push((domain, group));
                }
            }
        }

        // Literals drawn from two different domains are mutually disjoint; if any of
        // them is required the intersection is `never`.
        if has_required_literal && by_domain.len() >= 2 {
            return true;
        }

        by_domain
            .iter()
            .any(|(_, group)| Self::literal_group_has_disjoint_pair(group))
    }

    /// Within a single literal domain, returns true when some *required* occurrence's
    /// value-set is disjoint from another occurrence's value-set. Single-literal
    /// occurrences (the discriminant case) resolve in O(k); the union fallback is
    /// pairwise but only runs for the rare property whose intersection members carry
    /// union-literal types, where the group is small.
    fn literal_group_has_disjoint_pair(group: &[LiteralOccurrence]) -> bool {
        if group.len() < 2 {
            return false;
        }

        if group
            .iter()
            .all(|occ| matches!(occ.kind, LiteralKind::Single(_)))
        {
            let mut distinct: FxHashSet<LiteralValue> = FxHashSet::default();
            let mut has_required = false;
            for occ in group {
                if let LiteralKind::Single(value) = occ.kind {
                    distinct.insert(value);
                }
                has_required |= !occ.optional;
            }
            return distinct.len() >= 2 && has_required;
        }

        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                if !(group[i].optional && group[j].optional)
                    && group[i].kind.is_disjoint(&group[j].kind)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn intersection_has_conflicting_private_brands(&self, members: &[TypeId]) -> bool {
        let mut brand_sets: SmallVec<[FxHashSet<Atom>; 8]> = SmallVec::new();

        for &member in members {
            if member.is_intrinsic() {
                continue;
            }
            let Some(type_data) = self.lookup(member) else {
                continue;
            };
            let properties: &[PropertyInfo] = match type_data {
                TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                    &self.object_shape(shape_id).properties
                }
                TypeData::Callable(callable_id) => &self.callable_shape(callable_id).properties,
                _ => continue,
            };

            let brands: FxHashSet<Atom> = properties
                .iter()
                .filter_map(|prop| {
                    let name = self.resolve_atom(prop.name);
                    name.starts_with("__private_brand_").then_some(prop.name)
                })
                .collect();
            if !brands.is_empty() {
                let has_private_member = properties.iter().any(|prop| {
                    prop.visibility == Visibility::Private
                        && !self.resolve_atom(prop.name).starts_with("__private_brand_")
                });
                let has_restricted_member = properties.iter().any(|prop| {
                    matches!(prop.visibility, Visibility::Private | Visibility::Protected)
                        && !self.resolve_atom(prop.name).starts_with("__private_brand_")
                });
                if has_private_member || !has_restricted_member {
                    brand_sets.push(brands);
                }
            }
        }

        if brand_sets.len() < 2 {
            return false;
        }

        let mut all_brands = FxHashSet::default();
        for brands in &brand_sets {
            all_brands.extend(brands.iter().copied());
        }
        if all_brands.len() < 2 {
            return false;
        }

        !brand_sets
            .iter()
            .any(|brands| all_brands.iter().all(|brand| brands.contains(brand)))
    }

    fn literal_kind_from_type(&self, type_id: TypeId) -> Option<LiteralKind> {
        let key = self.lookup(type_id)?;
        match key {
            TypeData::Literal(literal) => Some(LiteralKind::Single(literal)),
            TypeData::Union(members) => {
                let members = self.type_list(members);
                let mut domain: Option<LiteralDomain> = None;
                let mut values = FxHashSet::default();
                for &member in members.iter() {
                    let Some(TypeData::Literal(literal)) = self.lookup(member) else {
                        return None;
                    };
                    let literal_domain = literal_domain(&literal);
                    if let Some(existing) = domain {
                        if existing != literal_domain {
                            return None;
                        }
                    } else {
                        domain = Some(literal_domain);
                    }
                    values.insert(literal);
                }
                domain.map(|domain| LiteralKind::Union(domain, values))
            }
            _ => None,
        }
    }

    pub(super) fn literal_domain_from_type(&self, type_id: TypeId) -> Option<LiteralDomain> {
        self.literal_kind_from_type(type_id).map(|k| k.domain())
    }

    pub(crate) fn primitive_class_for(&self, type_id: TypeId) -> Option<PrimitiveClass> {
        match type_id {
            TypeId::STRING => return Some(PrimitiveClass::String),
            TypeId::NUMBER => return Some(PrimitiveClass::Number),
            TypeId::BOOLEAN => return Some(PrimitiveClass::Boolean),
            TypeId::BIGINT => return Some(PrimitiveClass::Bigint),
            TypeId::SYMBOL => return Some(PrimitiveClass::Symbol),
            TypeId::NULL => return Some(PrimitiveClass::Null),
            TypeId::UNDEFINED | TypeId::VOID => return Some(PrimitiveClass::Undefined),
            _ => {}
        }

        let key = self.lookup(type_id)?;

        match key {
            TypeData::Intrinsic(kind) => match kind {
                IntrinsicKind::String => Some(PrimitiveClass::String),
                IntrinsicKind::Number => Some(PrimitiveClass::Number),
                IntrinsicKind::Boolean => Some(PrimitiveClass::Boolean),
                IntrinsicKind::Bigint => Some(PrimitiveClass::Bigint),
                IntrinsicKind::Symbol => Some(PrimitiveClass::Symbol),
                IntrinsicKind::Null => Some(PrimitiveClass::Null),
                IntrinsicKind::Undefined | IntrinsicKind::Void => Some(PrimitiveClass::Undefined),
                _ => None,
            },
            TypeData::Literal(literal) => match literal {
                LiteralValue::String(_) => Some(PrimitiveClass::String),
                LiteralValue::Number(_) => Some(PrimitiveClass::Number),
                LiteralValue::Boolean(_) => Some(PrimitiveClass::Boolean),
                LiteralValue::BigInt(_) => Some(PrimitiveClass::Bigint),
            },
            TypeData::UniqueSymbol(_) => Some(PrimitiveClass::Symbol),
            _ => None,
        }
    }

    /// Merge `Enum(D, X) | Enum(D, Y)` into `Enum(D, X | Y)` for same-`DefId` enum
    /// types in the flat union list. After `sort_union_members`, same-def-id enums are
    /// adjacent, so a single forward scan suffices.
    ///
    /// This preserves the nominal enum wrapper when control-flow analysis
    /// splits and rejoins enum member types (e.g., `E1.a | E1.b` → `E1`).
    pub(crate) fn merge_same_enum_parts(&self, flat: &mut TypeListBuffer) {
        if flat.len() < 2 {
            return;
        }
        let mut i = 0;
        while i < flat.len() {
            let Some(TypeData::Enum(def_a, _)) = self.lookup(flat[i]) else {
                i += 1;
                continue;
            };
            // Collect consecutive enum members with the same DefId.
            let mut j = i + 1;
            while j < flat.len() {
                if let Some(TypeData::Enum(def_b, _)) = self.lookup(flat[j])
                    && def_b == def_a
                {
                    j += 1;
                    continue;
                }
                break;
            }
            if j > i + 1 {
                // Multiple same-def enum parts: merge their inners.
                let inners: Vec<TypeId> = flat[i..j]
                    .iter()
                    .filter_map(|&id| match self.lookup(id) {
                        Some(TypeData::Enum(_, inner)) => Some(inner),
                        _ => None,
                    })
                    .collect();
                let merged_inner = self.union_from_iter(inners);
                flat[i] = self.intern(TypeData::Enum(def_a, merged_inner));
                flat.drain(i + 1..j);
            }
            i += 1;
        }
    }

    /// Absorb literal types into their corresponding primitive types.
    /// e.g., "a" | string | number => string | number
    /// e.g., 1 | 2 | number => number
    /// e.g., true | boolean => boolean
    ///
    /// This is called after deduplication and before creating the union.
    pub(crate) fn absorb_literals_into_primitives(&self, flat: &mut TypeListBuffer) {
        // Group types by primitive class
        let mut has_string = false;
        let mut has_number = false;
        let mut has_boolean = false;
        let mut has_bigint = false;
        let mut _has_symbol = false;
        let mut has_true = false;
        let mut has_false = false;

        // First pass: identify which primitive types are present
        for &type_id in flat.iter() {
            match type_id {
                TypeId::STRING => has_string = true,
                TypeId::NUMBER => has_number = true,
                TypeId::BOOLEAN => has_boolean = true,
                TypeId::BIGINT => has_bigint = true,
                TypeId::SYMBOL => _has_symbol = true,
                TypeId::BOOLEAN_TRUE => has_true = true,
                TypeId::BOOLEAN_FALSE => has_false = true,
                _ => {
                    if let Some(TypeData::Intrinsic(kind)) = self.lookup(type_id) {
                        match kind {
                            IntrinsicKind::String => has_string = true,
                            IntrinsicKind::Number => has_number = true,
                            IntrinsicKind::Boolean => has_boolean = true,
                            IntrinsicKind::Bigint => has_bigint = true,
                            IntrinsicKind::Symbol => _has_symbol = true,
                            _ => {}
                        }
                    }
                }
            }
        }

        // If both `true` and `false` are present without `boolean`, reduce to `boolean`
        // TypeScript: `true | false` === `boolean`
        if has_true && has_false && !has_boolean {
            has_boolean = true;
            // Replace `true` with `boolean`, remove `false`
            for type_id in flat.iter_mut() {
                if *type_id == TypeId::BOOLEAN_TRUE {
                    *type_id = TypeId::BOOLEAN;
                }
            }
            flat.retain(|type_id| *type_id != TypeId::BOOLEAN_FALSE);
        }

        // Second pass: remove literal types that have a corresponding primitive
        flat.retain(|type_id| {
            // Check for boolean literal intrinsics
            if *type_id == TypeId::BOOLEAN_TRUE || *type_id == TypeId::BOOLEAN_FALSE {
                return !has_boolean;
            }

            // Keep if it's not a literal type
            let Some(TypeData::Literal(literal)) = self.lookup(*type_id) else {
                return true;
            };

            // Remove literal if the corresponding primitive is present
            match literal {
                LiteralValue::String(_) => !has_string,
                LiteralValue::Number(_) => !has_number,
                LiteralValue::Boolean(_) => !has_boolean,
                LiteralValue::BigInt(_) => !has_bigint,
            }
        });
    }

    /// TS2590 pairwise-iteration budget matching tsc `removeSubtypes`.
    /// `UNION_NORMALIZE_CACHE_MAX_LEN` keeps flag-setting inputs uncached.
    pub(crate) const UNION_SUBTYPE_PAIRWISE_LIMIT: u64 = 1_000_000;

    /// Whether `id` is an *unevaluated deferred type-level operation* that the
    /// shallow subtype engine (`is_subtype_shallow`) can only relate to a
    /// distinct peer by identity — never as a structural subtype in either
    /// direction. Members of this family are inert under union subtype reduction:
    /// they can neither be removed nor remove a peer, so the O(N²) pairwise sweep
    /// over them is pure waste and is lifted out by `reduce_union_subtypes`.
    ///
    /// The shallow engine returns `true` for a *distinct* pair only through its
    /// literal→primitive/template, builtin/union-membership, small-union, or
    /// `Object`/`Function` structural arms. None of the variants below are a
    /// `Literal`, a builtin primitive, a `Union`, an `Object`/`ObjectWithIndex`,
    /// a `Function`, or a `TemplateLiteral`, so every arm misses and the engine
    /// falls through to `false` whether the member is the source or the target.
    /// `TypeParameter`/`Lazy` are intentionally excluded: they are skipped
    /// wholesale upstream because they interact with reduction non-locally (an
    /// unresolved parameter can stand for a super/subtype of a concrete peer),
    /// which requires leaving the surrounding concrete members unreduced rather
    /// than reduced around them.
    fn is_shallow_inert_member(&self, id: TypeId) -> bool {
        matches!(
            self.lookup(id),
            Some(
                TypeData::Conditional(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::Mapped(_)
                    | TypeData::KeyOf(_)
                    | TypeData::Infer(_)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::TypeQuery(_)
                    | TypeData::ModuleNamespace(_)
                    | TypeData::NoInfer(_)
                    | TypeData::Application(_)
                    | TypeData::BoundParameter(_)
                    | TypeData::Recursive(_)
                    | TypeData::ThisType
                    | TypeData::UnresolvedTypeName(_)
            )
        )
    }

    /// Remove redundant types from a union using shallow subtype checks.
    /// If A <: B, then A | B = B (A is redundant).
    pub(crate) fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }

        // Lift out *unevaluated deferred type-level operations* before the
        // pairwise sweep. The shallow subtype engine (`is_subtype_shallow`) has
        // no rules for any member of this family: it relates to a distinct peer
        // only by identity (already removed by the upstream dedup), neither as
        // source nor as target, so it is *fully inert* — it can neither be
        // removed by reduction nor cause another member's removal. Running the
        // O(N) pairwise scan over each one is pure waste.
        //
        // This is the eager-vs-deferred divergence #13242 targets: distributing
        // `Exclude`/`Extract` over a wide union yields a union of deferred
        // `Conditional`s tsc keeps lazy, and the same flat-slope reasoning holds
        // for every other deferred operator that can stack up wide before it
        // resolves — `keyof U` arms, `T[K]` index accesses, mapped/string-
        // intrinsic/application/infer operands, `typeof`/namespace leaves, and
        // the De Bruijn placeholders used while canonicalizing recursive aliases.
        // #13667 proved the `Conditional`/`IndexAccess` slice; the full inert
        // family (see `is_shallow_inert_member`) removes the same wasted
        // N·(N−1) sweep for `keyof`-distribution and mapped-operand unions.
        //
        // Partition the inert members aside, reduce only the reducible remainder
        // through the *unchanged* engine below (so discriminant partitioning,
        // literal absorption, and structural object reduction are all preserved
        // exactly as for an inert-free union), then splice the inert members
        // back. An all-inert union short-circuits to zero pairwise checks; a
        // mixed union (e.g. a JSX intrinsic-element props union carrying an
        // `IndexAccess` arm) still reduces its concrete members. Reducing only
        // the remainder — rather than skipping reduction for the whole union
        // when any inert member is present — is what keeps the concrete arms
        // collapsing the way tsc does.
        let deferred_count = flat
            .iter()
            .filter(|&&id| self.is_shallow_inert_member(id))
            .count();
        if deferred_count > 0 {
            if deferred_count == len {
                // Every member is an inert deferred form: nothing to reduce.
                return;
            }
            let mut deferred: TypeListBuffer = SmallVec::new();
            let mut reducible: TypeListBuffer = SmallVec::new();
            for &id in flat.iter() {
                if self.is_shallow_inert_member(id) {
                    deferred.push(id);
                } else {
                    reducible.push(id);
                }
            }
            self.reduce_union_subtypes(&mut reducible);
            reducible.extend(deferred);
            // Reduction may have dropped reducible members; re-canonicalize the
            // recombined list so union member identity (sort + dedup) stays stable.
            self.sort_union_members(&mut reducible);
            reducible.dedup();
            *flat = reducible;
            return;
        }

        let len = flat.len();
        let pairwise = (len as u64) * (len as u64 - 1);
        tsz_common::perf_counters::record_union_subtype_reduction(len as u64, pairwise);
        tracing::trace!(len, "reduce_union_subtypes: entry");

        // Skip structures tsc's default literal reduction would not structurally
        // reduce. Template-literal members can only absorb matching string
        // literals, so that family uses a targeted pass instead of O(N²).
        {
            let mut has_primitive = false;
            let mut string_literals: Vec<(usize, Atom)> = Vec::new();
            let mut templates: Vec<TemplateLiteralId> = Vec::new();
            let all_non_reducible = flat.iter().enumerate().all(|(idx, &ty)| {
                if self.is_identity_comparable_type(ty) {
                    // A widened primitive can absorb literals of its kind.
                    if ty == TypeId::STRING
                        || ty == TypeId::NUMBER
                        || ty == TypeId::BOOLEAN
                        || ty == TypeId::BIGINT
                        || ty == TypeId::SYMBOL
                    {
                        has_primitive = true;
                    }
                    if let Some(TypeData::Literal(LiteralValue::String(atom))) = self.lookup(ty) {
                        string_literals.push((idx, atom));
                    }
                    return true;
                }
                match self.lookup(ty) {
                    Some(TypeData::TemplateLiteral(template_id)) => {
                        templates.push(template_id);
                        true
                    }
                    Some(
                        TypeData::Array(_)
                        | TypeData::Tuple(_)
                        | TypeData::Object(_)
                        | TypeData::ObjectWithIndex(_)
                        | TypeData::Enum(_, _)
                        | TypeData::Lazy(_)
                        | TypeData::Application(_)
                        | TypeData::Callable(_)
                        // Without a widened primitive peer, literals are inert.
                        | TypeData::Literal(_),
                    ) => true,
                    _ => false,
                }
            });
            if all_non_reducible && !has_primitive {
                // Only string-literal × template absorption can still reduce.
                if !templates.is_empty() && !string_literals.is_empty() {
                    self.remove_string_literals_matched_by_templates(
                        flat,
                        &string_literals,
                        &templates,
                    );
                }
                return;
            }
        }

        // TS2590 is sticky for checker diagnostics; internal construction keeps
        // the large union instead of poisoning downstream with ERROR.
        if pairwise >= Self::UNION_SUBTYPE_PAIRWISE_LIMIT {
            self.set_union_too_complex();
            return; // skip reduction, preserve the union members as-is
        }

        // Partition large discriminated unions before pairwise checks.
        if len > 16
            && let Some(partitioned) = self.try_partition_union_reduction(flat)
        {
            *flat = partitioned;
            return;
        }

        // Dispatch on width: small unions use a u64 bitset; wider ones a Vec.
        self.reduce_union_subtypes_sized(flat);
    }

    /// Size-dispatching union reduction: the bitset path caps at 64 members, so
    /// anything wider must use the heap-backed large-row reducer. Every caller
    /// that reduces a union of unbounded width — the top-level entry and each
    /// discriminant/fallback/combined group in `try_partition_union_reduction`
    /// — routes through this dispatcher so the `len <= 64` invariant of
    /// `reduce_union_subtypes_quadratic` is never violated.
    fn reduce_union_subtypes_sized(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 64 {
            self.reduce_union_subtypes_quadratic(flat);
        } else {
            self.reduce_union_subtypes_large_row(flat);
        }
    }

    /// Heap-backed union reduction for partitions wider than the 64-bit bitset.
    /// Identical semantics to `reduce_union_subtypes_quadratic`
    /// (`shallow_reduce_kind` / `may_relate` / `is_subtype_shallow`), only the
    /// keep-set is a `Vec<bool>` instead of a `u64` so it is size-unbounded.
    fn reduce_union_subtypes_large_row(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }
        let mut keep = vec![true; len];
        // Precompute one coarse structural bucket per member (O(N)). A pair
        // whose buckets cannot relate (e.g. object-vs-literal, primitive-vs-
        // object, literal-vs-literal) is skipped without the shallow subtype
        // call: `is_subtype_shallow` would deterministically answer `false`.
        // This subsumes the bare-literal-pair skip and collapses the dominant
        // cross-kind term of the mixed object/primitive/literal large-row
        // union shape from N(N-1) shallow calls toward the count of genuinely
        // same-kind pairs, while object-vs-object / function-vs-function
        // reductions are preserved exactly. The inert deferred family is
        // already lifted out above, so the residual here is concrete; any
        // member the classifier does not model precisely buckets as
        // `Wildcard` and keeps the real check.
        let kinds: Vec<ShallowReduceKind> = flat
            .iter()
            .map(|&id| self.shallow_reduce_kind(id))
            .collect();
        let shallow_checks = tsz_common::perf_counters::enabled_fast()
            .then(|| &tsz_common::perf_counters::counters().union_subtype_reduction_shallow_checks);
        for i in 0..len {
            if !keep[i] {
                continue;
            }
            for j in 0..len {
                if i == j || !keep[j] {
                    continue;
                }
                if !ShallowReduceKind::may_relate(kinds[i], kinds[j]) {
                    // The bucket skip must never hide a real subtype
                    // relation: validate the over-approximation against the
                    // shallow engine in debug/test builds across the whole
                    // corpus. Release builds pay only the `may_relate` match.
                    debug_assert!(
                        !self.is_subtype_shallow(flat[i], flat[j]),
                        "shallow_reduce_kind skipped a relating pair: \
                         {:?} <: {:?}",
                        kinds[i],
                        kinds[j],
                    );
                    continue;
                }
                if let Some(counter) = shallow_checks {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                if self.is_subtype_shallow(flat[i], flat[j]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        let mut write = 0;
        for read in 0..len {
            if keep[read] {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
    }

    /// Reduce large object unions by likely discriminant property when useful.
    fn try_partition_union_reduction(&self, members: &[TypeId]) -> Option<TypeListBuffer> {
        // Candidate property appears in at least half of object members.
        let mut prop_counts: FxHashMap<Atom, usize> = FxHashMap::default();
        let mut object_count = 0;

        for &member in members {
            if let Some(shape_id) = crate::visitor::object_shape_id(self, member)
                .or_else(|| crate::visitor::object_with_index_shape_id(self, member))
            {
                object_count += 1;
                let shape = self.object_shape(shape_id);
                for prop in &shape.properties {
                    *prop_counts.entry(prop.name).or_insert(0) += 1;
                }
            }
        }

        if object_count < 8 {
            return None;
        }

        let discriminant_prop = prop_counts
            .into_iter()
            .filter(|&(_, count)| count >= object_count / 2)
            .max_by_key(|&(_, count)| count)
            .map(|(name, _)| name)?;

        // Missing/non-object members go into a fallback group.
        let mut partitions: FxHashMap<TypeId, Vec<TypeId>> = FxHashMap::default();
        let mut fallback: Vec<TypeId> = Vec::new();

        for &member in members {
            let val = crate::visitor::object_shape_id(self, member)
                .or_else(|| crate::visitor::object_with_index_shape_id(self, member))
                .and_then(|sid| {
                    let shape = self.object_shape(sid);
                    crate::utils::lookup_property(
                        self,
                        &shape.properties,
                        Some(sid),
                        discriminant_prop,
                    )
                    .map(|p| p.type_id)
                });

            if let Some(v) = val {
                partitions.entry(v).or_default().push(member);
            } else {
                fallback.push(member);
            }
        }

        // Reduce each partition independently. A single discriminant value can
        // hold more than 64 members (the typebox >64-arm same-discriminant
        // shape), so route through the width dispatcher rather than the bitset
        // reducer directly.
        let mut result: TypeListBuffer = SmallVec::new();
        for (_, group) in partitions {
            let mut group_buf = TypeListBuffer::from_vec(group);
            self.reduce_union_subtypes_sized(&mut group_buf);
            result.extend(group_buf);
        }

        // Reduce fallback, then check fallback against all winners.
        if !fallback.is_empty() {
            let mut fallback_buf = TypeListBuffer::from_vec(fallback);
            self.reduce_union_subtypes_sized(&mut fallback_buf);
            result.extend(fallback_buf);
        }

        // Final pass only when partitioning actually reduced the problem.
        if result.len() < members.len() {
            self.reduce_union_subtypes_sized(&mut result);
            Some(result)
        } else {
            None
        }
    }

    /// quadratic implementation of union reduction, used within partitions.
    fn reduce_union_subtypes_quadratic(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }
        // Use a u64 bitset; callers keep partitions within this size.
        debug_assert!(len <= 64, "reduce_union_subtypes_quadratic: len={len} > 64");
        let mut keep: u64 = if len >= 64 {
            u64::MAX
        } else {
            (1u64 << len) - 1
        };
        // One coarse structural bucket per member (stack-allocated for the
        // <=64-member partition). Pairs whose buckets cannot relate skip the
        // shallow subtype call; see `ShallowReduceKind::may_relate`. This
        // subsumes the bare-literal-pair skip (`may_relate(Literal, Literal)` is
        // `false`) and every other cross-kind disjoint pair.
        let kinds: SmallVec<[ShallowReduceKind; 64]> = flat
            .iter()
            .map(|&id| self.shallow_reduce_kind(id))
            .collect();
        let shallow_checks = tsz_common::perf_counters::enabled_fast()
            .then(|| &tsz_common::perf_counters::counters().union_subtype_reduction_shallow_checks);
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
                }
                if !ShallowReduceKind::may_relate(kinds[i], kinds[j]) {
                    debug_assert!(
                        !self.is_subtype_shallow(flat[i], flat[j]),
                        "shallow_reduce_kind skipped a relating pair: {:?} <: {:?}",
                        kinds[i],
                        kinds[j],
                    );
                    continue;
                }
                if let Some(counter) = shallow_checks {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                if self.is_subtype_shallow(flat[i], flat[j]) {
                    keep &= !(1u64 << i);
                    break;
                }
            }
        }
        let mut write = 0;
        for read in 0..len {
            if keep & (1u64 << read) != 0 {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
    }

    /// Remove redundant types from an intersection using shallow subtype checks.
    /// If A <: B, then A & B = A (B is redundant).
    pub(crate) fn reduce_intersection_subtypes(&self, flat: &mut TypeListBuffer) {
        // Performance guard: skip O(N²) reduction for large intersections.
        // This is an optimization (removing redundant supertypes), not required for correctness.
        // For very large intersections (e.g., T extends A & B & C & ...), the O(N²) pairwise
        // subtype checks are prohibitively expensive. Skip and keep all members.
        const MAX_REDUCTION_SIZE: usize = 25;
        if flat.len() > MAX_REDUCTION_SIZE {
            return;
        }

        // Mark redundant elements using a u64 bitset (max 25 members from guard above),
        // then compact in one pass. Avoids heap allocation for the keep-set.
        let len = flat.len();
        debug_assert!(len <= 64, "reduce_intersection_subtypes: len={len} > 64");
        let mut keep: u64 = (1u64 << len) - 1; // all bits set
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
                }
                // If j is a subtype of i, i is the supertype and redundant in an intersection
                if self.is_subtype_shallow(flat[j], flat[i]) {
                    keep &= !(1u64 << i);
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep & (1u64 << read) != 0 {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
    }

    /// Distribute an intersection over unions: A & (B | C) → (A & B) | (A & C)
    ///
    /// This is a critical normalization rule for the Judge layer that enables
    /// better simplification and canonical form detection.
    ///
    /// # Cardinality Guard
    /// To prevent exponential explosion (e.g., (A|B) & (C|D) & (E|F)...),
    /// we limit distribution to cases where the resulting union would have ≤ 25 members.
    ///
    /// # Returns
    /// - Some(result) if distribution was applied and should replace the intersection
    /// - None if no distribution occurred (no union members, or would exceed cardinality limit)
    pub(crate) fn distribute_intersection_over_unions(
        &self,
        flat: &TypeListBuffer,
    ) -> Option<TypeId> {
        // Find all union members in the intersection and calculate total combinations.
        // Two-pass approach: first compute the full cross-product size to check TS2590,
        // then apply the conservative distribution guard.
        let mut union_indices = Vec::with_capacity(flat.len());
        let mut total_combinations: usize = 1;

        for (i, &id) in flat.iter().enumerate() {
            if let Some(TypeData::Union(members)) = self.lookup(id) {
                let member_count = self.type_list(members).len();
                total_combinations = total_combinations.saturating_mul(member_count);
                union_indices.push(i);
            }
        }

        // TS2590: tsc checkCrossProductUnion bails at 100,000.
        // Must check BEFORE the conservative distribution guard so that
        // intersections like `(A|B) & (C|D) & ... & (Y|Z)` (18+ unions)
        // correctly trigger the too-complex flag even though we won't distribute.
        if total_combinations >= 100_000 {
            self.set_union_too_complex();
            return None;
        }

        // Conservative guard: skip distribution if would produce > 25 members
        if total_combinations > 25 {
            return None;
        }

        // No unions to distribute
        if union_indices.is_empty() {
            return None;
        }

        // Build the distributed union
        // Start with the first non-union member as the base
        let base_members: Vec<_> = flat
            .iter()
            .enumerate()
            .filter(|(i, _)| !union_indices.contains(i))
            .map(|(_, &id)| id)
            .collect();

        // If all members are unions, start with an empty intersection (unknown)
        let initial_intersection = if base_members.is_empty() {
            vec![]
        } else {
            base_members
        };

        // Recursively distribute: for each union, create intersections with all combinations
        let mut combinations = vec![initial_intersection];

        for &union_idx in &union_indices {
            let union_type = flat[union_idx];
            let TypeData::Union(union_members) = self.lookup(union_type)? else {
                continue;
            };
            let union_members = self.type_list(union_members);

            // For each existing combination, create new combinations with each union member
            let mut new_combinations =
                Vec::with_capacity(combinations.len().saturating_mul(union_members.len()));
            for combination in &combinations {
                for &union_member in union_members.iter() {
                    let mut new_combination = combination.clone();
                    new_combination.push(union_member);
                    new_combinations.push(new_combination);
                }
            }
            combinations = new_combinations;
        }

        // Convert each combination to an intersection TypeId
        let intersection_results: Vec<_> = combinations
            .iter()
            .map(|combination| self.intersection(combination.clone()))
            .collect();

        // Return the union of all intersections
        Some(self.union(intersection_results))
    }
}

#[cfg(test)]
#[path = "normalize_tests.rs"]
mod tests;

#[cfg(test)]
mod application_order_tests;
