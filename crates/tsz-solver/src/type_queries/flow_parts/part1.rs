use crate::construction::TypeDatabase;

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

use crate::type_queries::{
    StringLiteralKeyKind, classify_for_string_literal_keys, get_array_element_type,
    get_callable_shape_for_type, get_string_literal_value, get_union_members, is_invokable_type,
};

use crate::types::{CallSignature, ParamInfo, TypeData, TypeId};

use rustc_hash::FxHashSet;

use std::cell::RefCell;

use tsz_common::Atom;

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

/// Check if two types are comparable for type assertion purposes (TS2352).
///
/// This is more permissive than the standard `types_are_comparable` - it only
/// requires that the types share at least one common property with comparable
/// types. It does NOT require all target properties to exist in the source.
///
/// This prevents false TS2352 errors on valid type assertions like:
/// - `{ required1: "hello" } as Foo` where Foo has additional required properties
/// - `{ payload: 'any-string' } as Action<'ACTION_A', string>`
pub fn types_are_comparable_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    types_are_comparable_for_assertion_inner(db, source, target, 0, false)
}

/// `nested` tracks whether we have descended into an object property or a
/// tuple/array element. tsc's `checkAssertionWorker` widens only the
/// *top-level* assertion source via `getWidenedType`, so two distinct literals
/// that appear as the direct operands of an assertion (e.g. `"x" as "y"`, or a
/// member of the top-level union in `"x" as "y" | "z"`) are treated as
/// overlapping, whereas distinct literals nested inside a shared property
/// (e.g. `{ k: "a" } as { k: "b" }`) are *not* widened and therefore do not
/// overlap. Union/intersection/readonly/enum decomposition of the top-level
/// types keeps `nested` unchanged; only descending through a property or
/// element sets it.
pub(super) fn types_are_comparable_for_assertion_inner(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
    nested: bool,
) -> bool {
    // Prevent infinite recursion
    if depth > 5 {
        return false;
    }

    // Same type is always comparable
    if source == target {
        return true;
    }

    // Cache the structural views once; this function pattern-matches both
    // operands repeatedly across the decomposition cases below.
    let source_data = db.lookup(source);
    let target_data = db.lookup(target);

    // Nested distinct literals do not overlap. The equal case is handled above,
    // so reaching here with two literal operands while `nested` means the
    // values differ; tsc does not widen nested literals, so `"a"` and `"b"` (or
    // `1` and `2`) are not comparable as shared property/element types. Enum
    // literals are `TypeData::Enum`, not `TypeData::Literal`, so same-enum
    // member comparability is unaffected.
    if nested
        && matches!(source_data, Some(TypeData::Literal(_)))
        && matches!(target_data, Some(TypeData::Literal(_)))
    {
        return false;
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

    // Handle Lazy types (unresolved semantic references like interface names).
    // Only assume comparable when BOTH are Lazy at depth > 0 — we can't
    // structurally compare two opaque references so we conservatively assume
    // overlap (avoids false TS2352). When only ONE side is Lazy, fall through
    // to the structural property check. The Lazy type will have no extractable
    // properties, so it correctly returns "not comparable" for cases like
    // comparing `{z: any}` (concrete Object) with `T1` (empty interface ref).
    //
    // NOTE: callers should pre-resolve Lazy types (via checker evaluation) before
    // invoking this function to avoid false TS2352 on assertions like
    // `{mode: ""} as UserSettings` where a nested interface property stays Lazy.
    let source_is_lazy = matches!(source_data, Some(TypeData::Lazy(_)));
    let target_is_lazy = matches!(target_data, Some(TypeData::Lazy(_)));
    if depth > 0 && source_is_lazy && target_is_lazy {
        return true;
    }

    // Unwrap ReadonlyType wrappers
    if let Some(TypeData::ReadonlyType(inner)) = source_data {
        return types_are_comparable_for_assertion_inner(db, inner, target, depth + 1, nested);
    }
    if let Some(TypeData::ReadonlyType(inner)) = target_data {
        return types_are_comparable_for_assertion_inner(db, source, inner, depth + 1, nested);
    }

    // Check union types
    if let Some(TypeData::Union(list_id)) = source_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, m, target, depth + 1, nested));
    }
    if let Some(TypeData::Union(list_id)) = target_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, source, m, depth + 1, nested));
    }

    // For intersection source S1 & S2 & ... & Sn: any member comparable to target suffices.
    // For intersection target T1 & T2 & ... & Tn: source must be comparable to every member
    // (tsc's eachTypeRelatedToType via comparableRelation).
    if let Some(TypeData::Intersection(list_id)) = source_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, m, target, depth + 1, nested));
    }
    if let Some(TypeData::Intersection(list_id)) = target_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .all(|&m| types_are_comparable_for_assertion_inner(db, source, m, depth + 1, nested));
    }

    // Enum comparability: unwrap to member type union, matching
    // `types_are_comparable_inner` behavior.
    if let Some(TypeData::Enum(_def_id, members_type_id)) = source_data {
        return types_are_comparable_for_assertion_inner(
            db,
            members_type_id,
            target,
            depth + 1,
            nested,
        );
    }
    if let Some(TypeData::Enum(_def_id, members_type_id)) = target_data {
        return types_are_comparable_for_assertion_inner(
            db,
            source,
            members_type_id,
            depth + 1,
            nested,
        );
    }

    // Check primitive ↔ literal comparability
    if is_primitive_comparable(db, source, target) || is_primitive_comparable(db, target, source) {
        return true;
    }

    if type_param_primitive_comparable_with_constraint(db, target, source)
        || type_param_primitive_comparable_with_constraint(db, source, target)
    {
        return true;
    }

    // A template-literal type (and string-intrinsic mapping type such as
    // `Uppercase<S>`) is a subtype of `string`. For assertion comparability
    // tsc widens the source literal to its base primitive, so any string-domain
    // type (`string`, a string literal, a template-literal, or a string
    // intrinsic) sufficiently overlaps a template-literal/string-intrinsic
    // target — e.g. `"x" as `a${number}b`` is legal even though the literal
    // text does not match the pattern. This is the assertion-only widening
    // rule; the strict comparable relation is intentionally unchanged.
    if is_string_domain_type(db, source)
        && is_string_domain_type(db, target)
        && (is_template_or_string_intrinsic(db, source)
            || is_template_or_string_intrinsic(db, target))
    {
        return true;
    }

    // The `object` primitive overlaps with any non-primitive type, including
    // `{}` and arbitrary object/array shapes. tsc's `isTypeComparableTo`
    // treats `object` as a supertype of all object-like values for assertion
    // purposes. Without this special case, `{} as T` with `T extends object`
    // falls through to the property-overlap check, which returns false
    // because both sides have empty extractable property lists.
    if (source == TypeId::OBJECT && is_object_like_for_assertion(db, target))
        || (target == TypeId::OBJECT && is_object_like_for_assertion(db, source))
    {
        return true;
    }

    // `keyof T` (the `KeyOf` type operator) reduces to a subset of
    // `string | number | symbol`, so for assertion overlap purposes it is
    // comparable to any of those primitives or their literals. tsc's
    // `isTypeComparableTo` walks `keyof T` to its key-space union; without
    // this case, an assertion like `(k as string)` where `k: keyof T` falls
    // through to the property-overlap check (KeyOf has no extractable
    // properties) and emits a false-positive TS2352.
    if is_keyof_to_string_number_symbol(db, source, target)
        || is_keyof_to_string_number_symbol(db, target, source)
    {
        return true;
    }

    // The empty object type `{}` is comparable to any type parameter whose
    // constraint contains an object-like or `object`-primitive member. tsc's
    // `isTypeComparableTo` walks the type parameter's constraint when the
    // source is a "wide" object type like `{}`. We narrow this to the empty-
    // object case only — fully unwrapping for any source would over-permit
    // assertions like `B as T extends A` (genericTypeAssertions4.ts).
    if is_empty_object_type(db, source)
        && let Some(TypeData::TypeParameter(info)) = db.lookup(target)
        && let Some(constraint) = info.constraint
    {
        return types_are_comparable_for_assertion_inner(db, source, constraint, depth + 1, nested);
    }
    if is_empty_object_type(db, target)
        && let Some(TypeData::TypeParameter(info)) = db.lookup(source)
        && let Some(constraint) = info.constraint
    {
        return types_are_comparable_for_assertion_inner(db, constraint, target, depth + 1, nested);
    }

    if callable_signatures_overlap_for_assertion(db, source, target, depth) {
        return true;
    }

    // For type assertions, only check that overlapping properties are comparable.
    // Do NOT require all target properties to exist in the source.
    super::assertion_overlap::types_have_common_properties_relaxed(db, source, target, depth)
}

fn type_param_primitive_comparable_with_constraint(
    db: &dyn TypeDatabase,
    type_param: TypeId,
    other: TypeId,
) -> bool {
    let Some(TypeData::TypeParameter(info)) = db.lookup(type_param) else {
        return false;
    };
    let Some(constraint) = info
        .constraint
        .filter(|&c| c != TypeId::ANY && c != TypeId::UNKNOWN)
    else {
        return false;
    };
    is_primitive_comparable(db, other, constraint) || is_primitive_comparable(db, constraint, other)
}

fn callable_signatures_overlap_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    if !is_direct_callable_for_assertion(db, source)
        || !is_direct_callable_for_assertion(db, target)
    {
        return false;
    }

    let Some(source_shape) = get_callable_shape_for_type(db, source) else {
        return false;
    };
    let Some(target_shape) = get_callable_shape_for_type(db, target) else {
        return false;
    };

    source_shape.call_signatures.iter().any(|source_sig| {
        target_shape.call_signatures.iter().any(|target_sig| {
            assertion_signatures_are_comparable(db, source_sig, target_sig, depth)
        })
    }) || source_shape.construct_signatures.iter().any(|source_sig| {
        target_shape.construct_signatures.iter().any(|target_sig| {
            assertion_signatures_are_comparable(db, source_sig, target_sig, depth)
        })
    })
}

fn is_direct_callable_for_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Function(_) | TypeData::Callable(_))
    )
}

fn assertion_signatures_are_comparable(
    db: &dyn TypeDatabase,
    source: &CallSignature,
    target: &CallSignature,
    depth: u32,
) -> bool {
    let (source_params, source_return) = erase_signature_for_assertion(db, source);
    let (target_params, target_return) = erase_signature_for_assertion(db, target);
    let min_params = source_params.len().min(target_params.len());

    for i in 0..min_params {
        let source_type = comparable_param_type(db, &source_params[i]);
        let target_type = comparable_param_type(db, &target_params[i]);
        if !signature_param_types_are_comparable_for_assertion(db, source_type, target_type, depth)
        {
            return false;
        }
    }

    types_are_comparable_for_assertion_inner(db, source_return, target_return, depth + 1, true)
}

fn signature_param_types_are_comparable_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    if let (Some(source_members), Some(target_members)) = (
        nullable_callable_union_members(db, source),
        nullable_callable_union_members(db, target),
    ) {
        return source_members.iter().any(|&source_member| {
            target_members.iter().any(|&target_member| {
                types_are_comparable_for_assertion_inner(
                    db,
                    source_member,
                    target_member,
                    depth + 1,
                    true,
                )
            })
        });
    }

    types_are_comparable_for_assertion_inner(db, source, target, depth + 1, true)
}

fn nullable_callable_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    let members = get_union_members(db, type_id)?;
    let has_nullable = members.iter().any(|member| member.is_nullable());
    if !has_nullable {
        return None;
    }

    let callable_members: Vec<TypeId> = members
        .into_iter()
        .filter(|&member| is_direct_callable_for_assertion(db, member))
        .collect();
    if callable_members.is_empty() {
        None
    } else {
        Some(callable_members)
    }
}

fn erase_signature_for_assertion(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
) -> (Vec<ParamInfo>, TypeId) {
    if sig.type_params.is_empty() {
        return (sig.params.clone(), sig.return_type);
    }

    let mut substitution = TypeSubstitution::new();
    for param in &sig.type_params {
        substitution.insert(param.name, TypeId::ANY);
    }

    let params = sig
        .params
        .iter()
        .map(|param| ParamInfo {
            name: param.name,
            type_id: instantiate_type(db, param.type_id, &substitution),
            optional: param.optional,
            rest: param.rest,
        })
        .collect();
    let return_type = instantiate_type(db, sig.return_type, &substitution);
    (params, return_type)
}

fn comparable_param_type(db: &dyn TypeDatabase, param: &ParamInfo) -> TypeId {
    if param.rest {
        get_array_element_type(db, param.type_id).unwrap_or(param.type_id)
    } else {
        param.type_id
    }
}

/// Returns true when `type_id` represents an "object-like" type for the
/// purposes of comparing against the `object` primitive in assertion overlap.
/// Object/Array/Tuple/Callable/Intersection all qualify; primitives like
/// `string`, `number`, `null`, `undefined` do not.
fn is_object_like_for_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Callable(_)
                | TypeData::Function(_)
                | TypeData::Intersection(_)
        )
    )
}

/// Returns true when `type_id` is the empty object type `{}` — an Object
/// type with no required properties, no callable signatures, and no index
/// signatures. Used to narrow the type-parameter constraint-unwrap rule:
/// only the "top" object type widely overlaps with any constraint that
/// contains an object-like member.
fn is_empty_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(TypeData::Object(shape_id)) = db.lookup(type_id) else {
        return false;
    };
    let shape = db.object_shape(shape_id);
    shape.properties.iter().all(|p| p.optional)
        && shape.string_index.is_none()
        && shape.number_index.is_none()
}

/// Returns true when `keyof_side` is `KeyOf(_)` and `prim_side` is one of
/// the keyof key-space primitives (`string`, `number`, `symbol`) or a
/// literal of those primitives. Used in assertion-overlap to recognize that
/// `keyof T` is structurally a subset of `string | number | symbol`.
fn is_keyof_to_string_number_symbol(
    db: &dyn TypeDatabase,
    keyof_side: TypeId,
    prim_side: TypeId,
) -> bool {
    if !matches!(db.lookup(keyof_side), Some(TypeData::KeyOf(_))) {
        return false;
    }
    if prim_side == TypeId::STRING || prim_side == TypeId::NUMBER || prim_side == TypeId::SYMBOL {
        return true;
    }
    if let Some(TypeData::Literal(lit)) = db.lookup(prim_side) {
        return matches!(
            lit,
            crate::types::LiteralValue::String(_) | crate::types::LiteralValue::Number(_)
        );
    }
    if let Some(TypeData::UniqueSymbol(_)) = db.lookup(prim_side) {
        return true;
    }
    false
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

    // Lazy (unresolved nominal reference like interface/class/enum) cannot be
    // structurally decomposed at the solver level. When encountered during a
    // property-level comparability check (depth > 0), assume the types are
    // comparable — the prior assignability checks already rejected the strict
    // case. At depth 0 the caller should have evaluated/resolved top-level
    // types, so Lazy is unexpected; fall through to other checks.
    if depth > 0
        && (matches!(db.lookup(source), Some(TypeData::Lazy(_)))
            || matches!(db.lookup(target), Some(TypeData::Lazy(_))))
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

    // Enum comparability: a literal is comparable to an enum if it matches any enum member.
    // Enums store their member types as a union in the second TypeId field.
    // For string enums, string literals are comparable if they match any member value.
    // For numeric enums, number literals are comparable if they match any member value.
    if let Some(TypeData::Enum(_def_id, members_type_id)) = db.lookup(source) {
        return types_are_comparable_inner(db, members_type_id, target, depth + 1);
    }
    if let Some(TypeData::Enum(_def_id, members_type_id)) = db.lookup(target) {
        return types_are_comparable_inner(db, source, members_type_id, depth + 1);
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
