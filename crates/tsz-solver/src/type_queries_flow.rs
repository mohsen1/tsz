//! Control Flow and Advanced Type Classification Queries
//!
//! This module provides classification helpers for control flow analysis
//! (narrowing, type predicates, constructor instances) and advanced type queries
//! (promise detection, comparability, contextual type parameter extraction).

use crate::TypeDatabase;
use crate::type_queries::{get_union_members, is_invokable_type};
use crate::type_queries_extended::{
    StringLiteralKeyKind, classify_for_string_literal_keys, get_string_literal_value,
};
use crate::types::{TypeData, TypeId};
use tsz_common::Atom;

// =============================================================================
// Control Flow Type Classification Helpers
// =============================================================================

/// Classification for type predicate signature extraction.
/// Used by control flow analysis to extract predicate signatures from callable types.
#[derive(Debug, Clone)]
pub enum PredicateSignatureKind {
    /// Function type - has `type_predicate` and params in function shape
    Function(crate::types::FunctionShapeId),
    /// Callable type - check `call_signatures` for predicate
    Callable(crate::types::CallableShapeId),
    /// Union - search members for predicate
    Union(Vec<TypeId>),
    /// Intersection - search members for predicate
    Intersection(Vec<TypeId>),
    /// No predicate available
    None,
}

/// Classify a type for predicate signature extraction.
pub fn classify_for_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PredicateSignatureKind {
    let Some(key) = db.lookup(type_id) else {
        return PredicateSignatureKind::None;
    };

    match key {
        TypeData::Function(shape_id) => PredicateSignatureKind::Function(shape_id),
        TypeData::Callable(shape_id) => PredicateSignatureKind::Callable(shape_id),
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            PredicateSignatureKind::Union(members.to_vec())
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            PredicateSignatureKind::Intersection(members.to_vec())
        }
        _ => PredicateSignatureKind::None,
    }
}

/// Classification for constructor instance type extraction.
/// Used by instanceof narrowing to get the instance type from a constructor.
#[derive(Debug, Clone)]
pub enum ConstructorInstanceKind {
    /// Callable type with construct signatures
    Callable(crate::types::CallableShapeId),
    /// Union - search members for construct signatures
    Union(Vec<TypeId>),
    /// Intersection - search members for construct signatures
    Intersection(Vec<TypeId>),
    /// Not a constructor type
    None,
}

/// Classify a type for constructor instance type extraction.
pub fn classify_for_constructor_instance(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ConstructorInstanceKind {
    let Some(key) = db.lookup(type_id) else {
        return ConstructorInstanceKind::None;
    };

    match key {
        TypeData::Callable(shape_id) => ConstructorInstanceKind::Callable(shape_id),
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            ConstructorInstanceKind::Union(members.to_vec())
        }
        TypeData::Intersection(members_id) => {
            let members = db.type_list(members_id);
            ConstructorInstanceKind::Intersection(members.to_vec())
        }
        _ => ConstructorInstanceKind::None,
    }
}

/// Extract the instance type from a constructor type.
///
/// Given a type with construct signatures, returns the union of their return types.
/// Recursively handles union types (collecting from all members) and intersection types
/// (returning from the first member with construct signatures).
pub fn instance_type_from_constructor(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
        return Some(type_id);
    }

    match classify_for_constructor_instance(db, type_id) {
        ConstructorInstanceKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            if shape.construct_signatures.is_empty() {
                return None;
            }
            let returns: Vec<TypeId> = shape
                .construct_signatures
                .iter()
                .map(|s| s.return_type)
                .collect();
            Some(if returns.len() == 1 {
                returns[0]
            } else {
                db.union(returns)
            })
        }
        ConstructorInstanceKind::Union(members) => {
            let instance_types: Vec<TypeId> = members
                .into_iter()
                .filter_map(|m| instance_type_from_constructor(db, m))
                .collect();
            if instance_types.is_empty() {
                None
            } else if instance_types.len() == 1 {
                Some(instance_types[0])
            } else {
                Some(db.union(instance_types))
            }
        }
        ConstructorInstanceKind::Intersection(members) => {
            // TypeScript takes the first member with construct signatures
            members
                .into_iter()
                .find_map(|m| instance_type_from_constructor(db, m))
        }
        ConstructorInstanceKind::None => None,
    }
}

/// Classification for type parameter constraint access.
/// Used by narrowing to check if a type has a constraint to narrow.
#[derive(Debug, Clone)]
pub enum TypeParameterConstraintKind {
    /// Type parameter with constraint
    TypeParameter { constraint: Option<TypeId> },
    /// Not a type parameter
    None,
}

/// Classify a type to check if it's a type parameter with a constraint.
pub fn classify_for_type_parameter_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeParameterConstraintKind {
    let Some(key) = db.lookup(type_id) else {
        return TypeParameterConstraintKind::None;
    };

    match key {
        TypeData::TypeParameter(info) | TypeData::Infer(info) => {
            TypeParameterConstraintKind::TypeParameter {
                constraint: info.constraint,
            }
        }
        _ => TypeParameterConstraintKind::None,
    }
}

/// Classification for union member access.
/// Used by narrowing to filter union members.
#[derive(Debug, Clone)]
pub enum UnionMembersKind {
    /// Union with members
    Union(Vec<TypeId>),
    /// Not a union
    NotUnion,
}

/// Classify a type to check if it's a union and get its members.
pub fn classify_for_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> UnionMembersKind {
    let Some(key) = db.lookup(type_id) else {
        return UnionMembersKind::NotUnion;
    };

    match key {
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            UnionMembersKind::Union(members.to_vec())
        }
        _ => UnionMembersKind::NotUnion,
    }
}

/// Classification for checking if a type is definitely not an object.
/// Used by instanceof and typeof narrowing.
#[derive(Debug, Clone)]
pub enum NonObjectKind {
    /// Literal type (always non-object)
    Literal,
    /// Intrinsic primitive type (void, undefined, null, boolean, number, string, bigint, symbol, never)
    IntrinsicPrimitive,
    /// Object or potentially object type
    MaybeObject,
}

/// Classify a type to check if it's definitely not an object.
pub fn classify_for_non_object(db: &dyn TypeDatabase, type_id: TypeId) -> NonObjectKind {
    let Some(key) = db.lookup(type_id) else {
        return NonObjectKind::MaybeObject;
    };

    match key {
        TypeData::Literal(_) => NonObjectKind::Literal,
        TypeData::Intrinsic(kind) => {
            use crate::IntrinsicKind;

            match kind {
                IntrinsicKind::Void
                | IntrinsicKind::Undefined
                | IntrinsicKind::Null
                | IntrinsicKind::Boolean
                | IntrinsicKind::Number
                | IntrinsicKind::String
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
                | IntrinsicKind::Never => NonObjectKind::IntrinsicPrimitive,
                _ => NonObjectKind::MaybeObject,
            }
        }
        _ => NonObjectKind::MaybeObject,
    }
}

/// Classification for property presence checking.
/// Used by 'in' operator narrowing.
#[derive(Debug, Clone)]
pub enum PropertyPresenceKind {
    /// Intrinsic object type (unknown properties)
    IntrinsicObject,
    /// Object with shape - check properties
    Object(crate::types::ObjectShapeId),
    /// Callable with properties
    Callable(crate::types::CallableShapeId),
    /// Array or Tuple - numeric access
    ArrayLike,
    /// Unknown property presence
    Unknown,
}

/// Classify a type for property presence checking.
pub fn classify_for_property_presence(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PropertyPresenceKind {
    let Some(key) = db.lookup(type_id) else {
        return PropertyPresenceKind::Unknown;
    };

    match key {
        TypeData::Intrinsic(crate::IntrinsicKind::Object) => PropertyPresenceKind::IntrinsicObject,
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            PropertyPresenceKind::Object(shape_id)
        }
        TypeData::Callable(shape_id) => PropertyPresenceKind::Callable(shape_id),
        TypeData::Array(_) | TypeData::Tuple(_) => PropertyPresenceKind::ArrayLike,
        _ => PropertyPresenceKind::Unknown,
    }
}

/// Classification for falsy component extraction.
/// Used by truthiness narrowing.
#[derive(Debug, Clone)]
pub enum FalsyComponentKind {
    /// Literal type - check if falsy value
    Literal(crate::LiteralValue),
    /// Union - get falsy component from each member
    Union(Vec<TypeId>),
    /// Type parameter or infer - keep as is
    TypeParameter,
    /// Other types - no falsy component
    None,
}

/// Classify a type for falsy component extraction.
pub fn classify_for_falsy_component(db: &dyn TypeDatabase, type_id: TypeId) -> FalsyComponentKind {
    let Some(key) = db.lookup(type_id) else {
        return FalsyComponentKind::None;
    };

    match key {
        TypeData::Literal(literal) => FalsyComponentKind::Literal(literal),
        TypeData::Union(members_id) => {
            let members = db.type_list(members_id);
            FalsyComponentKind::Union(members.to_vec())
        }
        TypeData::TypeParameter(_) | TypeData::Infer(_) => FalsyComponentKind::TypeParameter,
        _ => FalsyComponentKind::None,
    }
}

/// Classification for literal value extraction.
/// Used by element access and property access narrowing.
#[derive(Debug, Clone)]
pub enum LiteralValueKind {
    /// String literal
    String(tsz_common::interner::Atom),
    /// Number literal
    Number(f64),
    /// Not a literal
    None,
}

/// Classify a type to extract literal value (string or number).
pub fn classify_for_literal_value(db: &dyn TypeDatabase, type_id: TypeId) -> LiteralValueKind {
    let Some(key) = db.lookup(type_id) else {
        return LiteralValueKind::None;
    };

    match key {
        TypeData::Literal(crate::LiteralValue::String(atom)) => LiteralValueKind::String(atom),
        TypeData::Literal(crate::LiteralValue::Number(num)) => LiteralValueKind::Number(num.0),
        _ => LiteralValueKind::None,
    }
}

/// Check if a type is suitable as a narrowing literal value.
///
/// Returns `Some(type_id)` for types that can be used as the comparand in
/// discriminant or literal equality narrowing:
/// - Literal types (string, number, boolean, bigint)
/// - Enum member types (nominal enum values like `Types.Str`)
///
/// Returns `None` for all other types.
pub fn is_narrowing_literal(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // null and undefined are unit types that can serve as discriminants
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return Some(type_id);
    }
    let key = db.lookup(type_id)?;
    match key {
        TypeData::Literal(_) | TypeData::Enum(_, _) => Some(type_id),
        _ => None,
    }
}

/// Check if a type is a "unit type" — a type with exactly one inhabitant.
///
/// Unit types: null, undefined, void, true, false, string/number/bigint literals.
/// A union is a unit type if ALL its members are unit types (e.g. `"A" | "B" | null`).
pub fn is_unit_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
        || type_id == TypeId::BOOLEAN_TRUE
        || type_id == TypeId::BOOLEAN_FALSE
    {
        return true;
    }

    if crate::visitor::is_literal_type_db(db, type_id) {
        return true;
    }

    if let Some(list_id) = crate::visitor::union_list_id(db, type_id) {
        let members = db.type_list(list_id);
        return members.iter().all(|&m| is_unit_type(db, m));
    }

    false
}

/// Check if a union type contains a specific member type.
pub fn union_contains(db: &dyn TypeDatabase, type_id: TypeId, target: TypeId) -> bool {
    if let Some(members) = get_union_members(db, type_id) {
        members.contains(&target)
    } else {
        false
    }
}

/// Check if a type is or contains `undefined` (directly or as a union member).
pub fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::UNDEFINED || union_contains(db, type_id, TypeId::UNDEFINED)
}

/// Extract string literal key names from a type (single literal, or union of literals).
///
/// Returns an empty Vec if the type doesn't contain string literals.
pub fn extract_string_literal_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_common::interner::Atom> {
    match classify_for_string_literal_keys(db, type_id) {
        StringLiteralKeyKind::SingleString(name) => vec![name],
        StringLiteralKeyKind::Union(members) => members
            .iter()
            .filter_map(|&member| get_string_literal_value(db, member))
            .collect(),
        StringLiteralKeyKind::NotStringLiteral => Vec::new(),
    }
}

/// Extracts the return type from a callable type for declaration emit.
///
/// For overloaded functions (Callable), returns the return type of the first signature.
/// For intersections, finds the first callable member and extracts its return type.
///
/// # Examples
///
/// ```ignore
/// let return_type = type_queries::get_return_type(&db, function_type_id);
/// ```
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The `TypeId` of a function or callable type
///
/// # Returns
///
/// * `Some(TypeId)` - The return type if this is a callable type
/// * `None` - If this is not a callable type or `type_id` is unknown
pub fn get_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => Some(db.function_shape(shape_id).return_type),
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // For overloads, use the first signature's return type
            shape.call_signatures.first().map(|sig| sig.return_type)
        }
        Some(TypeData::Intersection(list_id)) => {
            // In an intersection, find the first callable member
            let members = db.type_list(list_id);
            members.iter().find_map(|&m| get_return_type(db, m))
        }
        _ => {
            // Handle special intrinsic types
            if type_id == TypeId::ANY {
                Some(TypeId::ANY)
            } else if type_id == TypeId::NEVER {
                Some(TypeId::NEVER)
            } else {
                None
            }
        }
    }
}

// =============================================================================
// Promise and Iterable Type Queries
// =============================================================================

use crate::operations_property::PropertyAccessEvaluator;

/// Check if a type is "promise-like" (has a callable 'then' method).
///
/// This is used to detect thenable types for async iterator handling.
/// A type is promise-like if it has a 'then' property that is callable.
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `resolver` - Type resolver for handling Lazy/Ref types
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If the type is promise-like (has callable 'then')
/// * `false` - Otherwise
///
/// # Examples
///
/// ```ignore
/// // Promise<T> is promise-like
/// assert!(is_promise_like(&db, &resolver, promise_type));
///
/// // any is always promise-like
/// assert!(is_promise_like(&db, &resolver, TypeId::ANY));
///
/// // Objects with 'then' method are promise-like
/// // { then: (fn: (value: T) => void) => void }
/// ```
pub fn is_promise_like(db: &dyn crate::db::QueryDatabase, type_id: TypeId) -> bool {
    // The 'any' trap: any is always promise-like
    if type_id == TypeId::ANY {
        return true;
    }

    // Use PropertyAccessEvaluator to find 'then' property
    // This handles Lazy/Ref/Intersection/Readonly correctly
    let evaluator = PropertyAccessEvaluator::new(db);
    evaluator
        .resolve_property_access(type_id, "then")
        .success_type()
        .is_some_and(|then_type| {
            // 'then' must be invokable (have call signatures) to be "thenable"
            // A class with only construct signatures is not thenable
            is_invokable_type(db, then_type)
        })
}

/// Check if a type is a valid target for for...in loops.
///
/// In TypeScript, for...in loops work on object types, arrays, and type parameters.
/// This function validates that a type can be used in a for...in statement.
///
/// # Arguments
///
/// * `db` - The type database/interner
/// * `type_id` - The type to check
///
/// # Returns
///
/// * `true` - If valid for for...in (Object, Array, `TypeParameter`, Any)
/// * `false` - Otherwise
///
/// # Examples
///
/// ```ignore
/// // Objects are valid
/// assert!(is_valid_for_in_target(&db, object_type));
///
/// // Arrays are valid
/// assert!(is_valid_for_in_target(&db, array_type));
///
/// // Type parameters are valid (generic constraints)
/// assert!(is_valid_for_in_target(&db, type_param_type));
///
/// // Primitives (except any) are not valid
/// assert!(!is_valid_for_in_target(&db, TypeId::STRING));
/// ```
pub fn is_valid_for_in_target(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Any is always valid
    if type_id == TypeId::ANY {
        return true;
    }

    // Primitives are valid (they box to objects in JS for...in)
    if type_id == TypeId::STRING || type_id == TypeId::NUMBER || type_id == TypeId::BOOLEAN {
        return true;
    }

    use crate::types::IntrinsicKind;
    match db.lookup(type_id) {
        // Object types are valid (for...in iterates properties)
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
        | Some(TypeData::Array(_))
        | Some(TypeData::TypeParameter(_))
        | Some(TypeData::Tuple(_))
        | Some(TypeData::Literal(_)) => true,
        // Unions are valid if all members are valid
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_valid_for_in_target(db, m))
        }
        // Intersections are valid if any member is valid
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_valid_for_in_target(db, m))
        }
        // Intrinsic primitives
        Some(TypeData::Intrinsic(kind)) => matches!(
            kind,
            IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Symbol
        ),
        // Everything else is not valid for for...in
        _ => false,
    }
}

/// Check if two types are "comparable" for TS2352 type assertion overlap check.
///
/// TSC uses `isTypeComparableTo` which is more relaxed than assignability.
/// Types are comparable if:
/// 1. They share at least one common object property name
/// 2. One is a base primitive type and the other is a literal/union of that primitive
/// 3. For union types, any member is comparable to the other type
///
/// This prevents false TS2352 errors on valid type assertions.
pub fn types_are_comparable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    types_are_comparable_inner(db, source, target, 0)
}

fn types_are_comparable_inner(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    // Prevent infinite recursion
    if depth > 5 {
        return false;
    }

    // Same type is always comparable
    if source == target {
        return true;
    }

    // Type parameters are not automatically comparable for TS2352 purposes.
    // Treating them as "comparable to anything" suppresses valid diagnostics
    // like asserting a specific subtype to an unconcretized type parameter.

    // Check union types: a union is comparable if ANY member is comparable
    if let Some(TypeData::Union(list_id)) = db.lookup(source) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_inner(db, m, target, depth + 1));
    }
    if let Some(TypeData::Union(list_id)) = db.lookup(target) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_inner(db, source, m, depth + 1));
    }

    // Check primitive ↔ literal comparability
    // string is comparable to any string literal
    // number is comparable to any numeric literal
    // etc.
    if is_primitive_comparable(db, source, target) || is_primitive_comparable(db, target, source) {
        return true;
    }

    // Check object property overlap
    types_have_common_properties(db, source, target, depth)
}

/// Check if a base primitive type is comparable to a literal or other form of that primitive.
fn is_primitive_comparable(db: &dyn TypeDatabase, base: TypeId, other: TypeId) -> bool {
    // string is comparable to string literals
    if base == TypeId::STRING {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::String(_));
        }
        return other == TypeId::STRING;
    }
    // number is comparable to numeric literals
    if base == TypeId::NUMBER {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::Number(_));
        }
        return other == TypeId::NUMBER;
    }
    // boolean is comparable to true/false
    if base == TypeId::BOOLEAN {
        return other == TypeId::BOOLEAN_TRUE
            || other == TypeId::BOOLEAN_FALSE
            || other == TypeId::BOOLEAN;
    }
    // bigint is comparable to bigint literals
    if base == TypeId::BIGINT {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::BigInt(_));
        }
        return other == TypeId::BIGINT;
    }
    // Two literals of the same primitive kind are comparable (e.g. "foo" ~ "baz",  1 ~ 2).
    // In tsc, comparability checks the "base constraint" — both widen to the same primitive.
    if let Some(TypeData::Literal(lit_a)) = db.lookup(base) {
        if let Some(TypeData::Literal(lit_b)) = db.lookup(other) {
            return std::mem::discriminant(&lit_a) == std::mem::discriminant(&lit_b);
        }
        // literal vs its base primitive: "foo" ~ string, 1 ~ number
        return match lit_a {
            crate::types::LiteralValue::String(_) => other == TypeId::STRING,
            crate::types::LiteralValue::Number(_) => other == TypeId::NUMBER,
            crate::types::LiteralValue::BigInt(_) => other == TypeId::BIGINT,
            crate::types::LiteralValue::Boolean(_) => {
                other == TypeId::BOOLEAN
                    || other == TypeId::BOOLEAN_TRUE
                    || other == TypeId::BOOLEAN_FALSE
            }
        };
    }
    // true/false are comparable to each other
    if (base == TypeId::BOOLEAN_TRUE || base == TypeId::BOOLEAN_FALSE)
        && (other == TypeId::BOOLEAN_TRUE || other == TypeId::BOOLEAN_FALSE)
    {
        return true;
    }
    false
}

/// Check if two types share at least one common property name.
fn types_have_common_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    // Helper to get properties from an object/callable type
    fn get_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId)> {
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect()
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = db.callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect()
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = db.type_list(list_id);
                let mut props = Vec::new();
                for &member in members.iter() {
                    props.extend(get_properties(db, member));
                }
                props
            }
            _ => Vec::new(),
        }
    }

    let source_props = get_properties(db, source);
    let target_props = get_properties(db, target);

    if source_props.is_empty() || target_props.is_empty() {
        return false;
    }

    // Consider overlap only when a shared property has comparable types.
    // Name-only matching is too permissive and suppresses valid TS2352 cases
    // on incompatible generic instantiations.
    use rustc_hash::FxHashMap;
    let mut target_by_name: FxHashMap<Atom, Vec<TypeId>> = FxHashMap::default();
    for (name, ty) in target_props {
        target_by_name.entry(name).or_default().push(ty);
    }

    source_props.iter().any(|(source_name, source_ty)| {
        target_by_name.get(source_name).is_some_and(|target_tys| {
            target_tys
                .iter()
                .any(|target_ty| types_are_comparable_inner(db, *source_ty, *target_ty, depth + 1))
        })
    })
}

/// Check if a type contains a `TypeQuery` referencing a specific symbol.
///
/// Used for TS2502 detection (circular reference in type annotation).
/// Traverses the type structure, expanding top-level lazy aliases via the provided callback.
/// Stops recursion at Function, Object, and Mapped types which break the "direct" reference cycle.
#[allow(clippy::match_same_arms)]
pub fn has_type_query_for_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    target_sym_id: u32,
    mut resolve_lazy: impl FnMut(TypeId) -> TypeId,
) -> bool {
    use crate::TypeData;
    use rustc_hash::FxHashSet;

    let mut worklist = vec![type_id];
    let mut visited = FxHashSet::default();

    while let Some(ty) = worklist.pop() {
        if !visited.insert(ty) {
            continue;
        }

        let resolved = resolve_lazy(ty);
        if resolved != ty {
            worklist.push(resolved);
            continue;
        }

        let Some(key) = db.lookup(ty) else { continue };
        match key {
            TypeData::TypeQuery(sym_ref) => {
                if sym_ref.0 == target_sym_id {
                    return true;
                }
            }
            TypeData::Array(elem) => worklist.push(elem),
            TypeData::Union(list) | TypeData::Intersection(list) => {
                let members = db.type_list(list);
                worklist.extend(members.iter().copied());
            }
            TypeData::Tuple(list) => {
                let elements = db.tuple_list(list);
                for elem in elements.iter() {
                    worklist.push(elem.type_id);
                }
            }
            TypeData::Conditional(id) => {
                let cond = db.conditional_type(id);
                worklist.push(cond.check_type);
                worklist.push(cond.extends_type);
                worklist.push(cond.true_type);
                worklist.push(cond.false_type);
            }
            TypeData::Application(id) => {
                let app = db.type_application(id);
                worklist.push(app.base);
                worklist.extend(&app.args);
            }
            TypeData::IndexAccess(obj, idx) => {
                worklist.push(obj);
                worklist.push(idx);
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) => {
                worklist.push(inner);
            }
            TypeData::Function(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Mapped(_) => {
                // These types break the "direct" reference cycle logic for TS2502.
                // Recursive types via function return/params or object properties are allowed.
            }
            _ => {}
        }
    }
    false
}

/// Extract contextual type parameters from a type.
///
/// Inspects function shapes, callable shapes (single call signature),
/// type applications (recurse into base), and unions (all members must agree).
/// Returns `None` if the type has no extractable type parameters or if
/// union members disagree.
///
/// This encapsulates the common checker pattern of extracting type parameters
/// from an expected contextual type for generic function inference.
pub fn extract_contextual_type_params(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::TypeParamInfo>> {
    extract_contextual_type_params_inner(db, type_id, 0)
}

fn extract_contextual_type_params_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: u32,
) -> Option<Vec<crate::types::TypeParamInfo>> {
    if depth > 20 {
        return None;
    }

    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            if shape.type_params.is_empty() {
                None
            } else {
                Some(shape.type_params.clone())
            }
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.call_signatures.len() != 1 {
                return None;
            }
            let sig = &shape.call_signatures[0];
            if sig.type_params.is_empty() {
                None
            } else {
                Some(sig.type_params.clone())
            }
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            extract_contextual_type_params_inner(db, app.base, depth + 1)
        }
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            if members.is_empty() {
                return None;
            }
            let mut candidate: Option<Vec<crate::types::TypeParamInfo>> = None;
            for &member in members.iter() {
                let params = extract_contextual_type_params_inner(db, member, depth + 1)?;
                if let Some(existing) = &candidate {
                    if existing.len() != params.len()
                        || existing
                            .iter()
                            .zip(params.iter())
                            .any(|(left, right)| left != right)
                    {
                        return None;
                    }
                } else {
                    candidate = Some(params);
                }
            }
            candidate
        }
        _ => None,
    }
}
