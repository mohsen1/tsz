//! Control Flow and Advanced Type Classification Queries
//!
//! This module provides classification helpers for control flow analysis
//! (narrowing, type predicates, constructor instances) and advanced type queries
//! (promise detection, comparability, contextual type parameter extraction).

mod comparability;

#[cfg(test)]
use comparability::is_primitive_comparable;
pub(super) use comparability::types_are_comparable_for_assertion_inner;
pub use comparability::{types_are_comparable, types_are_comparable_for_assertion};

use crate::construction::TypeDatabase;
use crate::def::resolver::{NoopResolver, TypeResolver};
use crate::evaluation::evaluate::evaluate_type_with_resolver;
use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type, instantiate_type_params_to_constraints_uncached,
};
use crate::type_queries::{
    StringLiteralKeyKind, classify_for_string_literal_keys, get_string_literal_value,
    get_union_members, is_invokable_type,
};
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

// Reusable scratch `FxHashSet<TypeId>` for the recursive DFS used by
// `has_type_query_for_symbol`. Mirrors the pool pattern from #4722 / #4790
// and several follow-up PRs.
thread_local! {
    static FLOW_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> = const { RefCell::new(None) };
}

#[inline]
fn with_flow_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = FLOW_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    FLOW_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

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
    if type_id.is_intrinsic() {
        return PredicateSignatureKind::None;
    }
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
            let predicate = shape.type_predicate?;
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
                        predicate: *predicate,
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
    if union_type_id.is_intrinsic() {
        return false;
    }
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
    if type_id.is_intrinsic() {
        return false;
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
    if type_id.is_intrinsic() {
        return ConstructorInstanceKind::None;
    }
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
/// Follows tsc's `getInstanceType` / `narrowTypeByInstanceof` logic:
/// 1. Check for `[Symbol.hasInstance]` whose call signature carries a
///    `value is T` type predicate — `T` is the instance type, overriding
///    `prototype` and any construct signatures (matches tsc:
///    `getNarrowedTypeForInstanceofPredicate`).
/// 2. Otherwise check for a `prototype` property whose type is not `any`.
/// 3. Otherwise fall back to construct signature return types.
///
/// Recursively handles union types (collecting from all members) and intersection types
/// (returning from the first member with construct signatures).
pub fn instance_type_from_constructor(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
        return Some(type_id);
    }

    // Step 1: A `[Symbol.hasInstance](value: ...): value is T` predicate, when
    // present, defines the instance type. This wins over `prototype` and over
    // construct signature return types per tsc.
    if let Some(instance_type) =
        instance_type_from_symbol_has_instance_with_any_fallback(db, type_id)
    {
        return Some(instance_type);
    }

    // Step 2: Check for `prototype` property (next priority per tsc spec).
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
                .map(|s| erase_signature_type_params_from_type(db, s.return_type, &s.type_params))
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
            // For intersection constructors (mixin pattern), the instance type
            // is the intersection of all members' instance types.
            // e.g. (new () => A) & (new () => B) → instance type is A & B
            let instance_types: Vec<TypeId> = members
                .into_iter()
                .filter_map(|m| instance_type_from_constructor(db, m))
                .collect();
            if instance_types.is_empty() {
                None
            } else if instance_types.len() == 1 {
                Some(instance_types[0])
            } else {
                Some(db.intersection(instance_types))
            }
        }
        ConstructorInstanceKind::None => None,
    }
}

/// Extract the instance type from a `[Symbol.hasInstance]` type predicate, if any.
///
/// Mirrors tsc's behaviour for `narrowTypeByInstanceof`: if the right-hand side
/// of `instanceof` declares `[Symbol.hasInstance](value: ...): value is T`,
/// the predicate's asserted type `T` IS the instance type used for narrowing,
/// overriding both `prototype` and construct signature return types.
///
/// Returns `None` when:
/// - The constructor type has no `[Symbol.hasInstance]` method,
/// - The method has no call signature with a type predicate, or
/// - The predicate is `asserts` only / has no `type_id`.
///
/// Handles unions (returns the union of per-member results) and intersections
/// (returns the first member result, matching tsc's "first signature wins").
pub fn instance_type_from_symbol_has_instance(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }

    // Recurse into union members and union the results so that
    // `(typeof A | typeof B)` with predicates on both members narrows
    // through the union of their predicate types.
    if let Some(TypeData::Union(list_id)) = db.lookup(type_id) {
        let members = db.type_list(list_id).to_vec();
        let predicate_types: Vec<TypeId> = members
            .iter()
            .filter_map(|&m| instance_type_from_symbol_has_instance(db, m))
            .collect();
        return match predicate_types.len() {
            0 => None,
            1 => Some(predicate_types[0]),
            _ => Some(db.union(predicate_types)),
        };
    }

    // For intersections, the first member with a predicate defines the
    // instance type — matches tsc's `getSymbolHasInstanceMethodOfObjectType`
    // which picks the first signature it finds.
    if let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) {
        let members = db.type_list(list_id).to_vec();
        for m in members {
            if let Some(t) = instance_type_from_symbol_has_instance(db, m) {
                return Some(t);
            }
        }
        return None;
    }

    let has_instance_prop =
        crate::type_queries::find_property_in_type_by_str(db, type_id, "[Symbol.hasInstance]")?;
    let signature = extract_predicate_signature(db, has_instance_prop.type_id)?;
    let predicate_type = erase_signature_type_params_from_type(
        db,
        signature.predicate.type_id?,
        &signature.type_params,
    );
    if signature.predicate.asserts {
        // `asserts value is T` does not narrow the instanceof source type
        // (tsc treats only non-asserting predicates as narrowing).
        return None;
    }
    Some(predicate_type)
}

/// Resolve a constructor type to the instance type its `[Symbol.hasInstance]`
/// predicate asserts, falling back to the erased generic construct return when
/// the predicate target collapses to `any`. Returns `None` when the constructor
/// has no usable `[Symbol.hasInstance]` predicate.
///
/// Shared by `instance_type_from_constructor` and `narrow_by_instanceof` so
/// both solver entry points apply identical precedence: the `any`-fallback
/// rule (`value is any` must not hide a more specific generic construct
/// candidate like `Box<any>`) is enforced exactly once.
pub fn instance_type_from_symbol_has_instance_with_any_fallback(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let predicate_type = instance_type_from_symbol_has_instance(db, type_id)?;
    if predicate_type == TypeId::ANY
        && let Some(generic_construct_type) =
            generic_construct_instance_type_from_constructor(db, type_id)
    {
        return Some(generic_construct_type);
    }
    Some(predicate_type)
}

fn generic_construct_instance_type_from_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    match classify_for_constructor_instance(db, type_id) {
        ConstructorInstanceKind::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            let returns: Vec<TypeId> = shape
                .construct_signatures
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .map(|sig| {
                    erase_signature_type_params_from_type(db, sig.return_type, &sig.type_params)
                })
                .filter(|&ty| ty != TypeId::ANY)
                .collect();
            match returns.len() {
                0 => None,
                1 => Some(returns[0]),
                _ => Some(db.union(returns)),
            }
        }
        ConstructorInstanceKind::Union(members) => {
            let instance_types: Vec<TypeId> = members
                .into_iter()
                .filter_map(|member| generic_construct_instance_type_from_constructor(db, member))
                .collect();
            match instance_types.len() {
                0 => None,
                1 => Some(instance_types[0]),
                _ => Some(db.union(instance_types)),
            }
        }
        ConstructorInstanceKind::Intersection(members) => {
            let instance_types: Vec<TypeId> = members
                .into_iter()
                .filter_map(|member| generic_construct_instance_type_from_constructor(db, member))
                .collect();
            match instance_types.len() {
                0 => None,
                1 => Some(instance_types[0]),
                _ => Some(db.intersection(instance_types)),
            }
        }
        ConstructorInstanceKind::None => None,
    }
}

fn erase_signature_type_params_from_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    type_params: &[crate::types::TypeParamInfo],
) -> TypeId {
    if type_params.is_empty() {
        return type_id;
    }

    let mut substitution = TypeSubstitution::new();
    for type_param in type_params {
        substitution.insert(type_param.name, TypeId::ANY);
    }
    instantiate_type(db, type_id, &substitution)
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
    if type_id.is_intrinsic() {
        return TypeParameterConstraintKind::None;
    }
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
    if type_id.is_intrinsic() {
        return UnionMembersKind::NotUnion;
    }
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
    if type_id.is_intrinsic() {
        return LiteralValueKind::None;
    }
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
            Some(crate::utils::js_number_to_string(n.0).into_owned())
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
/// discriminant or literal equality narrowing for general (non-unknown,
/// non-any) sources:
/// - Literal types (string, number, boolean, bigint)
/// - Enum member types (nominal enum values like `Types.Str`)
/// - `unique symbol` types
///
/// Mirrors tsc's `isNarrowingLiteralType`, which only accepts unit types
/// (`TypeFlags.Literal | UniqueESSymbol | Nullable`). Primitive intrinsics
/// (`string`, `number`, …) are intentionally NOT accepted here: in the
/// false branch of `x !== y` where `y: string`, narrowing a union source
/// `string | number` against the primitive `string` would incorrectly
/// remove the `string` member. Primitive-intrinsic comparands are only
/// valid when the source is `unknown` / `any`; that case is handled
/// separately via [`is_unknown_narrowing_literal`].
///
/// Returns `None` for all other types.
pub fn is_narrowing_literal(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // null and undefined are unit types that can serve as discriminants
    if type_id == TypeId::NULL || type_id == TypeId::UNDEFINED {
        return Some(type_id);
    }
    let key = db.lookup(type_id)?;
    match key {
        TypeData::Literal(_) | TypeData::Enum(_, _) | TypeData::UniqueSymbol(_) => Some(type_id),
        _ => None,
    }
}

/// Like [`is_narrowing_literal`], but additionally accepts primitive
/// intrinsics (`string`, `number`, `boolean`, `bigint`, `symbol`,
/// `object`) as valid narrowing comparands.
///
/// Use this only at call sites where the source is `unknown` or `any`:
/// `if (u === aString)` where `u: unknown` and `aString: string` should
/// narrow `u` to `string` in the true branch. Primitive intrinsics MUST
/// NOT flow through `is_narrowing_literal` because the false-branch
/// `narrow_excluding_type` path would then incorrectly strip primitive
/// members from union sources (e.g. narrow `string | number` to `number`
/// when the comparand is `string`).
pub fn is_unknown_narrowing_literal(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if matches!(
        type_id,
        TypeId::STRING
            | TypeId::NUMBER
            | TypeId::BOOLEAN
            | TypeId::BIGINT
            | TypeId::SYMBOL
            | TypeId::OBJECT
    ) {
        return Some(type_id);
    }
    is_narrowing_literal(db, type_id)
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
    // Other intrinsics (STRING/NUMBER/ANY/...) resolve to TypeData::Intrinsic
    // and never match Literal/Enum/UniqueSymbol/Union — skip the dyn lookup.
    if type_id.is_intrinsic() {
        return false;
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

/// Mirror of tsc's `typeCouldHaveTopLevelSingletonTypes`: whether `type_id`
/// could contain a unit (singleton) type at the top level — a literal, enum
/// member, unique symbol, `null`/`undefined`, a template-literal type, or a
/// union/intersection with any such member.
///
/// The `boolean` base intrinsic is explicitly excluded: although it is the
/// upper bound of the two boolean literals, tsc treats it as a non-singleton
/// primitive here (`if (type.flags & TypeFlags.Boolean) return false`).
///
/// Used by assignability-diagnostic source display to decide whether to
/// generalize a fresh literal source to its base type (tsc's
/// `reportRelationError`): the source literal is preserved when the target could
/// hold a singleton (a literal/union-of-literals target makes the literal vs
/// literal mismatch meaningful) and widened to its base otherwise.
pub fn type_could_have_top_level_singleton_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_could_have_top_level_singleton_types_inner(db, &NoopResolver, type_id, 0)
}

/// [`type_could_have_top_level_singleton_types`] with a resolver, so deferred
/// semantic refs answer through their constraints the way tsc's
/// `getConstraintOfType` does for every `Instantiable` type: an indexed access
/// `Cfg[K]` or conditional `Cond<T>` target whose constraint contains a unit
/// type preserves a literal source, while one whose constraint is all
/// primitives generalizes it. `Lazy`/`Application` refs are resolver-evaluated
/// first — in tsc they are already evaluated types, so this restores the same
/// answer surface.
pub fn type_could_have_top_level_singleton_types_resolved<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> bool {
    type_could_have_top_level_singleton_types_inner(db, resolver, type_id, 0)
}

/// Whether `type_id` is one of the deferred instantiable/semantic-ref forms
/// whose singleton-capacity answer requires constraint computation or resolver
/// evaluation ([`type_could_have_top_level_singleton_types_resolved`]) rather
/// than direct shape inspection. Owned here, next to the predicate's match
/// arms, so the variant list cannot drift from the predicate.
pub fn singleton_capacity_needs_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::IndexAccess(_, _)
                | TypeData::Conditional(_)
                | TypeData::Substitution { .. }
                | TypeData::Application(_)
                | TypeData::Lazy(_)
        )
    )
}

fn type_could_have_top_level_singleton_types_inner<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
    depth: u32,
) -> bool {
    // A computed constraint answers for its type only when it made progress
    // and did not collapse to the error sentinel; otherwise fall back toward
    // "no singleton capacity" (the caller's `is_unit_type` arm).
    let recurse_if_progress = |constraint: TypeId, depth: u32| {
        constraint != type_id
            && constraint != TypeId::ERROR
            && type_could_have_top_level_singleton_types_inner(db, resolver, constraint, depth + 1)
    };
    // Fuel exhaustion answers "no singleton capacity", failing toward
    // *generalizing* a literal source in the diagnostic — the direction that
    // loses precision rather than inventing a literal-vs-literal contrast.
    if depth > 16 {
        return false;
    }
    if type_id == TypeId::BOOLEAN {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            db.type_list(list_id).iter().any(|&member| {
                // tsc stores `boolean` inside a union as its two literal
                // members (`true | false`), so a union containing `boolean`
                // has top-level singleton capacity there. tsz may keep the
                // `boolean` intrinsic as the member; treat it as the
                // `true | false` pair it denotes rather than re-applying the
                // top-level `TypeFlags.Boolean` carve-out.
                member == TypeId::BOOLEAN
                    || type_could_have_top_level_singleton_types_inner(
                        db,
                        resolver,
                        member,
                        depth + 1,
                    )
            })
        }
        Some(TypeData::TemplateLiteral(_)) => true,
        // tsc's `Instantiable` branch: a type parameter (or `infer` binder)
        // answers through its constraint when it has one — a target `T extends
        // "a" | "b"` can hold a singleton, `T extends string` cannot.
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => match info.constraint {
            Some(constraint) if constraint != type_id => {
                type_could_have_top_level_singleton_types_inner(db, resolver, constraint, depth + 1)
            }
            _ => is_unit_type(db, type_id),
        },
        // Remaining `Instantiable` forms answer through their computed
        // constraint (tsc `getConstraintOfType`); no progress falls back to
        // the unit check, i.e. "no singleton capacity".
        Some(TypeData::Substitution { .. }) => recurse_if_progress(
            crate::type_queries::get_base_constraint_of_type(db, type_id),
            depth,
        ),
        Some(TypeData::IndexAccess(_, _)) => {
            // Reduce contained type parameters to their constraints, then
            // evaluate the access (tsc `getConstraintOfIndexedAccess`):
            // `Cfg[K]` with `K extends keyof Cfg` answers through the union
            // of `Cfg`'s property types. (This reduces the *index*'s type
            // parameters too, unlike `reduce_index_access_to_base_constraint`,
            // which only reduces the object side for the comparability
            // relation.)
            let substituted = instantiate_type_params_to_constraints_uncached(db, type_id);
            recurse_if_progress(
                evaluate_type_with_resolver(db, resolver, substituted),
                depth,
            )
        }
        Some(TypeData::Conditional(_)) => {
            // tsc `getDefaultConstraintOfConditionalType`: the union of the
            // (inferred) true branch and the false branch.
            match crate::type_queries::get_conditional_default_constraint(db, type_id) {
                Some(constraint) => recurse_if_progress(constraint, depth),
                None => is_unit_type(db, type_id),
            }
        }
        Some(TypeData::Lazy(def_id)) => {
            // A still-deferred semantic ref. Structural def kinds
            // (interface/class/function/...) can never evaluate to a unit
            // type, so answer without the evaluator; enums (union of
            // enum-literal members), type aliases, and unknown kinds resolve
            // and re-ask.
            match resolver.get_def_kind(def_id) {
                Some(
                    crate::def::DefKind::Interface
                    | crate::def::DefKind::Class
                    | crate::def::DefKind::ClassConstructor
                    | crate::def::DefKind::Namespace
                    | crate::def::DefKind::Function
                    | crate::def::DefKind::Variable,
                ) => false,
                _ => recurse_if_progress(evaluate_type_with_resolver(db, resolver, type_id), depth),
            }
        }
        Some(TypeData::Application(_)) => {
            // A deferred generic alias application (`Cond<T>`): tsc holds the
            // evaluated type here, so resolve it and re-ask.
            recurse_if_progress(evaluate_type_with_resolver(db, resolver, type_id), depth)
        }
        _ => is_unit_type(db, type_id),
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
    // Handle special intrinsic types first; other intrinsics resolve to
    // TypeData::Intrinsic and never match Function/Callable/Intersection.
    if type_id == TypeId::ANY {
        return Some(TypeId::ANY);
    }
    if type_id == TypeId::NEVER {
        return Some(TypeId::NEVER);
    }
    if type_id.is_intrinsic() {
        return None;
    }
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
        _ => None,
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

/// Check if a type contains a `TypeQuery` referencing a specific symbol.
///
/// Used for TS2502 detection (circular reference in type annotation).
/// Traverses the type structure, expanding top-level lazy aliases via the provided callback.
/// Stops recursion at Function, Object, and Mapped types which break the "direct" reference cycle.
pub fn has_type_query_for_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    target_sym_id: u32,
    mut resolve_lazy: impl FnMut(TypeId) -> TypeId,
) -> bool {
    with_flow_visited(|visited| {
        let mut worklist = vec![type_id];
        while let Some(ty) = worklist.pop() {
            if !visited.insert(ty) {
                continue;
            }

            if ty.is_intrinsic() {
                continue;
            }

            let resolved = resolve_lazy(ty);
            if resolved != ty {
                worklist.push(resolved);
                continue;
            }

            let Some(key) = db.lookup(ty) else { continue };
            match key {
                TypeData::TypeQuery(sym_ref) if sym_ref.0 == target_sym_id => {
                    return true;
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
                _ => {
                    // `Function`, `Object`, `ObjectWithIndex`, and `Mapped` intentionally stop
                    // traversal here: they break the "direct" reference cycle check for TS2502,
                    // because recursive types via function return/params or object properties
                    // are allowed.
                }
            }
        }
        false
    })
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
    if type_id.is_intrinsic() {
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
            let first = shape.call_signatures.first()?;
            // tsc `getContextualSignature`: an overloaded callable contributes the
            // *combined* signature's type parameters only when every call
            // signature is combinable — identical type-parameter arity and
            // constraints (`getIntersectedSignatures` /
            // `compareTypeParametersIdentical`). Two overloads that each declare
            // their own `<T>` are combinable, so a function expression assigned to
            // `{ <T>(x: T): string; <T>(x: T): number }` adopts a single `<T>` and
            // stays a generic identity (`<T>(x: T) => T`) that each overload can
            // instantiate, rather than a non-generic `(x: T) => T` that fails to
            // satisfy either overload. A non-combinable set (differing arity, e.g.
            // a generic/non-generic mix) contributes nothing, matching the sibling
            // gate in the contextual-parameter extractor.
            if shape.call_signatures.len() > 1
                && !shape.call_signatures[1..].iter().all(|sig| {
                    crate::contextual::extractors::type_parameters_identical(
                        db,
                        &first.type_params,
                        &sig.type_params,
                    )
                })
            {
                return None;
            }
            if first.type_params.is_empty() {
                None
            } else {
                Some(first.type_params.clone())
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
    use crate::construction::TypeInterner;
    use crate::types::TupleElement;

    #[test]
    fn void_and_undefined_are_assertion_comparable_both_directions() {
        let db = TypeInterner::new();
        // `void` and `undefined` overlap in tsc's comparable relation, in both
        // assertion directions.
        assert!(types_are_comparable_for_assertion(
            &db,
            TypeId::VOID,
            TypeId::UNDEFINED
        ));
        assert!(types_are_comparable_for_assertion(
            &db,
            TypeId::UNDEFINED,
            TypeId::VOID
        ));
        // The rule stays scoped to void/undefined — unrelated primitives stay
        // incomparable.
        assert!(!types_are_comparable_for_assertion(
            &db,
            TypeId::UNDEFINED,
            TypeId::STRING
        ));
        assert!(!types_are_comparable_for_assertion(
            &db,
            TypeId::VOID,
            TypeId::NUMBER
        ));
    }

    #[test]
    fn singleton_predicate_excludes_base_primitives() {
        let interner = TypeInterner::new();
        // Base primitives cannot hold a top-level singleton: source literals
        // widen against them.
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::NUMBER
        ));
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::STRING
        ));
        // `boolean` is two literals but is treated as a non-singleton primitive,
        // mirroring tsc's explicit `TypeFlags.Boolean` carve-out.
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN
        ));
    }

    #[test]
    fn singleton_predicate_includes_unit_types() {
        let interner = TypeInterner::new();
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN_TRUE
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::NULL
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            TypeId::UNDEFINED
        ));
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            interner.literal_number(1.0)
        ));
    }

    #[test]
    fn singleton_predicate_unions_use_any_member() {
        let interner = TypeInterner::new();
        // No singleton member -> false (source widens).
        let primitive_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            primitive_union
        ));
        // Any singleton member -> true (source preserved), even alongside a
        // non-singleton member.
        let mixed_union = interner.union(vec![interner.literal_number(1.0), TypeId::STRING]);
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            mixed_union
        ));
    }

    #[test]
    fn singleton_predicate_boolean_union_member_counts_as_singleton() {
        let interner = TypeInterner::new();
        // tsc stores `boolean` in a union as `true | false` (unit members), so
        // `string | boolean` has singleton capacity even though a bare
        // `boolean` target does not. Build the member list explicitly so the
        // interner's union normalization cannot pre-flatten the intrinsic
        // away and mask the member-level rule.
        let with_boolean = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
        assert!(type_could_have_top_level_singleton_types(
            &interner,
            with_boolean
        ));
        assert!(!type_could_have_top_level_singleton_types(
            &interner,
            TypeId::BOOLEAN
        ));
    }

    #[test]
    fn singleton_predicate_conditional_answers_through_default_constraint() {
        let interner = TypeInterner::new();
        let check = interner.type_param(crate::types::TypeParamInfo::simple(
            interner.intern_string("T"),
        ));
        // `T extends string ? "a" | "b" : number` — default constraint
        // contains units -> singleton-capable.
        let unit_branch = interner.union(vec![
            interner.literal_string("a"),
            interner.literal_string("b"),
        ]);
        let cond_unit = interner.conditional(crate::types::ConditionalType {
            check_type: check,
            extends_type: TypeId::STRING,
            true_type: unit_branch,
            false_type: TypeId::NUMBER,
            is_distributive: true,
        });
        assert!(type_could_have_top_level_singleton_types(
            &interner, cond_unit
        ));
        // `T extends string ? string : number` — all-primitive constraint.
        let cond_prim = interner.conditional(crate::types::ConditionalType {
            check_type: check,
            extends_type: TypeId::STRING,
            true_type: TypeId::STRING,
            false_type: TypeId::NUMBER,
            is_distributive: true,
        });
        assert!(!type_could_have_top_level_singleton_types(
            &interner, cond_prim
        ));
    }

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

    /// Verify that when object property types are Lazy (unresolved), the
    /// solver's comparable check correctly returns false (not comparable),
    /// because Lazy types have no extractable properties for structural
    /// comparison.  The CHECKER is responsible for resolving Lazy types
    /// before calling this function (via `deep_evaluate_object_properties`).
    #[test]
    fn assertion_comparable_object_with_lazy_property_not_resolved_by_solver() {
        use crate::def::DefId;
        use crate::types::{PropertyInfo, Visibility};

        let db = TypeInterner::new();

        let mode_name = db.intern_string("mode");
        let source = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }]);

        // Target has Lazy property type — solver cannot resolve it
        let lazy_ref = db.lazy(DefId(9999));
        let target = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: lazy_ref,
            write_type: lazy_ref,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }]);

        // Solver returns false because Lazy types are opaque here.
        // The checker resolves Lazy types before calling this function.
        assert!(
            !types_are_comparable_for_assertion(&db, source, target),
            "Unresolved Lazy property should not be comparable at solver level"
        );
    }

    /// When property types are both concrete (no Lazy), objects with a
    /// matching property whose types are comparable should be comparable.
    #[test]
    fn assertion_comparable_objects_with_resolved_enum_property() {
        use crate::def::DefId;
        use crate::types::{PropertyInfo, Visibility};

        let db = TypeInterner::new();

        let mode_name = db.intern_string("mode");
        // Source: { mode: string }
        let source = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }]);

        // Target: { mode: AutomationMode } (enum with string members)
        let structural_union = db.union(vec![
            db.literal_string(""),
            db.literal_string("time"),
            db.literal_string("system"),
        ]);
        let enum_type = db.enum_type(DefId(8888), structural_union);
        let target = db.object(vec![PropertyInfo {
            name: mode_name,
            type_id: enum_type,
            write_type: enum_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        }]);

        // When both sides are resolved, the comparable check succeeds
        // because string is comparable to a string enum.
        assert!(
            types_are_comparable_for_assertion(&db, source, target),
            "Object with string property should be comparable to object with string enum property"
        );
    }

    /// `instance_type_from_constructor` returns the predicate type of
    /// `[Symbol.hasInstance]` (overriding construct signature returns).
    ///
    /// This locks in tsc parity for `interface T { new (): A; [Symbol.hasInstance](v: unknown): value is B; }` —
    /// the predicate type `B` defines the instance type, NOT the construct
    /// signature return `A`. Variable name is verified with two iteration
    /// names (P, K) in `instance_type_from_symbol_has_instance_predicate_works_for_any_value_name`.
    #[test]
    fn instance_type_from_constructor_uses_symbol_has_instance_predicate() {
        use crate::types::{
            CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        // [Symbol.hasInstance](value: unknown): value is STRING
        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        // Constructor: { new (): NUMBER; [Symbol.hasInstance](value: unknown): value is STRING }
        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature::new(vec![], TypeId::NUMBER)],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
                non_widening: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        let result = super::instance_type_from_constructor(&db, constructor);
        assert_eq!(
            result,
            Some(TypeId::STRING),
            "Predicate type STRING must override construct sig return NUMBER"
        );
    }

    #[test]
    fn instance_type_from_constructor_erases_generic_construct_return_to_any() {
        use crate::def::DefId;
        use crate::types::{CallSignature, CallableShape, TypeParamInfo};

        let db = crate::intern::TypeInterner::new();
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4242));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![t_info],
                params: vec![],
                this_type: None,
                return_type: box_t,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "generic construct signatures must produce their erased instance type for instanceof"
        );
    }

    #[test]
    fn instance_type_from_symbol_has_instance_erases_generic_predicate_to_any() {
        use crate::def::DefId;
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4243));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![t_info],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(box_t),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: true,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
                non_widening: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "generic Symbol.hasInstance predicates must erase their own type parameters to any"
        );
    }

    #[test]
    fn instance_type_from_constructor_uses_generic_construct_when_predicate_collapses_to_any() {
        use crate::def::DefId;
        use crate::types::{
            CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypeParamInfo,
            TypePredicate, TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let t_name = db.intern_string("T");
        let t_info = TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let t_type = db.type_param(t_info);
        let box_base = db.lazy(DefId(4244));
        let box_t = db.application(box_base, vec![t_type]);
        let box_any = db.application(box_base, vec![TypeId::ANY]);
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let has_instance_fn = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: false,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::ANY),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: true,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![t_info],
                params: vec![],
                this_type: None,
                return_type: box_t,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: has_instance_fn,
                write_type: has_instance_fn,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
                non_widening: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_constructor(&db, constructor),
            Some(box_any),
            "a collapsed any predicate should not hide the concrete erased generic construct candidate"
        );
    }

    /// `instance_type_from_symbol_has_instance` does not depend on the
    /// user-chosen parameter name — `value` and `x` give identical results.
    /// Locks in §25 of `.claude/CLAUDE.md` (no hardcoded user-chosen names).
    #[test]
    fn instance_type_from_symbol_has_instance_predicate_works_for_any_value_name() {
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        for &param_name in &["value", "x"] {
            let db = crate::intern::TypeInterner::new();
            let name_atom = db.intern_string(param_name);
            let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

            let fn_id = db.function(FunctionShape {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(name_atom),
                    type_id: TypeId::UNKNOWN,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: Some(TypePredicate {
                    asserts: false,
                    target: TypePredicateTarget::Identifier(name_atom),
                    type_id: Some(TypeId::NUMBER),
                    parameter_index: Some(0),
                }),
                is_constructor: false,
                is_method: false,
            });

            let constructor = db.callable(CallableShape {
                call_signatures: vec![],
                construct_signatures: vec![],
                properties: vec![PropertyInfo {
                    name: has_instance_atom,
                    type_id: fn_id,
                    write_type: fn_id,
                    optional: false,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                    non_widening: false,
                }],
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            });

            assert_eq!(
                super::instance_type_from_symbol_has_instance(&db, constructor),
                Some(TypeId::NUMBER),
                "Predicate type must be parameter-name-independent (got param={param_name})"
            );
        }
    }

    /// `asserts value is T` does NOT carry through to instanceof narrowing —
    /// tsc only uses non-asserting predicates for the instanceof candidate.
    #[test]
    fn instance_type_from_symbol_has_instance_ignores_asserts_predicate() {
        use crate::types::{
            CallableShape, FunctionShape, ParamInfo, PropertyInfo, TypePredicate,
            TypePredicateTarget, Visibility,
        };

        let db = crate::intern::TypeInterner::new();
        let value_atom = db.intern_string("value");
        let has_instance_atom = db.intern_string("[Symbol.hasInstance]");

        let fn_id = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(value_atom),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: Some(TypePredicate {
                asserts: true,
                target: TypePredicateTarget::Identifier(value_atom),
                type_id: Some(TypeId::STRING),
                parameter_index: Some(0),
            }),
            is_constructor: false,
            is_method: false,
        });

        let constructor = db.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![],
            properties: vec![PropertyInfo {
                name: has_instance_atom,
                type_id: fn_id,
                write_type: fn_id,
                optional: false,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
                non_widening: false,
            }],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });

        assert_eq!(
            super::instance_type_from_symbol_has_instance(&db, constructor),
            None,
            "asserts predicates must not be used for instanceof narrowing"
        );
    }

    /// Two distinct string literals remain broadly primitive-comparable. The
    /// stricter value-level rule is applied by assertion property overlap, not
    /// by this shared primitive helper.
    #[test]
    fn distinct_string_literals_are_primitive_comparable() {
        let db = TypeInterner::new();
        let lit_draft = db.literal_string("draft");
        let lit_published = db.literal_string("published");
        assert!(
            is_primitive_comparable(&db, lit_draft, lit_published),
            "\"draft\" must remain primitive-comparable to \"published\""
        );
        assert!(
            is_primitive_comparable(&db, lit_published, lit_draft),
            "\"published\" must remain primitive-comparable to \"draft\""
        );
    }

    /// Two identical string literals must be primitive-comparable (same value).
    #[test]
    fn same_string_literal_is_comparable() {
        let db = TypeInterner::new();
        let lit_a = db.literal_string("draft");
        let lit_b = db.literal_string("draft");
        assert!(
            is_primitive_comparable(&db, lit_a, lit_b),
            "\"draft\" must be primitive-comparable to \"draft\""
        );
    }

    /// A string literal must be primitive-comparable to its base primitive.
    #[test]
    fn string_literal_comparable_to_string_primitive() {
        let db = TypeInterner::new();
        let lit = db.literal_string("hello");
        assert!(
            is_primitive_comparable(&db, lit, TypeId::STRING),
            "\"hello\" must be primitive-comparable to `string`"
        );
        assert!(
            is_primitive_comparable(&db, TypeId::STRING, lit),
            "`string` must be primitive-comparable to \"hello\""
        );
    }

    /// Two distinct number literals remain broadly primitive-comparable.
    #[test]
    fn distinct_number_literals_are_primitive_comparable() {
        let db = TypeInterner::new();
        let lit_200 = db.literal_number(200.0);
        let lit_404 = db.literal_number(404.0);
        assert!(
            is_primitive_comparable(&db, lit_200, lit_404),
            "200 must remain primitive-comparable to 404"
        );
    }

    /// Verify that enum structural union types are comparable to their
    /// base primitive type via `is_primitive_comparable` union decomposition.
    #[test]
    fn enum_structural_union_comparable_to_base_primitive() {
        use crate::def::DefId;

        let db = TypeInterner::new();

        // Create enum structural type: "" | "time" | "system"
        let lit_empty = db.literal_string("");
        let lit_time = db.literal_string("time");
        let lit_system = db.literal_string("system");
        let structural_union = db.union(vec![lit_empty, lit_time, lit_system]);

        // Create the enum type
        let enum_type = db.enum_type(DefId(8888), structural_union);

        // string should be comparable to the enum
        assert!(
            is_primitive_comparable(&db, TypeId::STRING, enum_type)
                || is_primitive_comparable(&db, enum_type, TypeId::STRING),
            "string should be primitive-comparable to a string enum"
        );

        // A string literal should also be comparable to the enum
        assert!(
            is_primitive_comparable(&db, lit_empty, enum_type)
                || is_primitive_comparable(&db, enum_type, lit_empty),
            "string literal should be primitive-comparable to a string enum containing it"
        );
    }
}
