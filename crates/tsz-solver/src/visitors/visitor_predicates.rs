//! Type Predicate Functions
//!
//! This module provides convenience functions for checking type classifications
//! and querying whether types contain specific nested type kinds. These are
//! extracted from the main visitor module for maintainability.
//!
//! # Categories
//!
//! - **Simple predicates** (`is_*`): Check if a type matches a specific `TypeData` variant.
//! - **Deep predicates** (`contains_*`): Recursively check if a type contains specific nested types.
//! - **Database wrappers** (`*_db`): Variants that unwrap through `ReadonlyType`/`NoInfer`/constraints.
//! - **Object classification**: `ObjectTypeKind` enum and `classify_object_type`.

use crate::types::{IntrinsicKind, ObjectShapeId};
use crate::{TypeData, TypeDatabase, TypeId};
use rustc_hash::FxHashMap;

// =============================================================================
// Specialized Type Predicate Visitors
// =============================================================================

/// Check if a type is a literal type.
///
/// Matches: `TypeData::Literal`(_)
pub fn is_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Literal(_)))
}

/// Check if a type is a module namespace type (import * as ns).
///
/// Matches: `TypeData::ModuleNamespace`(_)
pub fn is_module_namespace_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::ModuleNamespace(_)))
}

/// Check if a type is a function type (Function or Callable).
///
/// This also handles intersections containing function types.
pub fn is_function_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_function_type_impl(types, type_id)
}

fn is_function_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .any(|&member| is_function_type_impl(types, member))
        }
        _ => false,
    }
}

/// Check if a type is an object-like type (suitable for typeof "object").
///
/// Returns true for: Object, `ObjectWithIndex`, Array, Tuple, Mapped, `ReadonlyType` (of object)
pub fn is_object_like_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(types, type_id)
}

fn is_object_like_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Mapped(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Intrinsic(IntrinsicKind::Object | IntrinsicKind::Function),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => is_object_like_type_impl(types, inner),
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(types, member))
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| is_object_like_type_impl(types, constraint)),
        _ => false,
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
    }
}

/// Check if a type is a primitive type (intrinsic or literal).
pub fn is_primitive_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check well-known intrinsic TypeIds first
    if type_id.is_intrinsic() {
        return true;
    }
    matches!(
        types.lookup(type_id),
        Some(TypeData::Intrinsic(_) | TypeData::Literal(_))
    )
}

/// Check if a type is a union type.
pub fn is_union_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Union(_)))
}

/// Check if a type is an intersection type.
pub fn is_intersection_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Intersection(_)))
}

/// Check if a type is an array type.
pub fn is_array_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Array(_)))
}

/// Check if a type is a tuple type.
pub fn is_tuple_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Tuple(_)))
}

/// Check if a type is a type parameter.
pub fn is_type_parameter(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is a conditional type.
pub fn is_conditional_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Conditional(_)))
}

/// Check if a type is a mapped type.
pub fn is_mapped_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Mapped(_)))
}

/// Check if a type is an index access type.
pub fn is_index_access_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
}

/// Check if a type is a template literal type.
pub fn is_template_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::TemplateLiteral(_)))
}

/// Check if a type is a type reference (Lazy/DefId).
pub fn is_type_reference(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_))
    )
}

/// Check if a type is a generic type application.
pub fn is_generic_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type is a "unit type" - a type that represents exactly one value.
///
/// Unit types are types where subtyping reduces to identity: two different unit types
/// are always disjoint (neither is a subtype of the other, except for identity).
///
/// This is used as an optimization to skip structural recursion in subtype checking.
/// For example, comparing `[E.A, E.B]` vs `[E.C, E.D]` can return `source == target`
/// in O(1) instead of walking into each tuple element.
///
/// Unit types include:
/// - Literal types (string, number, boolean, bigint literals)
/// - Enum members (`TypeData::Enum`)
/// - Unique symbols
/// - null, undefined, void
/// - Tuples where ALL elements are unit types (and no rest elements)
///
/// NOTE: This does NOT handle `ReadonlyType` - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different `TypeIds`.
pub fn is_unit_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_unit_type_impl(types, type_id, 0)
}

const MAX_UNIT_TYPE_DEPTH: u32 = 10;

fn is_unit_type_impl(types: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    // Prevent stack overflow on pathological types
    if depth > MAX_UNIT_TYPE_DEPTH {
        return false;
    }

    // Check well-known singleton types first
    if type_id == TypeId::NULL
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
        || type_id == TypeId::NEVER
    {
        return true;
    }

    match types.lookup(type_id) {
        // Unit-like scalar types are handled together.
        Some(TypeData::Literal(_))
        | Some(TypeData::Enum(_, _))
        | Some(TypeData::UniqueSymbol(_)) => true,

        // Tuples are unit types if ALL elements are unit types (no rest elements)
        Some(TypeData::Tuple(list_id)) => {
            let elements = types.tuple_list(list_id);
            // Check for rest elements - if any, not a unit type
            if elements.iter().any(|e| e.rest) {
                return false;
            }
            // All elements must be unit types
            elements
                .iter()
                .all(|e| is_unit_type_impl(types, e.type_id, depth + 1))
        }

        // Everything else is not a unit type
        // ReadonlyType of a unit tuple is NOT considered a unit type for optimization purposes
        // because ["a"] <: readonly ["a"] but they have different TypeIds.
        _ => false,
    }
}

// =============================================================================
// Type Contains Visitor - Check if a type contains specific types
// =============================================================================

/// Check if a type contains any type parameters.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    })
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains the error type.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Error))
}

/// Check if a type contains the `this` type anywhere.
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::ThisType))
}

/// Check if a type contains any type matching a predicate.
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut checker = ContainsTypeChecker {
        types,
        predicate,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    types: &'a dyn TypeDatabase,
    predicate: F,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let Some(key) = self.types.lookup(type_id) else {
            self.guard.leave(type_id);
            return false;
        };

        if (self.predicate)(&key) {
            self.guard.leave(type_id);
            self.memo.insert(type_id, true);
            return true;
        }

        let result = self.check_key(&key);

        self.guard.leave(type_id);
        self.memo.insert(type_id, result);

        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                self.check(app.base) || app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.conditional_type(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.mapped_type(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

// =============================================================================
// TypeDatabase-based convenience functions
// =============================================================================

/// Check if a type is a literal type (`TypeDatabase` version).
pub fn is_literal_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a module namespace type (`TypeDatabase` version).
pub fn is_module_namespace_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(types.lookup(type_id), Some(TypeData::ModuleNamespace(_)))
}

/// Check if a type is a function type (`TypeDatabase` version).
pub fn is_function_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (`TypeDatabase` version).
pub fn is_object_like_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (`TypeDatabase` version).
pub fn is_empty_object_type_db(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let checker = EmptyObjectChecker::new(types);
    checker.check(type_id)
}

// =============================================================================
// Object Type Classification
// =============================================================================

/// Classification of object types for freshness tracking.
pub enum ObjectTypeKind {
    /// A regular object type (no index signatures).
    Object(ObjectShapeId),
    /// An object type with index signatures.
    ObjectWithIndex(ObjectShapeId),
    /// Not an object type.
    NotObject,
}

/// Classify a type as an object type kind.
///
/// This is used by the freshness tracking system to determine if a type
/// is a fresh object literal that needs special handling.
pub fn classify_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> ObjectTypeKind {
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
        _ => ObjectTypeKind::NotObject,
    }
}

// =============================================================================
// Visitor Pattern Implementations for Helper Functions
// =============================================================================

/// Visitor to check if a type is a literal type.
struct LiteralTypeChecker;

impl LiteralTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        match types.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Mapped(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| Self::check(types, constraint)),
            _ => false,
        }
    }
}

/// Visitor to check if a type is an empty object type.
struct EmptyObjectChecker<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> EmptyObjectChecker<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn check(&self, type_id: TypeId) -> bool {
        match self.db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => self.check(inner),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| self.check(c))
            }
            _ => false,
        }
    }
}
