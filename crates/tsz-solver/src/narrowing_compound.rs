//! Typeof negation, truthiness, falsy, and array narrowing.
//!
//! This module contains narrowing methods for:
//! - typeof negation (excluding types by typeof result)
//! - objectish narrowing (filtering to object-like types)
//! - truthiness narrowing (removing falsy types)
//! - falsy narrowing (keeping only falsy types)
//! - `Array.isArray()` narrowing

use crate::narrowing::NarrowingContext;
use crate::narrowing_utils::NarrowingVisitor;
use crate::subtype::{SubtypeChecker, is_subtype_of};
use crate::type_queries::{UnionMembersKind, classify_for_union_members};
use crate::types::{LiteralValue, TypeData, TypeId};
use crate::visitor::{
    TypeVisitor, intersection_list_id, literal_value, type_param_info, union_list_id,
};
use tracing::{Level, span};

impl<'a> NarrowingContext<'a> {
    /// Narrow a type by removing typeof-matching types.
    ///
    /// This is the negation of `narrow_by_typeof`.
    /// For example, narrowing `string | number` with `typeof "string"` (sense=false)
    /// yields `number`.
    pub(crate) fn narrow_by_typeof_negation(
        &self,
        source_type: TypeId,
        typeof_result: &str,
    ) -> TypeId {
        // For each typeof result, we exclude matching types
        let excluded = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "function" => {
                // Functions are more complex - handle separately
                return self.narrow_excluding_function(source_type);
            }
            "object" => {
                // typeof x !== "object": keep only types where typeof !== "object"
                // Keep: primitives (string, number, boolean, bigint, symbol), undefined, void, functions
                // Exclude: null (typeof null === "object") and object types
                let without_null = self.narrow_excluding_type(source_type, TypeId::NULL);
                return self.narrow_excluding_typeof_object(without_null);
            }
            _ => return source_type,
        };

        self.narrow_excluding_type(source_type, excluded)
    }

    /// Exclude types where `typeof` would return `"object"` from a union.
    ///
    /// This is used for the negation of `typeof x === "object"`.
    /// Keeps primitives, undefined, void, and function types.
    /// Excludes object types (objects, arrays, tuples, class instances).
    /// Note: null should already be excluded before calling this.
    fn narrow_excluding_typeof_object(&self, source_type: TypeId) -> TypeId {
        let resolved = self.resolve_type(source_type);

        // For non-union types, check if it's an object type
        let Some(members) = union_list_id(self.db, resolved) else {
            // Single type: check if typeof would be "object"
            if self.is_typeof_object(resolved) {
                return TypeId::NEVER;
            }
            return source_type;
        };

        // Filter union members: keep only non-object types
        let members = self.db.type_list(members);
        let kept: Vec<TypeId> = members
            .iter()
            .filter(|&&member| {
                let resolved_member = self.resolve_type(member);
                !self.is_typeof_object(resolved_member)
            })
            .copied()
            .collect();

        if kept.is_empty() {
            TypeId::NEVER
        } else if kept.len() == members.len() {
            source_type
        } else {
            self.db.union(kept)
        }
    }

    /// Check if a type would produce `"object"` from the `typeof` operator.
    fn is_typeof_object(&self, type_id: TypeId) -> bool {
        // Primitives and their literal types are NOT "object"
        if matches!(
            type_id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::UNDEFINED
                | TypeId::VOID
                | TypeId::NEVER
                | TypeId::ANY
                | TypeId::UNKNOWN
        ) {
            return false;
        }

        // Check type data for structural types
        if let Some(data) = self.db.lookup(type_id) {
            // Object, intersection, mapped, tuple, array: typeof === "object"
            matches!(
                data,
                TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Intersection(_)
                    | TypeData::Mapped(_)
                    | TypeData::Tuple(_)
                    | TypeData::Array(_)
            )
        } else {
            // OBJECT intrinsic: typeof === "object"
            type_id == TypeId::OBJECT
        }
    }

    /// Check if a type is definitely a primitive (can never pass instanceof).
    ///
    /// Returns true for primitive types and their literals:
    /// string, number, boolean, bigint, symbol, undefined, void, null, never
    fn is_definitely_primitive(&self, type_id: TypeId) -> bool {
        // Fast path: check intrinsic primitive types
        if matches!(
            type_id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::UNDEFINED
                | TypeId::VOID
                | TypeId::NULL
                | TypeId::NEVER
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) {
            return true;
        }

        // Check for literal types (which are primitives)
        if let Some(data) = self.db.lookup(type_id) {
            matches!(data, TypeData::Literal(_))
        } else {
            false
        }
    }

    /// Narrow a type to keep only object-like types (excluding primitives).
    ///
    /// This is used for instanceof fallback: if we're on the true branch of
    /// an instanceof check but couldn't narrow to the specific instance type,
    /// at least narrow to exclude primitives (which can never pass instanceof).
    pub(crate) fn narrow_to_objectish(&self, source_type: TypeId) -> TypeId {
        // ANY and UNKNOWN are kept as-is
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if source_type == TypeId::UNKNOWN {
            return TypeId::OBJECT;
        }

        let resolved = self.resolve_type(source_type);

        // Handle unions: filter out primitive members
        if let Some(members_id) = union_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let kept: Vec<TypeId> = members
                .iter()
                .filter(|&&member| !self.is_definitely_primitive(member))
                .copied()
                .collect();

            return match kept.len() {
                0 => TypeId::NEVER,
                1 => kept[0],
                n if n == members.len() => source_type, // All members kept
                _ => self.db.union(kept),
            };
        }

        // Non-union: check if primitive
        if self.is_definitely_primitive(resolved) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Check if a type is definitely falsy.
    ///
    /// Returns true for: null, undefined, void, false, 0, -0, `NaN`, "", 0n
    fn is_definitely_falsy(&self, type_id: TypeId) -> bool {
        let resolved = self.resolve_type(type_id);

        // 1. Check intrinsics that are always falsy
        if matches!(resolved, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID) {
            return true;
        }

        // 2. Check literals
        if let Some(lit) = literal_value(self.db, resolved) {
            return match lit {
                LiteralValue::Boolean(false) => true,
                LiteralValue::Number(n) => n.0 == 0.0 || n.0.is_nan(), // Handles 0, -0, and NaN
                LiteralValue::String(atom) => self.db.resolve_atom_ref(atom).is_empty(), // Handles ""
                LiteralValue::BigInt(atom) => self.db.resolve_atom_ref(atom).as_ref() == "0", // Handles 0n
                _ => false,
            };
        }

        false
    }

    /// Narrow an array's element type when using array.every(predicate).
    ///
    /// For `arr.every(isString)` where `arr: (number | string)[]` and `isString: x is string`,
    /// this narrows the array to `string[]`.
    ///
    /// Only applies to array types. Non-array types are returned unchanged.
    pub(crate) fn narrow_array_element_type(
        &self,
        source_type: TypeId,
        narrowed_element: TypeId,
    ) -> TypeId {
        use tracing::trace;

        trace!(
            ?source_type,
            ?narrowed_element,
            "narrow_array_element_type called"
        );

        let resolved = self.resolve_type(source_type);
        trace!(?resolved, "Resolved source type");

        // Check if this is an array type
        if let Some(TypeData::Array(current_elem)) = self.db.lookup(resolved) {
            trace!(?current_elem, "Found array type");
            // Narrow the element type
            let new_elem = self.narrow_to_type(current_elem, narrowed_element);
            trace!(?new_elem, "Narrowed element type");

            // Reconstruct the array with narrowed element type
            let result = self.db.array(new_elem);
            trace!(?result, "Created narrowed array type");
            return result;
        }

        // Check if this is a union - narrow each member that's an array
        if let Some(TypeData::Union(list_id)) = self.db.lookup(resolved) {
            trace!(?list_id, "Found union type");
            let members = self.db.type_list(list_id);
            trace!(?members, "Union members");
            let narrowed_members: Vec<TypeId> = members
                .iter()
                .map(|&member| self.narrow_array_element_type(member, narrowed_element))
                .collect();

            // If any members changed, create a new union
            if narrowed_members
                .iter()
                .zip(members.iter())
                .any(|(a, b)| a != b)
            {
                trace!("Union members changed, creating new union");
                return self.db.union(narrowed_members);
            }
        }

        trace!("Not an array or union of arrays, returning unchanged");
        // Not an array or union of arrays - return unchanged
        source_type
    }

    /// Narrow a type by removing definitely falsy values (truthiness check).
    ///
    /// Narrow a type to its falsy component(s).
    ///
    /// This is used for the false branch of truthiness checks (e.g., `if (!x)`).
    /// Returns the union of all falsy values that the type could be.
    ///
    /// Falsy values in TypeScript:
    /// - null, undefined, void
    /// - false (boolean literal)
    /// - 0, -0, `NaN` (number literals)
    /// - "" (empty string)
    /// - 0n (bigint literal)
    ///
    /// CRITICAL: TypeScript does NOT narrow primitive types in falsy branches.
    /// For `boolean`, `number`, `string`, and `bigint`, they stay as their primitive type.
    /// For `unknown`, TypeScript does NOT narrow in falsy branches.
    ///
    /// Only literal types are narrowed (e.g., `0 | 1` -> `0`, `true | false` -> `false`).
    /// Narrows a type by nullishness (like `if (x != null)` or `if (x == null)`).
    /// If `nullish` is true, returns the nullish part (null | undefined).
    /// If `nullish` is false, returns the non-nullish part.
    pub fn narrow_by_nullishness(&self, source_type: TypeId, nullish: bool) -> TypeId {
        if source_type == TypeId::ANY {
            return source_type;
        }

        if source_type == TypeId::UNKNOWN {
            if nullish {
                return self.db.union(vec![TypeId::NULL, TypeId::UNDEFINED]);
            } else {
                let narrowed = self.narrow_excluding_type(source_type, TypeId::NULL);
                return self.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
            }
        }

        let (non_nullish, null_part) =
            crate::narrowing_utils::split_nullish_type(self.db, source_type);
        if nullish {
            null_part.unwrap_or(TypeId::NEVER)
        } else {
            non_nullish.unwrap_or(TypeId::NEVER)
        }
    }

    pub fn narrow_to_falsy(&self, type_id: TypeId) -> TypeId {
        let _span = span!(Level::TRACE, "narrow_to_falsy", type_id = type_id.0).entered();

        // Handle ANY - suppresses all narrowing
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }

        // Handle UNKNOWN - TypeScript does NOT narrow unknown in falsy branches
        if type_id == TypeId::UNKNOWN {
            return TypeId::UNKNOWN;
        }

        let resolved = self.resolve_type(type_id);

        // Handle Unions - recursively narrow each member and collect falsy components
        if let UnionMembersKind::Union(members) = classify_for_union_members(self.db, resolved) {
            let falsy_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.narrow_to_falsy(m))
                .filter(|&m| m != TypeId::NEVER)
                .collect();

            return if falsy_members.is_empty() {
                TypeId::NEVER
            } else if falsy_members.len() == 1 {
                falsy_members[0]
            } else {
                self.db.union(falsy_members)
            };
        }

        // Handle primitive types
        // CRITICAL: TypeScript has different behavior for different primitives

        // boolean is special: it's effectively true | false, so it narrows to false
        if resolved == TypeId::BOOLEAN {
            return TypeId::BOOLEAN_FALSE;
        }

        // TypeScript does NOT narrow these primitives in falsy branches
        if matches!(resolved, TypeId::STRING | TypeId::NUMBER | TypeId::BIGINT) {
            return resolved;
        }

        // null, undefined, void are always falsy
        if matches!(resolved, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID) {
            return resolved;
        }

        // Handle literals - check if they're falsy
        // This correctly handles `0` vs `1`, `""` vs `"a"`, `NaN` vs other numbers,
        // `true` vs `false`, etc.
        if let Some(_lit) = literal_value(self.db, resolved)
            && self.is_definitely_falsy(resolved)
        {
            return type_id;
        }

        TypeId::NEVER
    }

    /// This matches TypeScript's behavior where `if (x)` narrows out:
    /// - null, undefined, void
    /// - false (boolean literal)
    /// - 0, -0, `NaN` (number literals)
    /// - "" (empty string)
    /// - 0n (bigint literal)
    pub fn narrow_by_truthiness(&self, source_type: TypeId) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_truthiness",
            source_type = source_type.0
        )
        .entered();

        // Handle special cases
        if source_type == TypeId::ANY {
            return source_type;
        }

        // CRITICAL FIX: unknown in truthy branch narrows to exclude null/undefined
        // TypeScript: if (x: unknown) { x } -> x is not null | undefined
        if source_type == TypeId::UNKNOWN {
            let narrowed = self.narrow_excluding_type(source_type, TypeId::NULL);
            return self.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        let resolved = self.resolve_type(source_type);

        // Handle Intersections (recursive)
        // CRITICAL: If ANY part of intersection is falsy, the WHOLE intersection is falsy
        if let Some(members_id) = intersection_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let mut narrowed_members = Vec::with_capacity(members.len());

            for &m in members.iter() {
                let narrowed = self.narrow_by_truthiness(m);
                // If any part is NEVER, the whole intersection is impossible
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                narrowed_members.push(narrowed);
            }

            if narrowed_members.len() == 1 {
                return narrowed_members[0];
            }
            return self.db.intersection(narrowed_members);
        }

        // Handle Unions (filter out falsy members)
        if let Some(members_id) = union_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let narrowed = self.narrow_by_truthiness(m);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        // Base Case: Check if definitely falsy
        if self.is_definitely_falsy(source_type) {
            return TypeId::NEVER;
        }

        // Handle boolean -> true (TypeScript narrows boolean in truthy checks)
        if resolved == TypeId::BOOLEAN {
            return TypeId::BOOLEAN_TRUE;
        }

        // Handle Type Parameters (check constraint)
        if let Some(info) = type_param_info(self.db, resolved)
            && let Some(constraint) = info.constraint
        {
            let narrowed_constraint = self.narrow_by_truthiness(constraint);
            if narrowed_constraint == TypeId::NEVER {
                return TypeId::NEVER;
            }
            // If constraint narrowed, intersect source with it
            if narrowed_constraint != constraint {
                return self.db.intersection2(source_type, narrowed_constraint);
            }
        }

        source_type
    }

    /// Narrows a type by another type using the Visitor pattern.
    ///
    /// This is the general-purpose narrowing function that implements the
    /// Solver-First architecture (North Star Section 3.1). The Checker
    /// identifies WHERE narrowing happens (AST nodes) and the Solver
    /// calculates the RESULT.
    ///
    /// # Arguments
    /// * `type_id` - The type to narrow (e.g., a union type)
    /// * `narrower` - The type to narrow by (e.g., a literal type)
    ///
    /// # Returns
    /// The narrowed type. For unions, filters to members assignable to narrower.
    /// For type parameters, intersects with narrower.
    ///
    /// # Examples
    /// - `narrow("A" | "B", "A")` → `"A"`
    /// - `narrow(string | number, "hello")` → `"hello"`
    /// - `narrow(T | null, undefined)` → `null` (filters out T)
    pub fn narrow(&self, type_id: TypeId, narrower: TypeId) -> TypeId {
        // Fast path: already a subtype
        if is_subtype_of(self.db, type_id, narrower) {
            return type_id;
        }

        // Use visitor to perform narrowing
        let mut visitor = NarrowingVisitor {
            db: self.db,
            narrower,
            checker: SubtypeChecker::new(self.db.as_type_database()),
        };
        visitor.visit_type(self.db, type_id)
    }

    /// Task 10: Narrow a type to only array-like types.
    ///
    /// Used for `Array.isArray(x)` in the true branch.
    /// Keeps only arrays, tuples, and readonly arrays - preserves element types.
    ///
    /// # Examples
    /// - `narrow_to_array(string[] | number)` → `string[]`
    /// - `narrow_to_array(unknown)` → `any[]`
    /// - `narrow_to_array(any)` → `any`
    /// - `narrow_to_array(readonly [number, string])` → `readonly [number, string]`
    pub(crate) fn narrow_to_array(&self, source_type: TypeId) -> TypeId {
        // Handle ANY and UNKNOWN first
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        if source_type == TypeId::UNKNOWN {
            // Unknown narrows to any[] (most general array type)
            return self.db.array(TypeId::ANY);
        }

        // Handle Union: filter members, keeping only array-like types
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let array_like: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let narrowed = self.narrow_to_array(member);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if array_like.is_empty() {
                return TypeId::NEVER;
            } else if array_like.len() == 1 {
                return array_like[0];
            }
            return self.db.union(array_like);
        }

        // Handle Intersections: if ANY member is array-like, the whole intersection is array-like
        // e.g., string[] & { foo: string } is an array-like type
        if let Some(members_id) = intersection_list_id(self.db, source_type) {
            let members = self.db.type_list(members_id);
            let is_array = members.iter().any(|&m| {
                let resolved = self.resolve_type(m);
                self.is_array_like(resolved) || self.narrow_to_array(resolved) != TypeId::NEVER
            });

            if is_array {
                return source_type;
            }
        }

        // Handle Type Parameters: intersect with any[]
        if let Some(_info) = type_param_info(self.db, source_type) {
            let any_array = self.db.array(TypeId::ANY);
            return self.db.intersection2(source_type, any_array);
        }

        // Check if type is array-like (Array, Tuple, or ReadonlyArray)
        if self.is_array_like(source_type) {
            return source_type;
        }

        // Not array-like
        TypeId::NEVER
    }

    /// Task 10: Exclude array-like types from a type.
    ///
    /// Used for `!Array.isArray(x)` in the false branch.
    /// Removes arrays, tuples, and readonly arrays.
    ///
    /// # Examples
    /// - `narrow_excluding_array(string[] | number)` → `number`
    /// - `narrow_excluding_array(string[])` → `NEVER`
    /// - `narrow_excluding_array(unknown)` → `unknown`
    pub(crate) fn narrow_excluding_array(&self, source_type: TypeId) -> TypeId {
        // Handle ANY and UNKNOWN
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        if source_type == TypeId::UNKNOWN {
            // Unknown doesn't have a "not array" type representation
            return TypeId::UNKNOWN;
        }

        // Handle Union: filter out array-like members
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let non_array: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let narrowed = self.narrow_excluding_array(member);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if non_array.is_empty() {
                return TypeId::NEVER;
            } else if non_array.len() == 1 {
                return non_array[0];
            }
            return self.db.union(non_array);
        }

        // Handle Type Parameters: check if constraint is definitely an array
        // e.g., if T extends string[] and we check !Array.isArray(x), then x is never
        if let Some(info) = type_param_info(self.db, source_type)
            && let Some(constraint) = info.constraint
        {
            // If the constraint is definitely an array, then T is definitely an array.
            // So !Array.isArray(T) is NEVER.
            let narrowed_constraint = self.narrow_excluding_array(constraint);
            if narrowed_constraint == TypeId::NEVER {
                return TypeId::NEVER;
            }
        }

        // If array-like, return NEVER (excluded)
        if self.is_array_like(source_type) {
            return TypeId::NEVER;
        }

        // Not array-like, keep as-is
        source_type
    }

    /// Check if a type is array-like (Array, Tuple, or `ReadonlyArray`).
    ///
    /// This unwraps `ReadonlyType` recursively to check the underlying type.
    pub(crate) fn is_array_like(&self, type_id: TypeId) -> bool {
        use crate::type_queries;

        // Check for ReadonlyType wrapper (unwrap recursively)
        if let Some(TypeData::ReadonlyType(inner)) = self.db.lookup(type_id) {
            return self.is_array_like(inner);
        }

        // Check if type is Array, Tuple, or ReadonlyArray (wrapped)
        type_queries::is_array_type(self.db, type_id)
            || type_queries::is_tuple_type(self.db, type_id)
    }
}
