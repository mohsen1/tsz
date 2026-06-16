//! Union/intersection reduction and disjointness checking.
//!
//! This module contains helper methods for:
//! - Primitive disjointness detection
//! - Object literal disjointness detection
//! - Union subtype reduction
//! - Intersection subtype reduction
//! - Intersection-over-union distribution
//! - Literal absorption into primitives

use super::{TypeInterner, TypeListBuffer};
use crate::types::{
    CallableShape, IntrinsicKind, LiteralValue, ObjectShape, PropertyInfo, TemplateLiteralId,
    TypeData, TypeId, Visibility,
};
use crate::visitor::is_literal_type;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::sync::Arc;
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
        // Performance guard: skip O(N²) check for large intersections.
        // The check detects { kind: "a" } & { kind: "b" } → never, but for very
        // large type intersections (e.g., T extends A & B & C & ...), the O(N²)
        // pairwise comparison is prohibitively expensive. Skip it and let the
        // merged object handle any conflicts.
        const MAX_DISJOINT_CHECK_SIZE: usize = 25;
        if members.len() > MAX_DISJOINT_CHECK_SIZE {
            return false;
        }

        // Collect property-bearing shapes from both Object AND Callable types.
        // Callable types (e.g., { (x: string): number, a: "" }) have named properties
        // that can conflict with Object type properties, reducing the intersection to never.
        let mut object_shapes: Vec<Arc<ObjectShape>> = Vec::with_capacity(members.len());
        let mut callable_shapes: Vec<Arc<CallableShape>> = Vec::with_capacity(members.len());

        for &member in members {
            if member.is_intrinsic() {
                continue;
            }
            let Some(key) = self.lookup(member) else {
                continue;
            };
            match key {
                TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                    object_shapes.push(self.object_shape(shape_id));
                }
                TypeData::Callable(callable_id) => {
                    let callable = self.callable_shape(callable_id);
                    if !callable.properties.is_empty() {
                        callable_shapes.push(callable);
                    }
                }
                _ => {}
            }
        }

        let total = object_shapes.len() + callable_shapes.len();
        if total < 2 {
            return false;
        }

        // Build property slice references for pairwise comparison
        let mut prop_slices: SmallVec<[&[PropertyInfo]; 8]> = SmallVec::new();
        for obj in &object_shapes {
            prop_slices.push(&obj.properties);
        }
        for callable in &callable_shapes {
            prop_slices.push(&callable.properties);
        }

        for i in 0..prop_slices.len() {
            for j in (i + 1)..prop_slices.len() {
                if self.object_literals_disjoint(prop_slices[i], prop_slices[j]) {
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

    fn object_literals_disjoint(&self, left: &[PropertyInfo], right: &[PropertyInfo]) -> bool {
        let (small, large) = if left.len() <= right.len() {
            (left, right)
        } else {
            (right, left)
        };

        for prop in small {
            let Some(other) = Self::find_property(large, prop.name) else {
                continue;
            };

            // Intersections cannot contain the same property as both private
            // and non-private. TypeScript reduces these to never because a
            // private member's declaring-class identity cannot be satisfied by
            // a public or protected property with the same name. Protected
            // members are intentionally left to the normal merge path because
            // protected/public intersections can expose a public property.
            if prop.visibility != other.visibility
                && (prop.visibility == Visibility::Private
                    || other.visibility == Visibility::Private)
            {
                return true;
            }

            // If BOTH are optional, the object intersection is NOT never
            // (the property itself just becomes never).
            if prop.optional && other.optional {
                continue;
            }

            // Check literal kinds for disjointness (e.g., "a" & "b", "a" & ("b" | "c"))
            if let Some(left_kind) = self.literal_kind_from_type(prop.type_id)
                && let Some(right_kind) = self.literal_kind_from_type(other.type_id)
                && left_kind.is_disjoint(&right_kind)
            {
                return true;
            }

            // Check cross-domain disjointness: literal vs incompatible primitive.
            // e.g., a: "" (string literal) & a: number → disjoint (string ≠ number domain).
            // Only fires when at least one side is a literal/unit type, matching tsc's
            // discriminant-based intersection reduction.
            if prop.type_id != other.type_id
                && (self.is_literal(prop.type_id) || self.is_literal(other.type_id))
                && let (Some(a_class), Some(b_class)) = (
                    self.primitive_class_for(prop.type_id),
                    self.primitive_class_for(other.type_id),
                )
                && a_class != b_class
            {
                return true;
            }
        }

        false
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

    fn find_property(props: &[PropertyInfo], name: Atom) -> Option<&PropertyInfo> {
        props
            .binary_search_by(|prop| prop.name.cmp(&name))
            .ok()
            .map(|idx| &props[idx])
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

    /// A bare unit literal: `TypeData::Literal(_)` (string/number/boolean/bigint
    /// literal). For two *distinct* bare unit literals `a != b`, the shallow
    /// subtype engine is unconditionally `false` in **both** directions:
    /// - As a source, a `Literal(_)` relates only to a widened primitive of its
    ///   own domain, a matching template literal, or a union/builtin containing
    ///   one of those — never to another `Literal(_)` (see
    ///   `is_subtype_shallow_depth`'s literal-source branch).
    /// - As a target, a `Literal(_)` is matched only by an identical literal
    ///   (removed upstream by dedup) or `never` (removed upstream) — no other
    ///   source is ever a subtype of a bare literal.
    ///
    /// So in the union-reduction pairwise sweep, a pair where **both** members
    /// are bare unit literals can never reduce either way; the subtype call is
    /// pure waste. Skipping such pairs collapses the dominant O(L²)
    /// literal-vs-literal term of a `{ primitive, Literal:L }` union (the
    /// residual ts-toolbelt #13250 signature) to O(L·K) useful checks against
    /// the K non-literal members, with identical reduction output. Boolean
    /// literals (`true`/`false`) are intrinsics, not `TypeData::Literal`, so
    /// they are conservatively treated as non-literals and keep full checks.
    fn is_bare_unit_literal(&self, id: TypeId) -> bool {
        matches!(self.lookup(id), Some(TypeData::Literal(_)))
    }

    /// Remove redundant types from a union using shallow subtype checks.
    /// If A <: B, then A | B = B (A is redundant).
    pub(crate) fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }

        // Lift out *unevaluated deferred operations* (`Conditional` / `IndexAccess`)
        // before the pairwise sweep. The shallow subtype engine has no rules for
        // them: such a member never relates to any other member as source or
        // target except by identity (already removed by the upstream dedup), so it
        // can neither be removed by reduction nor cause another member's removal —
        // it is fully inert. Running the O(N) pairwise scan over each one is pure
        // waste; that is the eager-vs-deferred divergence #13242 targets, where
        // distributing `Exclude`/`Extract` over a wide union yields a union of
        // deferred conditionals tsc keeps lazy. Partition them aside, reduce only
        // the reducible remainder through the *unchanged* engine below (so
        // discriminant partitioning, literal absorption, and structural object
        // reduction are all preserved exactly as for a deferred-free union), then
        // splice the deferred members back. An all-deferred union short-circuits
        // to zero pairwise checks; a mixed union (e.g. a JSX intrinsic-element
        // props union carrying an `IndexAccess` arm) still reduces its concrete
        // members. Reducing only the remainder — rather than skipping reduction
        // for the whole union when any deferred member is present — is what keeps
        // the concrete arms collapsing the way tsc does.
        let deferred_count = flat
            .iter()
            .filter(|&&id| {
                matches!(
                    self.lookup(id),
                    Some(TypeData::Conditional(_) | TypeData::IndexAccess(_, _))
                )
            })
            .count();
        if deferred_count > 0 {
            if deferred_count == len {
                // Every member is an inert deferred form: nothing to reduce.
                return;
            }
            let mut deferred: TypeListBuffer = SmallVec::new();
            let mut reducible: TypeListBuffer = SmallVec::new();
            for &id in flat.iter() {
                if matches!(
                    self.lookup(id),
                    Some(TypeData::Conditional(_) | TypeData::IndexAccess(_, _))
                ) {
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

        // Small unions use a u64 bitset; larger partition misses use Vec<bool>.
        if len <= 64 {
            self.reduce_union_subtypes_quadratic(flat);
        } else {
            let mut keep = vec![true; len];
            // Bare-unit-literal mask (see `is_bare_unit_literal`): a pair whose
            // endpoints are both literals never reduces either way, so the
            // shallow subtype call is skipped for it. One O(N) classification
            // pass replaces the O(L²) literal-vs-literal term of the sweep.
            let literal: Vec<bool> = flat
                .iter()
                .map(|&id| self.is_bare_unit_literal(id))
                .collect();
            let shallow_checks = tsz_common::perf_counters::enabled_fast().then(|| {
                &tsz_common::perf_counters::counters().union_subtype_reduction_shallow_checks
            });
            for i in 0..len {
                if !keep[i] {
                    continue;
                }
                for j in 0..len {
                    if i == j || !keep[j] {
                        continue;
                    }
                    // Both endpoints bare unit literals: provably non-subtype.
                    if literal[i] && literal[j] {
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

        // Reduce each partition independently.
        let mut result: TypeListBuffer = SmallVec::new();
        for (_, group) in partitions {
            let mut group_buf = TypeListBuffer::from_vec(group);
            self.reduce_union_subtypes_quadratic(&mut group_buf);
            result.extend(group_buf);
        }

        // Reduce fallback, then check fallback against all winners.
        if !fallback.is_empty() {
            let mut fallback_buf = TypeListBuffer::from_vec(fallback);
            self.reduce_union_subtypes_quadratic(&mut fallback_buf);
            result.extend(fallback_buf);
        }

        // Final pass only when partitioning actually reduced the problem.
        if result.len() < members.len() {
            self.reduce_union_subtypes_quadratic(&mut result);
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
        // Bare-unit-literal mask: a pair where both endpoints are literals can
        // never reduce either way (see `is_bare_unit_literal`), so skip the
        // shallow subtype call for it. One O(N) classification pass replaces the
        // O(L²) literal-vs-literal term of the sweep.
        let mut literal_mask: u64 = 0;
        for (idx, &id) in flat.iter().enumerate() {
            if self.is_bare_unit_literal(id) {
                literal_mask |= 1u64 << idx;
            }
        }
        let shallow_checks = tsz_common::perf_counters::enabled_fast()
            .then(|| &tsz_common::perf_counters::counters().union_subtype_reduction_shallow_checks);
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            let i_is_literal = literal_mask & (1u64 << i) != 0;
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
                }
                // Both endpoints bare unit literals: provably non-subtype.
                if i_is_literal && literal_mask & (1u64 << j) != 0 {
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
mod tests {
    use super::*;
    use crate::types::TemplateSpan;

    #[test]
    fn shallow_subtype_skips_literal_to_template_literal_matching() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("foo-x");
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("foo-")),
            TemplateSpan::Type(TypeId::STRING),
        ]);

        assert!(
            !interner.is_subtype_shallow(literal, template),
            "union normalization should not invoke full template-literal subtype matching"
        );
    }

    #[test]
    fn union_template_absorbs_matching_string_literal() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("foo-1");
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("foo-")),
            TemplateSpan::Type(TypeId::NUMBER),
        ]);

        // `"foo-1" | `foo-${number}`` reduces to the template member.
        assert_eq!(interner.union(vec![literal, template]), template);
    }

    #[test]
    fn union_template_keeps_non_matching_string_literal() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("bar-1");
        let template = interner.template_literal(vec![
            TemplateSpan::Text(interner.intern_string("foo-")),
            TemplateSpan::Type(TypeId::NUMBER),
        ]);

        let union = interner.union(vec![literal, template]);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected a two-member union to survive normalization");
        };
        assert_eq!(interner.type_list(list_id).len(), 2);
    }

    #[test]
    fn union_template_without_leading_text_absorbs_literal() {
        let interner = TypeInterner::new();
        let literal = interner.literal_string("12px");
        let template = interner.template_literal(vec![
            TemplateSpan::Type(TypeId::NUMBER),
            TemplateSpan::Text(interner.intern_string("px")),
        ]);

        // No leading-text prefilter applies; the full shallow match still runs.
        assert_eq!(interner.union(vec![literal, template]), template);
    }

    #[test]
    fn union_string_placeholder_template_keeps_literals_shallow() {
        let interner = TypeInterner::new();
        // Mirrors the lib.dom `AutoFill` family shape: many plain literals next
        // to `${string}`-placeholder templates. Shallow matching does not match
        // `${string}` placeholders, so every member must survive.
        let members = vec![
            interner.literal_string("name"),
            interner.literal_string("billing name"),
            interner.template_literal(vec![
                TemplateSpan::Text(interner.intern_string("section-")),
                TemplateSpan::Type(TypeId::STRING),
                TemplateSpan::Text(interner.intern_string(" name")),
            ]),
        ];
        let union = interner.union(members);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected union to survive normalization");
        };
        assert_eq!(interner.type_list(list_id).len(), 3);
    }

    fn distinct_conditional(interner: &TypeInterner, n: u32) -> TypeId {
        // A deferred, distributive conditional whose check type is unique per `n`
        // so each interns to a separate `TypeData::Conditional`. This is the shape
        // produced by distributing `Exclude`/`Extract` over a wide union before
        // the conditionals resolve.
        interner.conditional(crate::types::ConditionalType {
            check_type: interner.literal_number(f64::from(n)),
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: true,
        })
    }

    #[test]
    fn union_of_deferred_conditionals_all_survive_reduction() {
        let interner = TypeInterner::new();
        // The shallow subtype engine cannot relate two deferred conditionals, so a
        // union of distinct unevaluated conditionals is irreducible: every member
        // must survive. (The pairwise sweep over them is also skipped, but the
        // perf counter is only wired under `TSZ_PERF_COUNTERS`, so the observable
        // unit-test invariant is the surviving member set.)
        const N: u32 = 64;
        let members: Vec<TypeId> = (0..N).map(|i| distinct_conditional(&interner, i)).collect();
        let union = interner.union(members);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected a wide deferred-conditional union to survive normalization");
        };
        assert_eq!(interner.type_list(list_id).len(), N as usize);
    }

    #[test]
    fn deferred_conditional_does_not_block_concrete_subtype_reduction() {
        let interner = TypeInterner::new();
        // A union mixing an inert deferred conditional with concrete members where
        // one is a subtype of another (`"a"` <: `string`). The deferred member is
        // inert and must be preserved, but it must NOT suppress the concrete
        // reduction: `"a"` is absorbed into `string`, leaving `string` + the
        // conditional. This is the JSX-props regression in miniature — folding the
        // deferred member into a whole-union skip wrongly kept `"a"`.
        let cond = distinct_conditional(&interner, 0);
        let members = vec![interner.literal_string("a"), TypeId::STRING, cond];
        let union = interner.union(members);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected a mixed deferred/concrete union to survive normalization");
        };
        let list = interner.type_list(list_id);
        assert_eq!(
            list.len(),
            2,
            "concrete `\"a\" | string` must reduce to `string` beside the inert conditional"
        );
        assert!(
            list.contains(&TypeId::STRING),
            "widened `string` must survive"
        );
        assert!(
            list.contains(&cond),
            "inert deferred conditional must survive"
        );
        assert!(
            !list.contains(&interner.literal_string("a")),
            "literal `\"a\"` must be absorbed by `string`"
        );
    }

    #[test]
    fn cross_domain_primitive_and_literals_all_survive_reduction() {
        let interner = TypeInterner::new();
        // `number | "m0" | "m1" | ... | "mN-1"`: the primitive (`number`) does
        // not absorb the string literals (different domain), and distinct string
        // literals are mutually non-subtypes. The literal-vs-literal pairs are
        // skipped by the bare-unit-literal mask, but the surviving member set
        // must be identical to a full pairwise sweep: every member survives.
        const N: usize = 50;
        let mut members = vec![TypeId::NUMBER];
        for i in 0..N {
            members.push(interner.literal_string(&format!("m{i}")));
        }
        let union = interner.union(members);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected a `number | <string literals>` union to survive");
        };
        let list = interner.type_list(list_id);
        assert_eq!(
            list.len(),
            N + 1,
            "no member of a cross-domain primitive + distinct-literals union reduces"
        );
        assert!(list.contains(&TypeId::NUMBER), "`number` must survive");
    }

    #[test]
    fn same_domain_primitive_absorbs_all_literals_with_mask() {
        let interner = TypeInterner::new();
        // `string | "s0" | ... | "sN-1"` must still collapse to just `string`.
        // Absorption runs before the pairwise sweep, so the literal mask never
        // changes this outcome — guards against the skip leaking into the
        // absorb path.
        const N: usize = 50;
        let mut members = vec![TypeId::STRING];
        for i in 0..N {
            members.push(interner.literal_string(&format!("s{i}")));
        }
        assert_eq!(
            interner.union(members),
            TypeId::STRING,
            "widened `string` absorbs every string literal regardless of the literal mask"
        );
    }

    #[test]
    fn literal_mask_preserves_object_vs_literal_reduction() {
        let interner = TypeInterner::new();
        // A union mixing distinct string literals with two structurally-related
        // objects where one reduces into the other. Literal-vs-literal pairs are
        // skipped, but the object-vs-object reduction (a non-literal pair) must
        // still fire: `{ a: 1 }` is absorbed by the wider `{ a: 1; b: 2 }`? No —
        // width subtyping makes the *narrower-keyed* object the supertype, so
        // `{ a: 1; b: 2 } <: { a: 1 }` and the wider one is removed. The literals
        // are irreducible and all survive.
        let obj_narrow = interner.object(vec![PropertyInfo::new(
            interner.intern_string("a"),
            interner.literal_number(1.0),
        )]);
        let obj_wide = interner.object(vec![
            PropertyInfo::new(interner.intern_string("a"), interner.literal_number(1.0)),
            PropertyInfo::new(interner.intern_string("b"), interner.literal_number(2.0)),
        ]);
        let members = vec![
            interner.literal_string("x"),
            interner.literal_string("y"),
            obj_narrow,
            obj_wide,
        ];
        let union = interner.union(members);
        let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
            panic!("expected the mixed literal/object union to survive as a union");
        };
        let list = interner.type_list(list_id);
        // Two literals survive; the object pair reduces to a single member.
        assert_eq!(
            list.len(),
            3,
            "object-vs-object reduction must still fire beside skipped literal pairs"
        );
        assert!(list.contains(&interner.literal_string("x")));
        assert!(list.contains(&interner.literal_string("y")));
    }
}
