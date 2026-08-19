//! Additional type query classifiers.
//!
//! Contains classification enums and functions for specific checker scenarios:
//! - Excess property checking
//! - Constructor access levels
//! - Assignability evaluation
//! - Binding element type extraction
//! - Type identity/accessor helpers
//! - Symbol resolution traversal
//! - Interface merge type classification
//! - Augmentation target classification

use crate::construction::TypeDatabase;
use crate::{TypeData, TypeId};

// =============================================================================
// Excess Properties Classification
// =============================================================================

/// Classification for checking excess properties.
#[derive(Debug, Clone)]
pub enum ExcessPropertiesKind {
    /// Object type (without index signature) - check for excess
    Object(crate::types::ObjectShapeId),
    /// Object with index signature - accepts any property
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// Union - check all members
    Union(Vec<TypeId>),
    /// Intersection - merge known members from all object constituents
    Intersection(Vec<TypeId>),
    /// Not an object type
    NotObject,
}

/// Classify a type for excess property checking.
pub fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    if type_id.is_intrinsic() {
        return ExcessPropertiesKind::NotObject;
    }
    let Some(key) = db.lookup(type_id) else {
        return ExcessPropertiesKind::NotObject;
    };

    match key {
        TypeData::Object(shape_id) => ExcessPropertiesKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => ExcessPropertiesKind::ObjectWithIndex(shape_id),
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            ExcessPropertiesKind::Union(members.to_vec())
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            ExcessPropertiesKind::Intersection(members.to_vec())
        }
        _ => ExcessPropertiesKind::NotObject,
    }
}

// =============================================================================
// Constructor Access Level Classification
// =============================================================================

/// Classification for checking constructor access level.
#[derive(Debug, Clone)]
pub enum ConstructorAccessKind {
    /// Ref or `TypeQuery` - resolve symbol
    SymbolRef(crate::types::SymbolRef),
    /// Application - check base
    Application(crate::types::TypeApplicationId),
    /// Not applicable
    Other,
}

/// Classify a type for constructor access level checking.
pub fn classify_for_constructor_access(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorAccessKind {
    if type_id.is_intrinsic() {
        return ConstructorAccessKind::Other;
    }
    let Some(key) = db.lookup(type_id) else {
        return ConstructorAccessKind::Other;
    };

    match key {
        TypeData::TypeQuery(sym_ref) => ConstructorAccessKind::SymbolRef(sym_ref),
        TypeData::Application(app_id) => ConstructorAccessKind::Application(app_id),
        _ => ConstructorAccessKind::Other,
    }
}

// =============================================================================
// Assignability Evaluation Classification
// =============================================================================

/// Classification for types that need evaluation before assignability.
#[derive(Debug, Clone)]
pub enum AssignabilityEvalKind {
    /// Application - evaluate with resolution
    Application,
    /// Index/KeyOf/Mapped/Conditional - evaluate with env
    NeedsEnvEval,
    /// Already resolved
    Resolved,
}

/// Classify a type for assignability evaluation.
pub fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    if type_id.is_intrinsic() {
        return AssignabilityEvalKind::Resolved;
    }
    let Some(key) = db.lookup(type_id) else {
        return AssignabilityEvalKind::Resolved;
    };

    match key {
        TypeData::Application(_) | TypeData::Lazy(_) => AssignabilityEvalKind::Application,
        TypeData::IndexAccess(object_type, _index_type) => {
            let object_is_deferred_type_param = if object_type.is_intrinsic() {
                false
            } else {
                match db.lookup(object_type) {
                    Some(TypeData::TypeParameter(info)) | Some(TypeData::Infer(info)) => {
                        info.constraint.is_none_or(|constraint| {
                            crate::type_queries::is_type_parameter_like(db, constraint)
                        })
                    }
                    Some(TypeData::ThisType) => true,
                    _ => false,
                }
            };

            if crate::type_queries::contains_type_parameters_db(db, type_id)
                && object_is_deferred_type_param
            {
                AssignabilityEvalKind::Resolved
            } else {
                AssignabilityEvalKind::NeedsEnvEval
            }
        }
        // For KeyOf, use contains_generic_type_parameters_db which excludes ThisType.
        // This ensures `keyof this` is evaluated (resolving `this` to the class type)
        // while `keyof T` (with generic T) remains deferred as Resolved.
        TypeData::KeyOf(_)
            if crate::type_queries::contains_generic_type_parameters_db(db, type_id) =>
        {
            AssignabilityEvalKind::Resolved
        }
        TypeData::KeyOf(_)
        | TypeData::Mapped(_)
        | TypeData::Conditional(_)
        | TypeData::StringIntrinsic { .. }
        | TypeData::TypeQuery(_) => AssignabilityEvalKind::NeedsEnvEval,
        _ => AssignabilityEvalKind::Resolved,
    }
}

// =============================================================================
// Additional Accessor Helpers
// =============================================================================

/// Get the `DefId` from a Lazy type.
pub fn get_lazy_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    // Fast path: intrinsics are never `Lazy(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
        _ => None,
    }
}

/// Get the `DefId` from a generic Application type whose base is `Lazy(def_id)`.
///
/// Returns `None` if the type is not an Application or if the base is not Lazy.
pub fn get_application_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::def::DefId> {
    // Fast path: intrinsics are never `Application(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    if let Some(TypeData::Application(app_id)) = db.lookup(type_id) {
        let app = db.type_application(app_id);
        if !app.base.is_intrinsic()
            && let Some(TypeData::Lazy(def_id)) = db.lookup(app.base)
        {
            return Some(def_id);
        }
    }
    None
}

/// Get the `SymbolRef` from a `TypeQuery` type (`typeof X`).
pub fn get_type_query_symbol_ref(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::SymbolRef> {
    // Fast path: intrinsics are never `TypeQuery(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeQuery(sym_ref)) => Some(sym_ref),
        _ => None,
    }
}

/// True when `type_id` is a surface type constructor over the canonical
/// polymorphic `this`, such as `this[]`, `readonly this[]`, `this | undefined`,
/// or `Foo & this`.
///
/// This deliberately walks only constructor surfaces. It does not inspect object
/// members, lazy class/interface bodies, or type-parameter constraints, because
/// those can mention `this` without making the receiver itself a `this`-relative
/// wrapper.
pub fn is_compound_this_relative_surface_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> bool {
    type_id != this_type && has_surface_this_relative_wrapper(db, type_id, this_type)
}

fn has_surface_this_relative_wrapper(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }

    let mut stack = vec![type_id];
    let mut fuel = 64usize;
    while let Some(current) = stack.pop() {
        if current == this_type {
            return true;
        }
        if current.is_intrinsic() || fuel == 0 {
            continue;
        }
        fuel -= 1;

        match db.lookup(current) {
            Some(
                TypeData::Array(element)
                | TypeData::ReadonlyType(element)
                | TypeData::NoInfer(element)
                | TypeData::KeyOf(element),
            ) => stack.push(element),
            Some(TypeData::Tuple(elements)) => {
                stack.extend(
                    db.tuple_list(elements)
                        .iter()
                        .map(|element| element.type_id),
                );
            }
            Some(TypeData::Union(list) | TypeData::Intersection(list)) => {
                stack.extend(db.type_list(list).iter().copied());
            }
            Some(TypeData::Application(application)) => {
                stack.extend(db.type_application(application).args.iter().copied());
            }
            Some(TypeData::IndexAccess(object, index)) => {
                stack.push(object);
                stack.push(index);
            }
            Some(TypeData::StringIntrinsic { type_arg, .. }) => stack.push(type_arg),
            Some(TypeData::Substitution {
                base_type,
                constraint,
            }) => {
                stack.push(base_type);
                stack.push(constraint);
            }
            _ => {}
        }
    }
    false
}

/// Get the mapped type ID if the type is a Mapped type.
pub fn get_mapped_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::MappedTypeId> {
    // Fast path: intrinsics are never `Mapped(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some(mapped_id),
        _ => None,
    }
}

/// Get the conditional type ID if the type is a Conditional type.
pub fn get_conditional_type_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::ConditionalTypeId> {
    // Fast path: intrinsics are never `Conditional(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Conditional(cond_id)) => Some(cond_id),
        _ => None,
    }
}

/// Returns true if `type_id` is a distributive `Conditional` whose
/// `check_type` defers into another type (`Lazy`, `Application`,
/// `IndexAccess`, or `KeyOf`).
///
/// Callers (currently the checker's non-generic conditional eager-eval gates)
/// use this to recognise alias bodies whose true-branch snapshot would freeze
/// the deferred union before it has been materialized on the resolver,
/// collapsing per-member distribution at the next consumer (e.g.
/// `Extract<V, P>`). Skipping eager evaluation in that shape keeps the body in
/// its raw `Conditional` form so the regular evaluator path with a fully
/// populated resolver drives distribution correctly.
pub fn is_distributive_conditional_with_deferred_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some(cond_id) = get_conditional_type_id(db, type_id) else {
        return false;
    };
    let cond = db.get_conditional(cond_id);
    cond.is_distributive
        && matches!(
            db.lookup(cond.check_type),
            Some(
                TypeData::Lazy(_)
                    | TypeData::Application(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
            )
        )
}

/// Returns true if `type_id` is an `IndexAccess(_, _)` type.
pub fn is_indexed_access(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
}

/// Returns true if `type_id` is a *deferred* indexed access — an
/// `IndexAccess(_, _)` that still mentions a type parameter (`T[K]`,
/// `T[keyof T]`, or `Obj[K]` with a generic index) — or an intersection
/// carrying one as a member.
///
/// `tsc` never relates such an operand against a union's constituents: the
/// relation defers to the operand's constraint, so it never reaches the
/// best-matching-member re-report that collapses a single-survivor nullable
/// union in diagnostics. A fully concrete indexed access evaluates before
/// display and does not qualify. Display-policy sibling of
/// `is_type_parameter_or_intersection_with_type_parameter`.
pub fn is_deferred_indexed_access_or_intersection_with_one(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let is_deferred = |t: TypeId| {
        matches!(db.lookup(t), Some(TypeData::IndexAccess(_, _)))
            && super::contains_type_parameters_db(db, t)
    };
    if is_deferred(type_id) {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Intersection(list_id)) => {
            db.type_list(list_id).iter().any(|&m| is_deferred(m))
        }
        _ => false,
    }
}

/// If `type_id` is an `IndexAccess(obj, KeyOf(obj))` (the same operand on both
/// sides — the canonical `Foo[keyof Foo]` shape used by lib types like
/// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]`), return `Some(obj)`.
///
/// This is intentionally narrow: it does not match `T[keyof T]` where `T` is
/// a generic type parameter, nor `T[U]` where the operands differ. The narrow
/// match keeps display-only resolution from kicking in on legitimately-deferred
/// indexed-access aliases (which, when prematurely evaluated, can blow up
/// recursion fuel and emit spurious TS2589s).
pub fn indexed_access_self_keyof(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Fast path: intrinsics are never `IndexAccess(_, _)`.
    if type_id.is_intrinsic() {
        return None;
    }
    let TypeData::IndexAccess(obj, idx) = db.lookup(type_id)? else {
        return None;
    };
    if idx.is_intrinsic() {
        return None;
    }
    let TypeData::KeyOf(idx_inner) = db.lookup(idx)? else {
        return None;
    };
    if idx_inner == obj { Some(obj) } else { None }
}

/// Returns true if `type_id` is still a deferred form (`Lazy` or `IndexAccess`)
/// that the solver could not reduce. Useful when deciding whether to keep an
/// alias display or substitute the resolved form.
pub fn is_deferred_lazy_or_indexed_access(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::IndexAccess(_, _) | TypeData::Lazy(_))
    )
}

/// Get the keyof inner type if the type is a `KeyOf` type.
pub fn get_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Fast path: intrinsics are never `KeyOf(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::KeyOf(inner)) => Some(inner),
        _ => None,
    }
}

/// Whether a `keyof` operand is *value-derived* — the type of a value rather
/// than a named type reference: a `typeof` query (`keyof typeof x`), an enum
/// type, an enum namespace object, or an anonymous object shape with no
/// declaring symbol.
///
/// tsc computes `keyof` over such an operand eagerly, so its diagnostics
/// render the reduced literal key union and keep a literal source un-widened
/// against it. `keyof` over a *named type* operand (interface/class) keeps
/// the written `keyof Name` spelling instead (verified against the pinned
/// typescript@7.0.2 oracle; see
/// `keyof_typeof_alias_body_reduction_tests.rs`).
pub fn keyof_operand_is_value_derived(db: &dyn TypeDatabase, operand: TypeId) -> bool {
    if operand.is_intrinsic() {
        return false;
    }
    match db.lookup(operand) {
        Some(TypeData::TypeQuery(_) | TypeData::Enum(_, _)) => true,
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape
                .flags
                .contains(crate::types::ObjectFlags::ENUM_NAMESPACE)
                || shape.symbol.is_none()
        }
        _ => false,
    }
}

/// Whether `ty` is a finite unit-literal key set: a single unit type
/// (string/number literal, unique symbol, enum member) or a union made only
/// of unit types. This is the shape a concrete `keyof` reduces to when the
/// operand has no string/number index signature, and it is the literal
/// context that keeps an assignment-source literal un-widened in
/// diagnostics.
pub fn is_finite_unit_literal_keyset(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match db.lookup(ty) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            !members.is_empty()
                && members
                    .iter()
                    .all(|&member| crate::type_queries::is_unit_type(db, member))
        }
        _ => crate::type_queries::is_unit_type(db, ty),
    }
}

// =============================================================================
// Interface Merge Type Classification
// =============================================================================

/// Classification for types when merging interfaces.
///
/// This enum provides a structured way to handle interface type merging,
/// abstracting away the internal `TypeData` representation. Used for merging
/// derived and base interface types.
#[derive(Debug, Clone, Copy)]
pub enum InterfaceMergeKind {
    /// Callable type with call/construct signatures and properties
    Callable(crate::types::CallableShapeId),
    /// Object type with properties only
    Object(crate::types::ObjectShapeId),
    /// Object type with properties and index signatures
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// Intersection type - create intersection with base
    Intersection,
    /// Other type kinds - return derived unchanged
    Other,
}

impl InterfaceMergeKind {
    /// Returns true if this kind represents a type whose properties can be
    /// structurally merged with another interface type (Callable, Object,
    /// or `ObjectWithIndex`).
    pub const fn is_structurally_mergeable(&self) -> bool {
        matches!(
            self,
            InterfaceMergeKind::Callable(_)
                | InterfaceMergeKind::Object(_)
                | InterfaceMergeKind::ObjectWithIndex(_)
        )
    }
}

/// Classify a type for interface merging operations.
///
/// This function examines a type and returns information about how to handle it
/// when merging interface types. Used by `merge_interface_types`.
///
/// # Example
///
/// ```text
/// use crate::type_queries::{classify_for_interface_merge, InterfaceMergeKind};
///
/// match classify_for_interface_merge(&db, type_id) {
///     InterfaceMergeKind::Callable(shape_id) => {
///         let shape = db.callable_shape(shape_id);
///         // Merge signatures and properties
///     }
///     InterfaceMergeKind::Object(shape_id) => {
///         let shape = db.object_shape(shape_id);
///         // Merge properties only
///     }
///     InterfaceMergeKind::ObjectWithIndex(shape_id) => {
///         let shape = db.object_shape(shape_id);
///         // Merge properties and index signatures
///     }
///     InterfaceMergeKind::Intersection => {
///         // Create intersection with base type
///     }
///     InterfaceMergeKind::Other => {
///         // Return derived unchanged
///     }
/// }
/// ```
pub fn classify_for_interface_merge(db: &dyn TypeDatabase, type_id: TypeId) -> InterfaceMergeKind {
    if type_id.is_intrinsic() {
        return InterfaceMergeKind::Other;
    }
    let Some(key) = db.lookup(type_id) else {
        return InterfaceMergeKind::Other;
    };

    match key {
        TypeData::Callable(shape_id) => InterfaceMergeKind::Callable(shape_id),
        TypeData::Object(shape_id) => InterfaceMergeKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => InterfaceMergeKind::ObjectWithIndex(shape_id),
        TypeData::Intersection(_) => InterfaceMergeKind::Intersection,
        // All other types cannot be structurally merged for interfaces
        TypeData::BoundParameter(_)
        | TypeData::Intrinsic(_)
        | TypeData::Literal(_)
        | TypeData::Union(_)
        | TypeData::Array(_)
        | TypeData::Tuple(_)
        | TypeData::Function(_)
        | TypeData::TypeParameter(_)
        | TypeData::Infer(_)
        | TypeData::Lazy(_)
        | TypeData::Recursive(_)
        | TypeData::Application(_)
        | TypeData::Conditional(_)
        | TypeData::Mapped(_)
        | TypeData::IndexAccess(_, _)
        | TypeData::KeyOf(_)
        | TypeData::TemplateLiteral(_)
        | TypeData::UniqueSymbol(_)
        | TypeData::ThisType
        | TypeData::TypeQuery(_)
        | TypeData::ReadonlyType(_)
        | TypeData::NoInfer(_)
        | TypeData::Substitution { .. }
        | TypeData::StringIntrinsic { .. }
        | TypeData::ModuleNamespace(_)
        | TypeData::UnresolvedTypeName(_)
        | TypeData::Error
        | TypeData::Enum(_, _) => InterfaceMergeKind::Other,
    }
}

// =============================================================================
// Augmentation Target Classification
// =============================================================================

/// Classification for augmentation operations on types.
///
/// Similar to `InterfaceMergeKind` but specifically for module augmentation
/// where we merge additional properties into an existing type.
#[derive(Debug, Clone)]
pub enum AugmentationTargetKind {
    /// Object type - merge properties directly
    Object(crate::types::ObjectShapeId),
    /// Object with index signatures - preserve index signatures when merging
    ObjectWithIndex(crate::types::ObjectShapeId),
    /// Callable type - merge properties while preserving signatures
    Callable(crate::types::CallableShapeId),
    /// Other type - create new object with augmentation members
    Other,
}

/// Classify a type for augmentation operations.
///
/// This function examines a type and returns information about how to handle it
/// when applying module augmentations. Used by `apply_module_augmentations`.
pub fn classify_for_augmentation(db: &dyn TypeDatabase, type_id: TypeId) -> AugmentationTargetKind {
    if type_id.is_intrinsic() {
        return AugmentationTargetKind::Other;
    }
    let Some(key) = db.lookup(type_id) else {
        return AugmentationTargetKind::Other;
    };

    match key {
        TypeData::Object(shape_id) => AugmentationTargetKind::Object(shape_id),
        TypeData::ObjectWithIndex(shape_id) => AugmentationTargetKind::ObjectWithIndex(shape_id),
        TypeData::Callable(shape_id) => AugmentationTargetKind::Callable(shape_id),
        // All other types are treated as Other for augmentation
        _ => AugmentationTargetKind::Other,
    }
}

/// Returns true if the type is exclusively composed of `false` literals and/or `never`.
///
/// Used by the checker to validate non-predicate members in a union of callables:
/// TSC permits a union to act as a type guard only when non-predicate members
/// can never return a truthy value.
pub fn is_only_false_or_never(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NEVER || type_id == TypeId::BOOLEAN_FALSE {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::LiteralValue::Boolean(false))) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_only_false_or_never(db, m))
        }
        _ => false,
    }
}

/// Check if a type is a deferred type operation (`IndexAccess` or Conditional).
///
/// Used by the checker to determine if a type alias body represents a
/// deferred computation that should not be eagerly displayed by its alias
/// name (tsc shows the expanded form for these).
pub fn is_deferred_type_operation(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::IndexAccess(_, _) | TypeData::Conditional(_))
    )
}
