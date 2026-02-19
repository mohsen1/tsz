//! Narrowing visitor and utility functions.
//!
//! Contains the `NarrowingVisitor` (`TypeVisitor` implementation for structural narrowing)
//! and standalone public utility functions for nullish/falsy type handling.

use crate::narrowing::{DiscriminantInfo, NarrowingContext};
use crate::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, TypeData, TypeId, TypeListId, TypeParamInfo};
use crate::visitor::{TypeVisitor, is_object_like_type_db, literal_value, union_list_id};
use crate::{QueryDatabase, TypeDatabase};
use tsz_common::interner::Atom;

/// Visitor that narrows a type by filtering/intersecting with a narrower type.
pub(crate) struct NarrowingVisitor<'a> {
    pub(crate) db: &'a dyn QueryDatabase,
    pub(crate) narrower: TypeId,
    /// PERF: Reusable `SubtypeChecker` to avoid per-call hash allocations
    pub(crate) checker: SubtypeChecker<'a>,
}

impl<'a> TypeVisitor for NarrowingVisitor<'a> {
    type Output = TypeId;

    /// Override `visit_type` to handle types that need special handling.
    /// We intercept Lazy/Ref/Application types for resolution, and Object/Function
    /// types for proper subtype checking (we need the `TypeId` here).
    fn visit_type(&mut self, types: &dyn TypeDatabase, type_id: TypeId) -> Self::Output {
        // Check if this is a type that needs special handling
        if let Some(type_key) = types.lookup(type_id) {
            match type_key {
                // Lazy types: resolve and recurse
                TypeData::Lazy(_) => {
                    // Use self.db (QueryDatabase) which has evaluate_type
                    let resolved = self.db.evaluate_type(type_id);
                    // If resolution changed the type, recurse with the resolved type
                    if resolved != type_id {
                        return self.visit_type(types, resolved);
                    }
                    // Otherwise, fall through to normal visitation
                }
                // Ref types: resolve and recurse
                TypeData::TypeQuery(_) | TypeData::Application(_) => {
                    let resolved = self.db.evaluate_type(type_id);
                    if resolved != type_id {
                        return self.visit_type(types, resolved);
                    }
                }
                // Object types: check subtype relationships
                TypeData::Object(_) => {
                    // Case 1: type_id is subtype of narrower (e.g., { a: "foo" } narrowed by { a: string })
                    // Result: type_id (keep the more specific type)
                    self.checker.reset();
                    if self.checker.is_subtype_of(type_id, self.narrower) {
                        return type_id;
                    }
                    // Case 2: narrower is subtype of type_id (e.g., { a: string } narrowed by { a: "foo" })
                    // Result: narrower (narrow down to the more specific type)
                    self.checker.reset();
                    if self.checker.is_subtype_of(self.narrower, type_id) {
                        return self.narrower;
                    }
                    // Case 3: Both are object types but not directly related
                    // They might overlap (e.g., interfaces with common properties)
                    // For now, conservatively return the intersection
                    if is_object_like_type_db(self.db, self.narrower) {
                        return self.db.intersection2(type_id, self.narrower);
                    }
                    // Case 4: Disjoint object types
                    return TypeId::NEVER;
                }
                // Function types: check subtype relationships
                TypeData::Function(_) => {
                    // Case 1: type_id is subtype of narrower (keep specific)
                    self.checker.reset();
                    if self.checker.is_subtype_of(type_id, self.narrower) {
                        return type_id;
                    }
                    // Case 2: narrower is subtype of type_id (narrow down)
                    self.checker.reset();
                    if self.checker.is_subtype_of(self.narrower, type_id) {
                        return self.narrower;
                    }
                    // Case 3: Disjoint function types
                    return TypeId::NEVER;
                }
                _ => {}
            }
        }

        // For all other types, use the default visit_type implementation
        // which calls visit_type_key and dispatches to specific methods
        <Self as TypeVisitor>::visit_type(self, types, type_id)
    }

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            IntrinsicKind::Any => {
                // Narrowing `any` by anything returns that type
                self.narrower
            }
            IntrinsicKind::Unknown => {
                // Narrowing `unknown` by anything returns that type
                self.narrower
            }
            IntrinsicKind::Never => {
                // Never stays never
                TypeId::NEVER
            }
            _ => {
                // For other intrinsics, we need to handle the overlap case
                // Narrowing primitive by primitive is effectively intersection
                let type_id = TypeId(kind as u32);

                // Case 1: narrower is subtype of type_id (e.g., narrow(string, "foo"))
                // Result: narrower
                self.checker.reset();
                if self.checker.is_subtype_of(self.narrower, type_id) {
                    self.narrower
                }
                // Case 2: type_id is subtype of narrower (e.g., narrow("foo", string))
                // Result: type_id (the original)
                else {
                    self.checker.reset();
                    if self.checker.is_subtype_of(type_id, self.narrower) {
                        type_id
                    }
                    // Case 3: Disjoint types (e.g., narrow(string, number))
                    // Result: never
                    else {
                        TypeId::NEVER
                    }
                }
            }
        }
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        // For literal types, check if assignable to narrower
        // The literal type_id will be constructed and checked
        // For now, return the narrower (will be refined with actual type_id)
        self.narrower
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));

        // CRITICAL: Recursively narrow each union member, don't just check subtype
        // This handles cases like: string narrowed by "foo" -> "foo"
        // where "foo" is NOT a subtype of string, but string contains "foo"
        let filtered: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let narrowed = self.visit_type(self.db, member);
                if narrowed == TypeId::NEVER {
                    None
                } else {
                    Some(narrowed)
                }
            })
            .collect();

        if filtered.is_empty() {
            TypeId::NEVER
        } else if filtered.len() == members.len() {
            // All members matched - reconstruct the union
            self.db.union(filtered)
        } else if filtered.len() == 1 {
            filtered[0]
        } else {
            self.db.union(filtered)
        }
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));

        // Narrow each intersection member individually and collect non-never results
        // For (A & B) narrowed by C, the result is (A narrowed by C) & (B narrowed by C)
        let narrowed_members: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let narrowed = self.visit_type(self.db, member);
                if narrowed == TypeId::NEVER {
                    None
                } else {
                    Some(narrowed)
                }
            })
            .collect();

        if narrowed_members.is_empty() {
            TypeId::NEVER
        } else if narrowed_members.len() == 1 {
            narrowed_members[0]
        } else {
            self.db.intersection(narrowed_members)
        }
    }

    fn visit_type_parameter(&mut self, info: &TypeParamInfo) -> Self::Output {
        // For type parameters, intersect with the narrower
        // This constrains the generic type variable
        if let Some(constraint) = info.constraint {
            self.db.intersection2(constraint, self.narrower)
        } else {
            // No constraint, so narrowing gives us the narrower
            self.narrower
        }
    }

    fn visit_lazy(&mut self, _def_id: u32) -> Self::Output {
        // Lazy types are now handled in visit_type by resolving and recursing
        // This should never be called anymore, but if it is, return narrower
        self.narrower
    }

    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
        // Ref types are now handled in visit_type by resolving and recursing
        // This should never be called anymore, but if it is, return narrower
        self.narrower
    }

    fn visit_application(&mut self, _app_id: u32) -> Self::Output {
        // Application types are now handled in visit_type by resolving and recursing
        // This should never be called anymore, but if it is, return narrower
        self.narrower
    }

    fn visit_object(&mut self, _shape_id: u32) -> Self::Output {
        // Object types are now handled in visit_type where we have the TypeId
        // For now, conservatively return the narrower
        self.narrower
    }

    fn visit_function(&mut self, _shape_id: u32) -> Self::Output {
        // Function types are now handled in visit_type where we have the TypeId
        // For now, conservatively return the narrower
        self.narrower
    }

    fn visit_callable(&mut self, _shape_id: u32) -> Self::Output {
        // For callable types, conservatively return the narrower
        self.narrower
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        // For tuple types, conservatively return the narrower
        self.narrower
    }

    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        // For array types, conservatively return the narrower
        self.narrower
    }

    fn default_output() -> Self::Output {
        // Fallback for types not explicitly handled above
        // Conservative: return never (type doesn't match the narrower)
        // This is safe because:
        // - For unions, this member will be excluded from the filtered result
        // - For other contexts, never means "no match"
        TypeId::NEVER
    }
}

/// Convenience function for finding discriminants.
pub fn find_discriminants(
    interner: &dyn QueryDatabase,
    union_type: TypeId,
) -> Vec<DiscriminantInfo> {
    let ctx = NarrowingContext::new(interner);
    ctx.find_discriminants(union_type)
}

/// Convenience function for narrowing by discriminant.
pub fn narrow_by_discriminant(
    interner: &dyn QueryDatabase,
    union_type: TypeId,
    property_path: &[Atom],
    literal_value: TypeId,
) -> TypeId {
    let ctx = NarrowingContext::new(interner);
    ctx.narrow_by_discriminant(union_type, property_path, literal_value)
}

/// Convenience function for typeof narrowing.
pub fn narrow_by_typeof(
    interner: &dyn QueryDatabase,
    source_type: TypeId,
    typeof_result: &str,
) -> TypeId {
    let ctx = NarrowingContext::new(interner);
    ctx.narrow_by_typeof(source_type, typeof_result)
}

// =============================================================================
// Nullish Type Helpers
// =============================================================================

fn top_level_union_members(types: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    union_list_id(types, type_id).map(|list_id| types.type_list(list_id).to_vec())
}

const fn is_nullish_intrinsic(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
}

const fn is_undefined_intrinsic(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::UNDEFINED | TypeId::VOID)
}

fn normalize_nullish(type_id: TypeId) -> TypeId {
    if type_id == TypeId::VOID {
        TypeId::UNDEFINED
    } else {
        type_id
    }
}

/// Check if a type is nullish (null/undefined/void or union containing them).
pub fn is_nullish_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_nullish_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members.iter().any(|&member| is_nullish_type(types, member));
    }
    false
}

/// Check if a type (possibly a union) contains null or undefined.
pub fn type_contains_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_nullish_type(types, type_id)
}

/// Check if a type contains undefined (or void).
pub fn type_contains_undefined(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_undefined_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members
            .iter()
            .any(|&member| type_contains_undefined(types, member));
    }
    false
}

/// Check if a type is definitely nullish (only null/undefined/void).
pub fn is_definitely_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_nullish_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members
            .iter()
            .all(|&member| is_definitely_nullish(types, member));
    }
    false
}

/// Check if a type can be nullish (contains null/undefined/void).
pub fn can_be_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_nullish_type(types, type_id)
}

fn split_nullish_members(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    non_nullish: &mut Vec<TypeId>,
    nullish: &mut Vec<TypeId>,
) {
    if is_nullish_intrinsic(type_id) {
        nullish.push(normalize_nullish(type_id));
        return;
    }

    if let Some(members) = top_level_union_members(types, type_id) {
        for member in members {
            split_nullish_members(types, member, non_nullish, nullish);
        }
        return;
    }

    non_nullish.push(type_id);
}

/// Split a type into its non-nullish part and its nullish cause.
pub fn split_nullish_type(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<TypeId>, Option<TypeId>) {
    let mut non_nullish = Vec::new();
    let mut nullish = Vec::new();

    split_nullish_members(types, type_id, &mut non_nullish, &mut nullish);

    if nullish.is_empty() {
        return (Some(type_id), None);
    }

    let non_nullish_type = if non_nullish.is_empty() {
        None
    } else if non_nullish.len() == 1 {
        Some(non_nullish[0])
    } else {
        Some(types.union(non_nullish))
    };

    let nullish_type = if nullish.len() == 1 {
        Some(nullish[0])
    } else {
        Some(types.union(nullish))
    };

    (non_nullish_type, nullish_type)
}

/// Remove nullish parts of a type (non-null assertion).
pub fn remove_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let (non_nullish, _) = split_nullish_type(types, type_id);
    non_nullish.unwrap_or(TypeId::NEVER)
}

/// Remove types that are *definitely* falsy from a union, without narrowing
/// non-falsy types. This matches TypeScript's `removeDefinitelyFalsyTypes`:
/// removes `null`, `undefined`, `void`, `false`, `0`, `""`, `0n` but keeps
/// `boolean`, `string`, `number`, `bigint`, and object types unchanged.
pub fn remove_definitely_falsy_types(types: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if is_always_falsy(types, type_id) {
        return TypeId::NEVER;
    }
    if let Some(members_id) = union_list_id(types, type_id) {
        let members = types.type_list(members_id);
        let remaining: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| !is_always_falsy(types, m))
            .collect();
        if remaining.is_empty() {
            return TypeId::NEVER;
        }
        if remaining.len() == 1 {
            return remaining[0];
        }
        if remaining.len() == members.len() {
            return type_id;
        }
        return types.union(remaining);
    }
    type_id
}

/// Check if a type is always falsy (null, undefined, void, false, 0, "", 0n).
fn is_always_falsy(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if matches!(type_id, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID) {
        return true;
    }
    if let Some(lit) = literal_value(types, type_id) {
        return match lit {
            LiteralValue::Boolean(false) => true,
            LiteralValue::Number(n) => n.0 == 0.0 || n.0.is_nan(),
            LiteralValue::String(atom) => types.resolve_atom_ref(atom).is_empty(),
            LiteralValue::BigInt(atom) => types.resolve_atom_ref(atom).as_ref() == "0",
            _ => false,
        };
    }
    false
}
