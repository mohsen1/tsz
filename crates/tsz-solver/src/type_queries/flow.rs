//! Control Flow and Advanced Type Classification Queries
//!
//! This module provides classification helpers for control flow analysis
//! (narrowing, type predicates, constructor instances) and advanced type queries
//! (promise detection, comparability, contextual type parameter extraction).

use crate::TypeDatabase;
use crate::type_queries::{
    StringLiteralKeyKind, classify_for_string_literal_keys, get_string_literal_value,
    get_union_members, is_invokable_type,
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

/// Extracted type predicate signature from a callable/function type.
///
/// Contains the predicate and parameter list needed for type narrowing.
/// This is a higher-level query that resolves the predicate from Function
/// or Callable types without leaking shape IDs to the caller.
#[derive(Debug, Clone)]
pub struct ExtractedPredicateSignature {
    pub predicate: crate::types::TypePredicate,
    pub params: Vec<crate::types::ParamInfo>,
    /// Generic type parameters of the signature containing the predicate.
    pub type_params: Vec<crate::types::TypeParamInfo>,
}

/// Extract a type predicate signature from a type, if present.
///
/// For Function types: returns the function's type predicate + params.
/// For Callable types: returns the first call signature with a predicate.
/// For Union types: recursively searches members.
/// Returns None for types without predicates.
pub fn extract_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ExtractedPredicateSignature> {
    match classify_for_predicate_signature(db, type_id) {
        PredicateSignatureKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            let predicate = shape.type_predicate.clone()?;
            Some(ExtractedPredicateSignature {
                predicate,
                params: shape.params.clone(),
                type_params: shape.type_params.clone(),
            })
        }
        PredicateSignatureKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for sig in &shape.call_signatures {
                if let Some(predicate) = &sig.type_predicate {
                    return Some(ExtractedPredicateSignature {
                        predicate: predicate.clone(),
                        params: sig.params.clone(),
                        type_params: sig.type_params.clone(),
                    });
                }
            }
            None
        }
        PredicateSignatureKind::Union(members) | PredicateSignatureKind::Intersection(members) => {
            for member in &members {
                if let Some(sig) = extract_predicate_signature(db, *member) {
                    return Some(sig);
                }
            }
            None
        }
        PredicateSignatureKind::None => None,
    }
}

/// Returns `true` if a union of callable types is a valid type predicate.
///
/// A union of callables `F1 | F2 | ...` is a valid type predicate when:
/// - At least one member has a type predicate, AND
/// - All non-predicate members return exclusively `false` or `never`.
///
/// TypeScript spec: `(x: unknown) => x is string | (x: unknown) => false` IS valid,
/// but `(x: unknown) => x is string | (x: unknown) => boolean` is NOT (unsound).
pub fn is_valid_union_predicate(db: &dyn TypeDatabase, union_type_id: TypeId) -> bool {
    let Some(TypeData::Union(list_id)) = db.lookup(union_type_id) else {
        return false;
    };
    let members = db.type_list(list_id);
    let mut has_predicate = false;

    for &member in members.iter() {
        if extract_predicate_signature(db, member).is_some() {
            has_predicate = true;
        } else {
            // Non-predicate member: return type must be exclusively `false` or `never`
            let return_ok = match get_return_type(db, member) {
                Some(rt) => is_type_only_false_or_never(db, rt),
                None => false,
            };
            if !return_ok {
                return false;
            }
        }
    }
    has_predicate
}

/// Returns `true` if `type_id` is exclusively composed of `false` literals and/or `never`.
fn is_type_only_false_or_never(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NEVER || type_id == TypeId::BOOLEAN_FALSE {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(crate::types::LiteralValue::Boolean(false))) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_type_only_false_or_never(db, m))
        }
        _ => false,
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
/// Follows tsc's `getInstanceType` logic:
/// 1. Check for a `prototype` property whose type is not `any` (highest priority)
/// 2. Fall back to construct signature return types
///
/// Recursively handles union types (collecting from all members) and intersection types
/// (returning from the first member with construct signatures).
pub fn instance_type_from_constructor(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
        return Some(type_id);
    }

    // Step 1: Check for `prototype` property (highest priority per tsc spec).
    // If the constructor has a `prototype` property whose type is not `any`,
    // that type IS the instance type. This handles interfaces like:
    //   interface C1 { (): C1; prototype: C1; p1: string; }
    if let Some(proto_prop) =
        crate::type_queries::find_property_in_type_by_str(db, type_id, "prototype")
        && proto_prop.type_id != TypeId::ANY
    {
        return Some(proto_prop.type_id);
    }

    // Step 2: Fall back to construct signatures
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

/// Convert a literal/enum type to its string representation for template evaluation.
///
/// Returns the stringified value for:
/// - String literals → the string value
/// - Number literals → JS-style number formatting
/// - Boolean literals → "true" or "false"
/// - `BigInt` literals → the numeric string
/// - Enum members → unwraps to the underlying literal and recurses
/// - null/undefined → "null"/"undefined"
///
/// Returns `None` for non-literal types.
pub fn stringify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    if type_id == TypeId::NULL {
        return Some("null".to_string());
    }
    if type_id == TypeId::UNDEFINED {
        return Some("undefined".to_string());
    }
    let key = db.lookup(type_id)?;
    match key {
        TypeData::Literal(crate::LiteralValue::String(atom))
        | TypeData::Literal(crate::LiteralValue::BigInt(atom)) => Some(db.resolve_atom(atom)),
        TypeData::Literal(crate::LiteralValue::Number(n)) => {
            let v = n.0;
            if v.fract() == 0.0 && v.abs() < 1e20 {
                Some((v as i64).to_string())
            } else {
                Some(format!("{v}"))
            }
        }
        TypeData::Literal(crate::LiteralValue::Boolean(b)) => {
            Some(if b { "true" } else { "false" }.to_string())
        }
        TypeData::Enum(_, structural_type) => stringify_literal_type(db, structural_type),
        _ => None,
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
/// Matches tsc's `isUnitType`: `TypeFlags.Unit = Enum | Literal | UniqueESSymbol | Nullable`.
/// Unit types: null, undefined, true, false, string/number/bigint literals, enum members,
/// unique symbols. A union is a unit type if ALL its members are unit types.
///
/// NOTE: This intentionally excludes `void` and `never` to match tsc semantics.
/// For solver-internal identity optimization (which includes void/never/tuples),
/// use `is_identity_comparable_type` from `visitor_predicates`.
pub fn is_unit_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NULL
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::BOOLEAN_TRUE
        || type_id == TypeId::BOOLEAN_FALSE
    {
        return true;
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(_))
        | Some(TypeData::Enum(_, _))
        | Some(TypeData::UniqueSymbol(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().all(|&m| is_unit_type(db, m))
        }
        _ => false,
    }
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
/// ```text
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

use crate::operations::property::PropertyAccessEvaluator;

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
/// ```text
/// // Promise<T> is promise-like
/// assert!(is_promise_like(&db, &resolver, promise_type));
///
/// // any is always promise-like
/// assert!(is_promise_like(&db, &resolver, TypeId::ANY));
///
/// // Objects with 'then' method are promise-like
/// // { then: (fn: (value: T) => void) => void }
/// ```
pub fn is_promise_like(db: &dyn crate::caches::db::QueryDatabase, type_id: TypeId) -> bool {
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

    // `never` is comparable to any type (it's the bottom type, subtype of all types).
    // `any` and `unknown` are also comparable to everything.
    if source == TypeId::NEVER
        || target == TypeId::NEVER
        || source == TypeId::ANY
        || target == TypeId::ANY
        || source == TypeId::UNKNOWN
        || target == TypeId::UNKNOWN
    {
        return true;
    }

    // Unwrap ReadonlyType wrappers — `readonly T[]` is comparable to `T[]`
    if let Some(TypeData::ReadonlyType(inner)) = db.lookup(source) {
        return types_are_comparable_inner(db, inner, target, depth + 1);
    }
    if let Some(TypeData::ReadonlyType(inner)) = db.lookup(target) {
        return types_are_comparable_inner(db, source, inner, depth + 1);
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

    // Array comparability: Array<A> is comparable to Array<B> if A and B are comparable.
    // This handles cases like `(string | number)[]` as `string[]`.
    if let Some(TypeData::Array(source_elem)) = db.lookup(source)
        && let Some(TypeData::Array(target_elem)) = db.lookup(target)
    {
        return types_are_comparable_inner(db, source_elem, target_elem, depth + 1);
    }

    // Tuple→Array comparability: a tuple is comparable to an array if any tuple element
    // type is comparable to the array element type. tsc compares the tuple's element union
    // (number-indexed type) against the array's element type.
    if let Some(TypeData::Tuple(source_tuple_id)) = db.lookup(source)
        && let Some(TypeData::Array(target_elem)) = db.lookup(target)
    {
        let elements = db.tuple_list(source_tuple_id);
        return elements
            .iter()
            .any(|elem| types_are_comparable_inner(db, elem.type_id, target_elem, depth + 1));
    }
    // Array→Tuple comparability: symmetric case.
    if let Some(TypeData::Array(source_elem)) = db.lookup(source)
        && let Some(TypeData::Tuple(target_tuple_id)) = db.lookup(target)
    {
        let elements = db.tuple_list(target_tuple_id);
        return elements
            .iter()
            .any(|elem| types_are_comparable_inner(db, source_elem, elem.type_id, depth + 1));
    }

    // Tuple↔Tuple comparability: two tuples are comparable if corresponding
    // element types are pairwise comparable. For fixed-length tuples, lengths
    // must match. Rest elements are compared by their element type.
    if let Some(TypeData::Tuple(source_tuple_id)) = db.lookup(source)
        && let Some(TypeData::Tuple(target_tuple_id)) = db.lookup(target)
    {
        let source_elems = db.tuple_list(source_tuple_id);
        let target_elems = db.tuple_list(target_tuple_id);

        // Count non-rest elements and find rest elements
        let source_fixed: Vec<_> = source_elems.iter().filter(|e| !e.rest).collect();
        let target_fixed: Vec<_> = target_elems.iter().filter(|e| !e.rest).collect();
        let source_rest = source_elems.iter().find(|e| e.rest);
        let target_rest = target_elems.iter().find(|e| e.rest);

        // For fixed-length tuples (no rest elements), lengths must match
        if source_rest.is_none() && target_rest.is_none() {
            if source_fixed.len() != target_fixed.len() {
                return false;
            }
            return source_fixed
                .iter()
                .zip(target_fixed.iter())
                .all(|(s, t)| types_are_comparable_inner(db, s.type_id, t.type_id, depth + 1));
        }

        // With rest elements, check that the overlapping fixed portion is comparable
        let min_fixed = source_fixed.len().min(target_fixed.len());
        for i in 0..min_fixed {
            if !types_are_comparable_inner(
                db,
                source_fixed[i].type_id,
                target_fixed[i].type_id,
                depth + 1,
            ) {
                return false;
            }
        }
        return true;
    }

    // Callable types: check if their signatures are comparable.
    // Two callable types are comparable if they share comparable call/construct
    // signatures (parameter types and return type all comparable), OR if they
    // share common properties with comparable types.
    if let Some(TypeData::Callable(source_id)) = db.lookup(source)
        && let Some(TypeData::Callable(target_id)) = db.lookup(target)
    {
        let source_shape = db.callable_shape(source_id);
        let target_shape = db.callable_shape(target_id);

        // Check if call signatures are comparable
        if let (Some(s_sig), Some(t_sig)) = (
            source_shape.call_signatures.first(),
            target_shape.call_signatures.first(),
        ) && signatures_are_comparable(db, s_sig, t_sig, depth)
        {
            return true;
        }

        // Check construct signatures
        if let (Some(s_sig), Some(t_sig)) = (
            source_shape.construct_signatures.first(),
            target_shape.construct_signatures.first(),
        ) && signatures_are_comparable(db, s_sig, t_sig, depth)
        {
            return true;
        }

        // Fall through to property overlap check
        return types_have_common_properties(db, source, target, depth);
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

/// Check if two call signatures are comparable: all overlapping parameter pairs
/// and the return types must be comparable.
fn signatures_are_comparable(
    db: &dyn TypeDatabase,
    source: &crate::types::CallSignature,
    target: &crate::types::CallSignature,
    depth: u32,
) -> bool {
    let min_params = source.params.len().min(target.params.len());
    for i in 0..min_params {
        if !types_are_comparable_inner(
            db,
            source.params[i].type_id,
            target.params[i].type_id,
            depth + 1,
        ) {
            return false;
        }
    }
    types_are_comparable_inner(db, source.return_type, target.return_type, depth + 1)
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
    // symbol is comparable to unique symbol (unique symbol is a subtype of symbol)
    if base == TypeId::SYMBOL {
        return matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)))
            || other == TypeId::SYMBOL;
    }
    // unique symbol is comparable to symbol and to other unique symbols
    if let Some(TypeData::UniqueSymbol(_)) = db.lookup(base) {
        return other == TypeId::SYMBOL
            || matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)));
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

/// Check if two types have common properties with ALL of them having comparable types.
///
/// Returns true when the types share at least one property name AND every shared
/// property has comparable types. This matches tsc's behavior for the comparable
/// relation on object types — a single incompatible shared property means the
/// types are NOT comparable, even if other properties match.
fn types_have_common_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    // Helper to get properties from an object/callable type.
    // Returns (name, type_id, optional) — the optional flag is needed because
    // optional properties implicitly include `undefined` for comparability.
    fn get_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId, bool)> {
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = db.callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
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

    // Build a lookup table for target properties by name.
    use rustc_hash::FxHashMap;
    let mut target_by_name: FxHashMap<Atom, Vec<(TypeId, bool)>> = FxHashMap::default();
    for (name, ty, optional) in target_props {
        target_by_name.entry(name).or_default().push((ty, optional));
    }

    // Require ALL common properties to have comparable types.
    // A single incompatible shared property means the types don't overlap.
    // Properties that exist only on one side don't affect comparability.
    let mut found_common = false;
    for (source_name, source_ty, source_optional) in &source_props {
        if let Some(target_entries) = target_by_name.get(source_name) {
            found_common = true;
            let any_comparable = target_entries.iter().any(|(target_ty, target_optional)| {
                // If either property is optional, `undefined` is part of the type.
                // E.g., `a?: string` effectively has type `string | undefined`,
                // so `undefined` is comparable to it.
                if (*source_optional || *target_optional)
                    && (*source_ty == TypeId::UNDEFINED || *target_ty == TypeId::UNDEFINED)
                {
                    return true;
                }
                types_are_comparable_inner(db, *source_ty, *target_ty, depth + 1)
            });
            if !any_comparable {
                return false;
            }
        }
    }
    found_common
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::types::TupleElement;

    #[test]
    fn tuple_to_tuple_comparable_same_elements() {
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_comparable_with_never() {
        // [undefined, string] vs [never, string] — should be comparable
        // because never is comparable to everything
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::UNDEFINED,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NEVER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_incomparable_different_lengths() {
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert!(!types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn tuple_to_tuple_incomparable_different_elements() {
        // [number, string] vs [boolean, boolean] — not comparable
        let interner = TypeInterner::new();
        let t1 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t2 = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert!(!types_are_comparable(&interner, t1, t2));
    }

    #[test]
    fn never_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(
            &interner,
            TypeId::NEVER,
            TypeId::STRING
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::NEVER,
            TypeId::NUMBER
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::STRING,
            TypeId::NEVER
        ));
    }

    #[test]
    fn any_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::STRING));
        assert!(types_are_comparable(&interner, TypeId::ANY, TypeId::NUMBER));
        assert!(types_are_comparable(&interner, TypeId::STRING, TypeId::ANY));
    }

    #[test]
    fn unknown_comparable_to_any_type() {
        let interner = TypeInterner::new();
        assert!(types_are_comparable(
            &interner,
            TypeId::UNKNOWN,
            TypeId::STRING
        ));
        assert!(types_are_comparable(
            &interner,
            TypeId::STRING,
            TypeId::UNKNOWN
        ));
    }

    #[test]
    fn test_extract_predicate_signature_function() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{FunctionShape, ParamInfo, TypePredicate, TypePredicateTarget};

        // Function with type predicate
        let fn_with_pred = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(interner.intern_string("x")),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        let sig = super::extract_predicate_signature(&interner, fn_with_pred);
        assert!(sig.is_some());
        let sig = sig.unwrap();
        assert_eq!(sig.predicate.type_id, Some(TypeId::STRING));
        assert_eq!(sig.params.len(), 1);

        // Function without predicate → None
        let fn_no_pred = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert!(super::extract_predicate_signature(&interner, fn_no_pred).is_none());

        // Non-function type → None
        assert!(super::extract_predicate_signature(&interner, TypeId::STRING).is_none());
    }
}
