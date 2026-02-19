//! Union/intersection reduction and disjointness checking.
//!
//! This module contains helper methods for:
//! - Primitive disjointness detection
//! - Object literal disjointness detection
//! - Union subtype reduction
//! - Intersection subtype reduction
//! - Intersection-over-union distribution
//! - Literal absorption into primitives

use crate::intern::{TypeInterner, TypeListBuffer};
use crate::types::{
    IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId, PropertyInfo, TypeData, TypeId,
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
            } else {
                // Check if this is an object type
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

        let mut objects: Vec<Arc<ObjectShape>> = Vec::new();

        for &member in members {
            let Some(key) = self.lookup(member) else {
                continue;
            };
            match key {
                TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                    objects.push(self.object_shape(shape_id));
                }
                _ => {}
            }
        }

        if objects.len() < 2 {
            return false;
        }

        for i in 0..objects.len() {
            for j in (i + 1)..objects.len() {
                if self.object_literals_disjoint(
                    objects[i].properties.as_slice(),
                    objects[j].properties.as_slice(),
                ) {
                    return true;
                }
            }
        }

        false
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

            // Check literal kinds for disjointness
            if let Some(left_kind) = self.literal_kind_from_type(prop.type_id)
                && let Some(right_kind) = self.literal_kind_from_type(other.type_id)
                && left_kind.is_disjoint(&right_kind)
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
    fn is_subtype_shallow(&self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }

        // Skip reduction for type parameters and lazy types
        // These need full type resolution to determine subtyping
        if matches!(
            (self.lookup(source), self.lookup(target)),
            (
                Some(TypeData::TypeParameter(_)) | _,
                Some(TypeData::TypeParameter(_))
            ) | (Some(TypeData::Lazy(_)) | _, Some(TypeData::Lazy(_)))
        ) {
            return false;
        }

        // Handle Top/Bottom types
        if target.is_any_or_unknown() {
            return true;
        }
        if source.is_never() {
            return true;
        }

        // Handle Literal to Primitive (including unions containing primitives)
        // Only if target is NOT a literal (we don't want "a" <: "b")
        if self
            .lookup(source)
            .is_some_and(|k| matches!(k, TypeData::Literal(_)))
        {
            if self
                .lookup(target)
                .is_some_and(|k| matches!(k, TypeData::Literal(_)))
            {
                // Both are literals - only subtype if identical (handled above)
                return false;
            }

            // Check if target is a union containing a compatible primitive
            if let Some(TypeData::Union(members)) = self.lookup(target) {
                let members = self.type_list(members);
                // A literal is a subtype of a union if it's a subtype of ANY member
                for &member in members.iter() {
                    if self.is_subtype_shallow(source, member) {
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

        // Handle Objects (Shallow structural check)
        // Uses TypeId equality for properties to avoid recursion.
        // Supports width subtyping (source can have extra properties).
        // Skips index signatures (too complex for shallow check).
        let s_key = self.lookup(source);
        let t_key = self.lookup(target);
        match (s_key, t_key) {
            (
                Some(TypeData::Object(s_id) | TypeData::ObjectWithIndex(s_id)),
                Some(TypeData::Object(t_id) | TypeData::ObjectWithIndex(t_id)),
            ) => self.is_object_shape_subtype_shallow(s_id, t_id),
            _ => false,
        }
    }

    /// Shallow object shape subtype check.
    ///
    /// Compares properties using `TypeId` equality (no recursion) to enable
    /// safe object reduction in unions/intersections without infinite recursion.
    ///
    /// ## Subtyping Rules:
    /// - **Width subtyping**: Source can have extra properties
    /// - **Type Identity**: Common properties must have identical `TypeIds` (no deep check)
    /// - **Optional**: Required <: Optional is true, Optional <: Required is false
    /// - **Readonly**: Mutable <: Readonly is true, Readonly <: Mutable is false
    /// - **Nominal**: If target has a symbol, source must have the same symbol
    /// - **Index Signatures**: Skipped (too complex for shallow check)
    ///
    /// ## Example Reductions:
    /// - `{a: 1} | {a: 1, b: 2}` → `{a: 1}` (a absorbs a, b)
    /// - `{a: 1, b: 2} & {a: 1}` → `{a: 1, b: 2}` (keeps more specific)
    ///
    /// Uses O(N+M) two-pointer scan since properties are sorted by Atom.
    fn is_object_shape_subtype_shallow(&self, s_id: ObjectShapeId, t_id: ObjectShapeId) -> bool {
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

                // Rule: Type Identity (no recursion)
                if sp.type_id != t_prop.type_id {
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

        // OPTIMIZATION: Skip reduction if all types are unit types or non-reducible structures.
        // Unit types are disjoint. Arrays, tuples, and objects always return false in is_subtype_shallow
        // unless they are identical (handled by dedup) or one is a structural subtype.
        if len > 2 {
            let all_non_reducible = flat.iter().all(|&ty| {
                if self.is_unit_type(ty) {
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
                    )
                )
            });
            if all_non_reducible {
                return;
            }
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

        // Mark redundant elements, then compact in one pass.
        let mut keep = vec![true; len];
        for i in 0..len {
            if !keep[i] {
                continue;
            }
            for j in 0..len {
                if i == j || !keep[j] {
                    continue;
                }
                // If i is a subtype of j, i is redundant in a union
                if self.is_subtype_shallow(flat[i], flat[j]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep[read] {
                flat[write] = flat[read];
                write += 1;
            }
        }
        flat.truncate(write);
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
                    shape
                        .properties
                        .binary_search_by_key(&discriminant_prop, |p| p.name)
                        .ok()
                        .map(|idx| shape.properties[idx].type_id)
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

        // Mark redundant elements, then compact in one pass.
        // This avoids O(n) Vec::remove() per element (which shifts all subsequent items).
        let len = flat.len();
        let mut keep = vec![true; len];
        for i in 0..len {
            if !keep[i] {
                continue;
            }
            for j in 0..len {
                if i == j || !keep[j] {
                    continue;
                }
                // If j is a subtype of i, i is the supertype and redundant in an intersection
                if self.is_subtype_shallow(flat[j], flat[i]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep[read] {
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
