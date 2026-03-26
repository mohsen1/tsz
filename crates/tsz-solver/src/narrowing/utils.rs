//! Narrowing visitor and utility functions.
//!
//! Contains the `NarrowingVisitor` (`TypeVisitor` implementation for structural narrowing)
//! and standalone public utility functions for nullish/falsy type handling.

use super::{DiscriminantInfo, NarrowingContext};
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, TypeData, TypeId, TypeListId, TypeParamInfo};
use crate::visitor::{TypeVisitor, is_object_like_type_through_type_constraints};
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
                    if is_object_like_type_through_type_constraints(self.db, self.narrower) {
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

#[inline]
fn top_level_union_members(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<[TypeId]>> {
    // Inline the union check directly instead of going through the visitor pattern.
    // This avoids creating a TypeDataDataVisitor and virtual dispatch on the hot path.
    match types.lookup(type_id) {
        Some(TypeData::Union(list_id)) => Some(types.type_list(list_id)),
        _ => None,
    }
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
#[inline]
pub fn is_nullish_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_nullable() {
        return true;
    }
    // Fast path: non-nullable intrinsics can't be nullish
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members.iter().any(|&member| is_nullish_type(types, member));
    }
    false
}

/// Check if a type contains undefined (or void).
#[inline]
pub fn type_contains_undefined(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_undefined_intrinsic(type_id) {
        return true;
    }
    // Fast path: intrinsic types that aren't undefined/void don't contain undefined
    if type_id.is_intrinsic() {
        return false;
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
    if type_id.is_nullable() {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members
            .iter()
            .all(|&member| is_definitely_nullish(types, member));
    }
    false
}

fn split_nullish_members(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    non_nullish: &mut smallvec::SmallVec<[TypeId; 4]>,
    nullish: &mut smallvec::SmallVec<[TypeId; 2]>,
) {
    if type_id.is_nullable() {
        nullish.push(normalize_nullish(type_id));
        return;
    }

    if let Some(members) = top_level_union_members(types, type_id) {
        for &member in members.iter() {
            split_nullish_members(types, member, non_nullish, nullish);
        }
        return;
    }

    // Handle deferred conditional types where both branches are nullish/never.
    // For example: `string extends T ? undefined : never` is deferred as a Conditional,
    // but both branches (undefined, never) are nullish, so the whole type is nullish.
    if let Some(TypeData::Conditional(cond_id)) = types.lookup(type_id) {
        let cond = types.conditional_type(cond_id);
        let true_nullish = cond.true_type == TypeId::NEVER || cond.true_type.is_nullable();
        let false_nullish = cond.false_type == TypeId::NEVER || cond.false_type.is_nullable();
        if true_nullish && false_nullish {
            // Both branches are nullish/never — classify the whole conditional as nullish.
            // The actual nullish type is the union of non-never branches.
            if cond.true_type != TypeId::NEVER && cond.true_type.is_nullable() {
                nullish.push(normalize_nullish(cond.true_type));
            }
            if cond.false_type != TypeId::NEVER && cond.false_type.is_nullable() {
                nullish.push(normalize_nullish(cond.false_type));
            }
            if nullish.is_empty() {
                // Both branches are never — effectively never, not nullish
                non_nullish.push(type_id);
            }
            return;
        }
    }

    non_nullish.push(type_id);
}

/// Split a type into its non-nullish part and its nullish cause.
#[inline]
pub fn split_nullish_type(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<TypeId>, Option<TypeId>) {
    if type_id.is_nullable() {
        return (None, Some(normalize_nullish(type_id)));
    }

    // Type parameters and `infer` variables can be nullable through their
    // constraints even when the type itself is not a top-level `null | undefined`
    // union. Preserve that for downstream diagnostics like TS18048 and carry a
    // non-nullish view forward for the actual access.
    if let Some(TypeData::TypeParameter(param) | TypeData::Infer(param)) = types.lookup(type_id)
        && let Some(constraint) = param.constraint
    {
        let (_, nullish_cause) = split_nullish_type(types, constraint);
        if let Some(cause) = nullish_cause {
            return (Some(remove_nullish(types, type_id)), Some(cause));
        }
    }

    // Fast path: non-union, non-nullable types are entirely non-nullish
    if type_id.is_intrinsic() || top_level_union_members(types, type_id).is_none() {
        return (Some(type_id), None);
    }

    // Use SmallVec to avoid heap allocation for common cases (e.g., T | undefined)
    let mut non_nullish: smallvec::SmallVec<[TypeId; 4]> = smallvec::SmallVec::new();
    let mut nullish: smallvec::SmallVec<[TypeId; 2]> = smallvec::SmallVec::new();

    split_nullish_members(types, type_id, &mut non_nullish, &mut nullish);

    if nullish.is_empty() {
        return (Some(type_id), None);
    }

    let non_nullish_type = if non_nullish.is_empty() {
        None
    } else if non_nullish.len() == 1 {
        Some(non_nullish[0])
    } else {
        Some(types.union(non_nullish.to_vec()))
    };

    let nullish_type = if nullish.len() == 1 {
        Some(nullish[0])
    } else {
        Some(types.union(nullish.to_vec()))
    };

    (non_nullish_type, nullish_type)
}

/// Remove nullish parts of a type (non-null assertion).
///
/// For type parameters whose constraint includes nullable types (e.g., `T extends string | undefined`),
/// returns `T & {}` (`NonNullable`<T>) instead of `T`. This matches tsc's behavior where
/// `x!` on a type parameter produces `T & {}`, ensuring proper assignability to the
/// non-nullable part of the constraint.
fn remove_nullish_inner(
    types: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    type_id: TypeId,
) -> TypeId {
    if let Some(db) = query_db
        && let Some(TypeData::Application(app_id)) = types.lookup(type_id)
    {
        let app = types.type_application(app_id);
        let def_id = match types.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::TypeQuery(sym_ref)) => db.symbol_to_def_id(sym_ref),
            _ => None,
        };
        if let Some(def_id) = def_id
            && let Some(type_params) = db.get_lazy_type_params(def_id)
            && let Some(resolved) = db.resolve_lazy(def_id, types)
        {
            let instantiated = crate::instantiate_generic(types, resolved, &type_params, &app.args);
            if std::env::var_os("TSZ_DEBUG_REMOVE_NULLISH").is_some() {
                eprintln!(
                    "remove-nullish-app type={:?} base={:?} resolved={:?} instantiated={:?}",
                    type_id, app.base, resolved, instantiated
                );
            }
            if instantiated != type_id {
                return remove_nullish_inner(types, query_db, instantiated);
            }
        }
    }

    if matches!(
        types.lookup(type_id),
        Some(TypeData::Application(_) | TypeData::Lazy(_) | TypeData::TypeQuery(_))
    ) {
        let evaluated = query_db
            .map(|db| db.evaluate_type(type_id))
            .unwrap_or_else(|| crate::evaluation::evaluate::evaluate_type(types, type_id));
        if evaluated != type_id {
            return remove_nullish_inner(types, query_db, evaluated);
        }
    }

    // Deferred conditional types need branch-aware nullish removal. This keeps
    // generic relationships intact for patterns like:
    //   Extract<T, string | undefined>
    // where truthiness should narrow to `Extract<T, string>` rather than leave
    // the deferred conditional untouched.
    if let Some(TypeData::Conditional(cond_id)) = types.lookup(type_id) {
        let cond = types.conditional_type(cond_id);
        if std::env::var_os("TSZ_DEBUG_REMOVE_NULLISH").is_some() {
            eprintln!(
                "remove-nullish-cond type={:?} check={:?} extends={:?} true={:?} false={:?} true_eq_check={}",
                type_id,
                cond.check_type,
                cond.extends_type,
                cond.true_type,
                cond.false_type,
                cond.true_type == cond.check_type
            );
        }

        // Common Extract-like shape: `T extends U ? T : never`.
        // Truthiness / non-null removal should narrow the true branch through the
        // constraint, yielding `T & NonNullable<U>`.
        if cond.true_type == cond.check_type {
            let non_null_extends = remove_nullish_inner(types, query_db, cond.extends_type);
            let true_branch = types.intersection2(cond.check_type, non_null_extends);
            let false_branch = remove_nullish_inner(types, query_db, cond.false_type);
            let result = if false_branch == TypeId::NEVER {
                true_branch
            } else {
                types.union2(true_branch, false_branch)
            };
            if std::env::var_os("TSZ_DEBUG_REMOVE_NULLISH").is_some() {
                eprintln!(
                    "remove-nullish-cond-result type={:?} non_null_extends={:?} true_branch={:?} false_branch={:?} result={:?}",
                    type_id, non_null_extends, true_branch, false_branch, result
                );
            }
            return result;
        }

        let true_branch = remove_nullish_inner(types, query_db, cond.true_type);
        let false_branch = remove_nullish_inner(types, query_db, cond.false_type);
        return match (true_branch, false_branch) {
            (TypeId::NEVER, TypeId::NEVER) => TypeId::NEVER,
            (TypeId::NEVER, other) | (other, TypeId::NEVER) => other,
            (left, right) => types.union2(left, right),
        };
    }

    // Handle type parameters: if the constraint includes nullable types,
    // produce T & {} (NonNullable<T>) to properly narrow the type.
    if let Some(TypeData::TypeParameter(param)) = types.lookup(type_id) {
        if let Some(constraint) = param.constraint {
            let constraint_has_nullable = constraint.is_nullable()
                || top_level_union_members(types, constraint)
                    .is_some_and(|members| members.iter().any(|m| m.is_nullable()));
            if constraint_has_nullable {
                let empty_obj = types.object(vec![]);
                return types.intersection2(type_id, empty_obj);
            }
        }
        return type_id;
    }

    // For unions containing type parameters with nullable constraints,
    // apply NonNullable to each member individually:
    // NonNullable<T | U> = NonNullable<T> | NonNullable<U>
    if let Some(members) = top_level_union_members(types, type_id) {
        let has_nullable_type_param = members.iter().any(|&m| {
            if let Some(TypeData::TypeParameter(p)) = types.lookup(m) {
                p.constraint.is_some_and(|c| {
                    c.is_nullable()
                        || top_level_union_members(types, c)
                            .is_some_and(|cms| cms.iter().any(|cm| cm.is_nullable()))
                })
            } else {
                false
            }
        });
        if has_nullable_type_param {
            let processed: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let narrowed = remove_nullish_inner(types, query_db, m);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();
            return match processed.len() {
                0 => TypeId::NEVER,
                1 => processed[0],
                _ => types.union(processed),
            };
        }
    }

    let (non_nullish, _) = split_nullish_type(types, type_id);
    non_nullish.unwrap_or(TypeId::NEVER)
}

pub fn remove_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    remove_nullish_inner(types, None, type_id)
}

pub fn remove_nullish_query(types: &dyn QueryDatabase, type_id: TypeId) -> TypeId {
    remove_nullish_inner(types, Some(types), type_id)
}

/// Remove `undefined` from a type while preserving `null` and other members.
///
/// Used by JSX attribute checking: when a property is optional (`prop?: T`),
/// `optional_property_type` adds `undefined` to produce `T | undefined` (the read type).
/// For write positions (providing an attribute value), `undefined` should be stripped
/// to match TSC's `removeMissingType` behavior.
pub fn remove_undefined(types: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id == TypeId::UNDEFINED {
        return TypeId::NEVER;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        let filtered: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| m != TypeId::UNDEFINED)
            .collect();
        if filtered.len() == members.len() {
            return type_id; // no undefined to remove
        }
        match filtered.len() {
            0 => TypeId::NEVER,
            1 => filtered[0],
            _ => types.union(filtered),
        }
    } else {
        type_id
    }
}
