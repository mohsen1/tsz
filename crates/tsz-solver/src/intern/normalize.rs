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
    CallableShape, FunctionShapeId, IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId,
    ParamInfo, PropertyInfo, TypeData, TypeId,
};
use crate::visitor::is_literal_type;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::sync::Arc;
use tsz_common::interner::Atom;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PrimitiveClass {
    String,
    Number,
    Boolean,
    Bigint,
    Symbol,
    Null,
    Undefined,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LiteralDomain {
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
            } else {
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
            if member == TypeId::OBJECT {
                has_object_intrinsic = true;
            } else if let Some(TypeData::Intrinsic(IntrinsicKind::Object)) = self.lookup(member) {
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
            } else if let Some(TypeData::TypeParameter(ref info)) = self.lookup(member)
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
                Some(TypeData::TypeParameter(ref info)) => {
                    if info.constraint.is_some()
                        && !constrained.iter().any(|(n, _)| *n == info.name)
                    {
                        constrained.push((info.name, member));
                    }
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

        // Dedup after replacement (may have introduced duplicates).
        if changed {
            flat.dedup();
        }
    }

    /// Check if a type is clearly non-nullable (cannot include null/undefined).
    ///
    /// Returns true for:
    /// - Primitive types: string, number, boolean, bigint, symbol
    /// - The `object` intrinsic
    /// - Structural object types, arrays, tuples, functions, callables
    /// - Literal types (string/number/boolean/bigint literals)
    ///
    /// Returns false for:
    /// - null, undefined, void, any, unknown, never
    /// - Union types (might contain nullable members)
    /// - Type parameters (constraint may be nullable)
    /// - Lazy/Application/Mapped (unresolved, can't determine)
    fn is_clearly_non_nullable_constraint(&self, id: TypeId) -> bool {
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
            _ => matches!(
                self.lookup(id),
                Some(
                    TypeData::Literal(_)
                        | TypeData::Object(_)
                        | TypeData::ObjectWithIndex(_)
                        | TypeData::Array(_)
                        | TypeData::Tuple(_)
                        | TypeData::Function(_)
                        | TypeData::Callable(_)
                        | TypeData::TemplateLiteral(_)
                        | TypeData::UniqueSymbol(_)
                )
            ),
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

    fn unit_values_are_disjoint(&self, left: &UnitValueKey, right: &UnitValueKey) -> bool {
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
                    self.unit_values_are_disjoint(key_a, key_b)
                }
            }
            (Enum(_, key), other) | (other, Enum(_, key)) => {
                self.unit_values_are_disjoint(key, other)
            }
            _ => true,
        }
    }

    pub(crate) fn intersection_has_disjoint_unit_values(&self, members: &[TypeId]) -> bool {
        let mut seen = Vec::new();

        for &member in members {
            let Some(key) = self.get_unit_value_key(member) else {
                continue;
            };
            if seen
                .iter()
                .any(|existing| self.unit_values_are_disjoint(existing, &key))
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
        let mut object_shapes: Vec<Arc<ObjectShape>> = Vec::new();
        let mut callable_shapes: Vec<Arc<CallableShape>> = Vec::new();

        for &member in members {
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
                brand_sets.push(brands);
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

    fn literal_domain_from_type(&self, type_id: TypeId) -> Option<LiteralDomain> {
        self.literal_kind_from_type(type_id).map(|k| k.domain())
    }

    fn find_property(props: &[PropertyInfo], name: Atom) -> Option<&PropertyInfo> {
        props
            .binary_search_by(|prop| prop.name.cmp(&name))
            .ok()
            .map(|idx| &props[idx])
    }

    fn primitive_class_for(&self, type_id: TypeId) -> Option<PrimitiveClass> {
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
            TypeData::TemplateLiteral(_) => Some(PrimitiveClass::String),
            _ => None,
        }
    }

    /// Shallow subtype check that avoids infinite recursion.
    /// Uses `TypeId` identity for nested components instead of recursive checking.
    /// This is safe for use during normalization because it only uses `lookup()` and
    /// never calls `intern()` or `evaluate()`.
    #[inline]
    fn is_subtype_shallow(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_shallow_depth(source, target, 3)
    }

    /// Depth-limited shallow subtype check. Handles primitives, literals, objects,
    /// and function types. The depth parameter limits recursion through object
    /// properties (each level allows one more structural comparison).
    fn is_subtype_shallow_depth(&self, source: TypeId, target: TypeId, depth: u32) -> bool {
        if source == target {
            return true;
        }
        if depth == 0 {
            return false;
        }

        // Handle Top/Bottom types (no lookup needed)
        if target.is_any_or_unknown() {
            return true;
        }
        if source.is_never() {
            return true;
        }

        // Single lookup per type — reuse throughout the function
        let s_data = self.lookup(source);
        let t_data = self.lookup(target);

        // Skip reduction for type parameters and lazy types
        if matches!(
            (&s_data, &t_data),
            (
                Some(TypeData::TypeParameter(_)) | _,
                Some(TypeData::TypeParameter(_))
            ) | (Some(TypeData::Lazy(_)) | _, Some(TypeData::Lazy(_)))
        ) {
            return false;
        }

        // Handle Literal to Primitive (including unions containing primitives)
        if matches!(s_data, Some(TypeData::Literal(_))) {
            if matches!(t_data, Some(TypeData::Literal(_))) {
                // Both are literals - only subtype if identical (handled above)
                return false;
            }

            // Check if target is a union containing a compatible primitive
            if let Some(TypeData::Union(members)) = t_data {
                let members = self.type_list(members);
                for &member in members.iter() {
                    if self.is_subtype_shallow_depth(source, member, depth) {
                        return true;
                    }
                }
                return false;
            }

            // Otherwise, check literal-to-primitive compatibility
            if let Some(domain) = self.literal_domain_from_type(source)
                && let Some(target_class) = self.primitive_class_for(target)
                && self.literal_domain_matches_primitive(domain, target_class)
            {
                return true;
            }
        }

        // Handle source as member of target union (for built-in/primitive types only).
        if self.is_builtin_type(source)
            && let Some(TypeData::Union(members)) = t_data
        {
            let members = self.type_list(members);
            return members.contains(&source);
        }

        // Handle source union: every member must be a subtype of the target.
        // This enables reduction of unions like `('hello' | undefined) <: (string | undefined)`
        // which arise from optional parameter types in function subtype checks.
        if let Some(TypeData::Union(s_members)) = s_data {
            let s_members = self.type_list(s_members);
            // Guard: only handle small unions to avoid O(N*M) blowup
            if s_members.len() <= 8 {
                return s_members
                    .iter()
                    .all(|&m| self.is_subtype_shallow_depth(m, target, depth - 1));
            }
            return false;
        }

        // Handle non-literal, non-builtin source against target union.
        // Generalizes the existing literal and builtin checks above to cover
        // cases like Function <: (Function | undefined).
        if let Some(TypeData::Union(t_members)) = t_data {
            let t_members = self.type_list(t_members);
            if t_members.len() <= 8 {
                return t_members
                    .iter()
                    .any(|&m| self.is_subtype_shallow_depth(source, m, depth - 1));
            }
            return false;
        }

        // Handle structural type comparisons
        match (s_data, t_data) {
            (
                Some(TypeData::Object(s_id) | TypeData::ObjectWithIndex(s_id)),
                Some(TypeData::Object(t_id) | TypeData::ObjectWithIndex(t_id)),
            ) => self.is_object_shape_subtype_shallow_depth(s_id, t_id, 0),
            (Some(TypeData::Function(s_id)), Some(TypeData::Function(t_id))) => {
                self.is_function_subtype_shallow(s_id, t_id, depth)
            }
            _ => false,
        }
    }

    /// Shallow object shape subtype check with depth-limited property comparison.
    ///
    /// At depth > 0, property types are compared using `is_subtype_shallow_depth`,
    /// enabling reduction of objects whose properties differ structurally
    /// (e.g., `{ f(): void } | { f(x?: string): void }`).
    /// At depth 0, falls back to `TypeId` equality for properties.
    ///
    /// ## Subtyping Rules:
    /// - **Width subtyping**: Source can have extra properties
    /// - **Type comparison**: TypeId equality first, then depth-limited structural check
    /// - **Optional**: Required <: Optional is true, Optional <: Required is false
    /// - **Readonly**: Mutable <: Readonly is true, Readonly <: Mutable is false
    /// - **Nominal**: If target has a symbol, source must have the same symbol
    /// - **Index Signatures**: Skipped (too complex for shallow check)
    ///
    /// Uses O(N+M) two-pointer scan since properties are sorted by Atom.
    fn is_object_shape_subtype_shallow_depth(
        &self,
        s_id: ObjectShapeId,
        t_id: ObjectShapeId,
        depth: u32,
    ) -> bool {
        if s_id == t_id {
            return true;
        }

        let s = self.object_shape(s_id);
        let t = self.object_shape(t_id);

        // 1. Nominal check: if target is a class instance, source must match
        if t.symbol.is_some() && s.symbol != t.symbol {
            return false;
        }

        // 2. Conservative: Index signatures make subtyping complex (deferred to Solver)
        if t.string_index.is_some() || t.number_index.is_some() {
            return false;
        }

        // 3. Structural scan: Source must satisfy all Target properties.
        // Also tracks if we found ANY property overlap. If source and target have
        // completely disjoint properties, they are not in a subtype relationship.
        // Properties are sorted by Atom, so use two-pointer scan for O(N+M).
        let mut s_idx = 0;
        let s_props = &s.properties;
        let t_props = &t.properties;
        let mut has_any_overlap = false;

        for t_prop in t_props {
            // Advance source pointer to match target property name
            while s_idx < s_props.len() && s_props[s_idx].name < t_prop.name {
                s_idx += 1;
            }

            if s_idx < s_props.len() && s_props[s_idx].name == t_prop.name {
                let sp = &s_props[s_idx];
                has_any_overlap = true;

                // Type comparison: try TypeId equality first, then depth-limited structural check
                if sp.type_id != t_prop.type_id
                    && !self.is_subtype_shallow_depth(sp.type_id, t_prop.type_id, depth)
                {
                    return false;
                }

                // Rule: Required <: Optional (Optional <: Required is False)
                if !t_prop.optional && sp.optional {
                    return false;
                }

                // Rule: Mutable <: Readonly (Readonly <: Mutable is False)
                if !t_prop.readonly && sp.readonly {
                    return false;
                }

                s_idx += 1;
            } else {
                // Property missing in source: only allowed if target property is optional
                if !t_prop.optional {
                    return false;
                }
            }
        }

        // Disjoint properties check: must have at least one overlapping property
        // (matching tsc's reduction logic for unrelated object types).
        has_any_overlap
    }

    /// Shallow function subtype check for union reduction.
    ///
    /// Implements TypeScript's function subtyping rules:
    /// - Source can have fewer params than target (callback parameter compatibility)
    /// - Extra source params must be optional (otherwise source requires more args)
    /// - Parameters are checked contravariantly (target param type <: source param type)
    /// - Return type is checked covariantly (source return type <: target return type)
    /// - Handles optional vs required params with `| undefined` equivalence
    /// - Skips generic functions (too complex for shallow check)
    fn is_function_subtype_shallow(
        &self,
        s_id: FunctionShapeId,
        t_id: FunctionShapeId,
        depth: u32,
    ) -> bool {
        if s_id == t_id {
            return true;
        }

        let s = self.function_shape(s_id);
        let t = self.function_shape(t_id);

        // Skip generic functions
        if !s.type_params.is_empty() || !t.type_params.is_empty() {
            return false;
        }

        // this-type must match (different `this` types = different function types)
        if s.this_type != t.this_type {
            return false;
        }

        // Return type: covariant (source return <: target return)
        if s.return_type != t.return_type
            && !self.is_subtype_shallow_depth(s.return_type, t.return_type, depth)
        {
            return false;
        }

        // Check params in the shared range contravariantly
        let min_len = s.params.len().min(t.params.len());
        for i in 0..min_len {
            if !self.param_contravariant_shallow(&t.params[i], &s.params[i], depth) {
                return false;
            }
        }

        // Source cannot have more total params than target (even optional ones).
        // In tsc's subtype relation, `(x?: string) => void` is NOT a subtype
        // of `() => void` — having extra params (even optional) prevents subtyping.
        // But source with FEWER params IS a subtype (callback compatibility).
        if s.params.len() > t.params.len() {
            return false;
        }

        // Conservative guard for overload-like function pairs:
        // If all overlapping params have identical TypeIds, the functions look like
        // overload variants of the same method (e.g., `reduce(cb)` vs `reduce(cb, init)`).
        // Don't reduce these — even though one is technically a subtype, removing it
        // can break contextual typing and overload resolution.
        //
        // This guard allows reduction when param types actually differ, which is the
        // pattern in unionTypeReduction2: `(x: string|undefined) => void <: (x?: string) => void`
        // where the param TypeIds differ (string|undefined vs string).
        if min_len > 0 {
            let all_params_identical =
                (0..min_len).all(|i| s.params[i].type_id == t.params[i].type_id);
            if all_params_identical {
                return false;
            }
        }

        true
    }

    /// Check contravariant parameter compatibility for function subtyping.
    ///
    /// For `S <: T`, parameters are checked contravariantly: `T_param <: S_param`.
    /// Handles the optional/required distinction where `x?: T` has effective type
    /// `T | undefined` and `x: T | undefined` is equivalent.
    fn param_contravariant_shallow(
        &self,
        t_param: &ParamInfo,
        s_param: &ParamInfo,
        depth: u32,
    ) -> bool {
        let t_type = t_param.type_id;
        let s_type = s_param.type_id;

        if t_type == s_type {
            // Same base types. Check optional/required compatibility.
            if t_param.optional && !s_param.optional {
                // t_effective = type | undefined, s_effective = type
                // Need: type | undefined <: type — only if type contains undefined
                return self.type_contains_undefined(s_type);
            }
            return true;
        }

        // Types differ. Check effective type subtyping based on optionality.
        match (t_param.optional, s_param.optional) {
            (false, false) => {
                // Both required: t_type <: s_type
                self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
            (true, true) => {
                // Both optional: t_type | undef <: s_type | undef
                // Reduces to: t_type <: s_type | undef, which holds if t_type <: s_type
                self.is_subtype_shallow_depth(t_type, s_type, depth) || t_type == TypeId::UNDEFINED
            }
            (false, true) => {
                // t required, s optional: t_type <: s_type | undef
                // Holds if t_type <: s_type (since s_type ⊂ s_type | undef)
                self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
            (true, false) => {
                // t optional, s required: t_type | undef <: s_type
                // Need both: t_type <: s_type AND undefined <: s_type
                self.type_contains_undefined(s_type)
                    && self.is_subtype_shallow_depth(t_type, s_type, depth)
            }
        }
    }

    /// Check if a type is a built-in primitive (string, number, boolean, etc.).
    /// These are safe to check against union targets without risk of cascading
    /// reductions that affect complex type inference.
    const fn is_builtin_type(&self, id: TypeId) -> bool {
        matches!(
            id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::VOID
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        )
    }

    /// Check if a type contains `undefined` (either is `undefined` or is a
    /// union containing `undefined`). Uses only `lookup()`, safe during normalization.
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED {
            return true;
        }
        if let Some(TypeData::Union(members)) = self.lookup(type_id) {
            let members = self.type_list(members);
            return members.contains(&TypeId::UNDEFINED);
        }
        false
    }

    /// Check if a literal domain matches a primitive class.
    const fn literal_domain_matches_primitive(
        &self,
        domain: LiteralDomain,
        class: PrimitiveClass,
    ) -> bool {
        matches!(
            (domain, class),
            (LiteralDomain::String, PrimitiveClass::String)
                | (LiteralDomain::Number, PrimitiveClass::Number)
                | (LiteralDomain::Boolean, PrimitiveClass::Boolean)
                | (LiteralDomain::Bigint, PrimitiveClass::Bigint)
        )
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

    /// Remove redundant types from a union using shallow subtype checks.
    /// If A <: B, then A | B = B (A is redundant).
    pub(crate) fn reduce_union_subtypes(&self, flat: &mut TypeListBuffer) {
        let len = flat.len();
        if len <= 1 {
            return;
        }

        // Skip reduction if all types are identity-comparable or non-reducible structures.
        // tsc's default union reduction (UnionReduction.Literal) does NOT remove structural
        // subtypes — it only absorbs literals into primitives. Structural subtype reduction
        // (UnionReduction.Subtype) is only used in specific contexts like conditional type
        // results. Object/Array/Tuple/Enum types are structurally distinct after dedup, so
        // subtype reduction would incorrectly collapse unions like `{A: number} | {A: number; B: number}`.
        //
        // Lazy/Application/Callable types are also non-reducible because `is_subtype_shallow`
        // returns false for them — they require full type resolution. Including them here
        // avoids O(N²) wasted work in unions of class types (which are Lazy at this stage).
        {
            let mut has_primitive = false;
            let all_non_reducible = flat.iter().all(|&ty| {
                if self.is_identity_comparable_type(ty) {
                    // Track whether a widened primitive is present.
                    // If so, literals of that kind ARE reducible (absorbed by the primitive).
                    if ty == TypeId::STRING
                        || ty == TypeId::NUMBER
                        || ty == TypeId::BOOLEAN
                        || ty == TypeId::BIGINT
                        || ty == TypeId::SYMBOL
                    {
                        has_primitive = true;
                    }
                    return true;
                }
                matches!(
                    self.lookup(ty),
                    Some(
                        TypeData::Array(_)
                            | TypeData::Tuple(_)
                            | TypeData::Object(_)
                            | TypeData::ObjectWithIndex(_)
                            | TypeData::Enum(_, _)
                            | TypeData::Lazy(_)
                            | TypeData::Application(_)
                            | TypeData::Callable(_)
                            // Literals without a widened primitive peer are non-reducible
                            // (no literal is a subtype of a different literal).
                            | TypeData::Literal(_)
                    )
                )
            });
            if all_non_reducible && !has_primitive {
                return;
            }
        }

        // TS2590: Expression produces a union type that is too complex to represent.
        // tsc's removeSubtypes counts pairwise iterations and bails at 1,000,000.
        // For n types the worst-case count is n*(n-1), so n >= 1001 hits the limit.
        // Set the flag for the checker to detect, but don't collapse to ERROR here —
        // internal type construction (template literals, etc.) may legitimately create
        // large unions. The checker decides whether to treat it as a diagnostic.
        let pairwise = (len as u64) * (len as u64 - 1);
        if pairwise >= 1_000_000 {
            self.set_union_too_complex();
            return; // skip reduction, preserve the union members as-is
        }

        // OPTIMIZATION: Property-based partitioning for large object unions.
        // For discriminated unions (common in CFA), members are disjoint based on a property value.
        // Partitioning avoids O(N²) comparisons across disjoint groups.
        if len > 16
            && let Some(partitioned) = self.try_partition_union_reduction(flat)
        {
            *flat = partitioned;
            return;
        }

        // For small unions (common case), delegate to the u64-bitset implementation.
        // For larger unions (rare, when partition fails), fall back to Vec<bool>.
        if len <= 64 {
            self.reduce_union_subtypes_quadratic(flat);
        } else {
            let mut keep = vec![true; len];
            for i in 0..len {
                if !keep[i] {
                    continue;
                }
                for j in 0..len {
                    if i == j || !keep[j] {
                        continue;
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

    /// Try to reduce a large union by partitioning members by a discriminant property.
    /// Returns `Some(reduced_vec)` if partitioning was successful, None otherwise.
    fn try_partition_union_reduction(&self, members: &[TypeId]) -> Option<TypeListBuffer> {
        // 1. Identify a candidate discriminant property common to many members.
        // We look for a property that appears in at least 50% of object members.
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

        // 2. Partition members by their value for this property.
        // Non-objects and objects missing the property go into a "fallback" group.
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

        // 3. Reduce each partition independently.
        let mut result: TypeListBuffer = SmallVec::new();
        for (_, group) in partitions {
            let mut group_buf = TypeListBuffer::from_vec(group);
            self.reduce_union_subtypes_quadratic(&mut group_buf);
            result.extend(group_buf);
        }

        // 4. Reduce fallback group and then check fallback against all winners.
        if !fallback.is_empty() {
            let mut fallback_buf = TypeListBuffer::from_vec(fallback);
            self.reduce_union_subtypes_quadratic(&mut fallback_buf);
            result.extend(fallback_buf);
        }

        // Final quadratic pass if the result is still large, but usually partitioning
        // significantly reduces the remaining work.
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
        // Use a u64 bitset instead of heap-allocated Vec<bool>.
        // Safe because callers guard len (partitions are always small subsets of
        // the already-guarded union, and the direct caller caps at 25 members).
        debug_assert!(len <= 64, "reduce_union_subtypes_quadratic: len={len} > 64");
        // Initialize bitset with first `len` bits set. Guard against shift overflow at len==64.
        let mut keep: u64 = if len >= 64 {
            u64::MAX
        } else {
            (1u64 << len) - 1
        };
        for i in 0..len {
            if keep & (1u64 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u64 << j) == 0 {
                    continue;
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
        // Find all union members in the intersection and calculate total combinations
        let mut union_indices = Vec::new();
        let mut total_combinations = 1;

        for (i, &id) in flat.iter().enumerate() {
            if let Some(TypeData::Union(members)) = self.lookup(id) {
                let member_count = self.type_list(members).len();

                // Calculate total combinations: product of all union sizes
                // e.g., (A|B|C) & (D|E) → 3 * 2 = 6 combinations
                total_combinations *= member_count;

                // Conservative guard: abort early if would exceed 25 members
                if total_combinations > 25 {
                    return None; // Too many combinations, skip distribution
                }

                union_indices.push(i);
            }
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
            let mut new_combinations = Vec::new();
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
