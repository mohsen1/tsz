use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::narrowing::{GuardSense, NarrowingContext, TypeGuard};
use tsz_solver::{CallSignature, ParamInfo, PropertyInfo, TupleElement, TypeId};

use super::assignability::RelationFlags;
use super::{assignability::RelationOutcome, relation_policy};

pub(crate) use super::common::{
    LiteralValueKind, PredicateSignatureKind, PropertyAccessResult, TypeResolver, TypeSubstitution,
    array_element_type as get_array_element_type, call_signatures_for_type,
    classify_for_literal_value, classify_for_predicate_signature, construct_signatures_for_type,
    contains_free_type_parameters, contains_type_parameter_named, contains_type_parameters,
    function_shape_for_type, instantiate_type, is_assignment_operator,
    is_compound_assignment_operator, is_keyof_type, is_literal_type_through_type_constraints,
    is_logical_compound_assignment_operator, is_narrowing_literal, is_type_parameter_like,
    is_union_type, is_unit_type, is_unknown_narrowing_literal, literal_value,
    map_compound_assignment_to_binary, new_binary_op_evaluator, object_shape_for_type,
    stringify_literal_type, tuple_elements as tuple_elements_for_type, type_contains_undefined,
    union_members as union_members_for_type,
};

pub(crate) fn union_types(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::union_or_single(db, members)
}

pub(crate) fn intersection_types(db: &dyn QueryDatabase, members: Vec<TypeId>) -> TypeId {
    tsz_solver::utils::intersection_or_single(db, members)
}

pub(crate) fn array_type(db: &dyn QueryDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) const fn flow_property(name: tsz_common::Atom, type_id: TypeId) -> PropertyInfo {
    PropertyInfo::new(name, type_id)
}

pub(crate) const fn optional_flow_property(
    name: tsz_common::Atom,
    type_id: TypeId,
) -> PropertyInfo {
    PropertyInfo::opt(name, type_id)
}

pub(crate) const fn flow_tuple_element(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

pub(crate) const fn flow_call_signature(
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
) -> CallSignature {
    CallSignature {
        type_params: Vec::new(),
        params,
        this_type,
        return_type,
        type_predicate: None,
        is_method: false,
        declaration_group: 0,
    }
}

pub(crate) fn empty_object_type(db: &dyn QueryDatabase) -> TypeId {
    db.object(Vec::new())
}

pub(crate) fn object_type_from_properties(
    db: &dyn TypeDatabase,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.object(properties)
}

pub(crate) fn call_only_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    super::construct_signatures::call_only_callable_type(db, call_signatures)
}

pub(crate) fn tuple_type(
    db: &dyn QueryDatabase,
    elements: Vec<tsz_solver::TupleElement>,
) -> TypeId {
    db.tuple(elements)
}

pub(crate) fn property_type_for_contextual_type(
    db: &dyn QueryDatabase,
    contextual_type: TypeId,
    property_name: &str,
) -> Option<TypeId> {
    super::common::ContextualTypeContext::with_expected(db, contextual_type)
        .get_property_type(property_name)
}

/// Return true when a resolved receiver type has a named property whose type
/// explicitly returns `never`.
///
/// The checker owns recognizing the property-access callee and deciding when
/// the type fallback is allowed. This boundary owns the reusable semantic
/// lookup: resolve the property through the solver and inspect the resulting
/// callable return type.
pub(crate) fn property_access_function_returns_never(
    db: &dyn QueryDatabase,
    object_type: TypeId,
    property_name: &str,
) -> bool {
    if matches!(object_type, TypeId::ANY | TypeId::ERROR) {
        return false;
    }

    matches!(
        super::property_access::resolve_property_access(db, object_type, db.intern_string(property_name)),
        super::common::PropertyAccessResult::Success { type_id, .. }
            if function_return_type(db.as_type_database(), type_id) == Some(TypeId::NEVER)
    )
}

pub(crate) fn enum_member_domain(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::visitor::enum_components(db, type_id)
        .map(|(_def_id, members)| members)
        .unwrap_or(type_id)
}

/// Return whether a type carries enum component identity.
///
/// The checker owns deciding which flow assignments get enum-specific
/// reduction. This boundary owns the reusable semantic enum-domain query.
pub(crate) fn has_enum_components(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::enum_components(db, type_id).is_some()
}

pub(crate) fn enum_member_union_domain(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(members) = union_members_for_type(db, type_id) else {
        return enum_member_domain(db, type_id);
    };

    let mut normalized: Option<Vec<TypeId>> = None;
    for (index, member) in members.iter().copied().enumerate() {
        let domain = enum_member_domain(db, member);
        if let Some(normalized) = normalized.as_mut() {
            normalized.push(domain);
        } else if domain != member {
            let mut changed = Vec::with_capacity(members.len());
            changed.extend_from_slice(&members[..index]);
            changed.push(domain);
            normalized = Some(changed);
        }
    }

    normalized.map_or(type_id, |members| union_types(db, members))
}

pub(crate) fn type_has_typeof_result(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    typeof_result: &str,
) -> bool {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_by_typeof(type_id, typeof_result) != TypeId::NEVER
}

/// Compute the possible string-literal `typeof` results for a switch operand.
///
/// The checker owns recognizing `switch (typeof expr)` and resolving `expr` to
/// a `TypeId`. This boundary owns the reusable type/narrowing semantics: which
/// JavaScript `typeof` strings can survive narrowing for that operand type.
pub(crate) fn typeof_switch_domain(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    operand_type: TypeId,
) -> Option<TypeId> {
    if operand_type == TypeId::ERROR {
        return None;
    }

    const TYPEOF_RESULTS: [&str; 8] = [
        "string",
        "number",
        "bigint",
        "boolean",
        "symbol",
        "undefined",
        "object",
        "function",
    ];

    let possible: Vec<TypeId> = TYPEOF_RESULTS
        .into_iter()
        .filter(|typeof_result| type_has_typeof_result(db, env, operand_type, typeof_result))
        .map(|typeof_result| db.literal_string(typeof_result))
        .collect();

    match possible.as_slice() {
        [] => None,
        [only] => Some(*only),
        _ => Some(union_types(db.as_type_database(), possible)),
    }
}

/// Compute the possible switch discriminant type for `left ?? right`.
///
/// The checker owns recognizing a nullish-coalescing switch expression and
/// resolving each operand to a `TypeId`. This boundary owns the reusable flow
/// type algebra: remove nullish from the left operand and fall back to the
/// right operand when the left side is wholly nullish.
pub(crate) fn nullish_coalescing_switch_domain(
    db: &dyn TypeDatabase,
    left_type: TypeId,
    right_type: TypeId,
) -> Option<TypeId> {
    if left_type == TypeId::ERROR || right_type == TypeId::ERROR {
        return None;
    }

    let left_non_nullish = super::flow::narrow_optional_chain(db, left_type);
    if left_non_nullish == TypeId::ERROR {
        return None;
    }
    if left_non_nullish == TypeId::NEVER {
        return Some(right_type);
    }

    Some(union_types(db, vec![left_non_nullish, right_type]))
}

pub(crate) fn cases_exhaust_type(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    switch_type: TypeId,
    case_types: &[TypeId],
) -> bool {
    let switch_type = enum_member_domain(db.as_type_database(), switch_type);
    if matches!(switch_type, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN) || case_types.is_empty()
    {
        return false;
    }
    if case_types
        .iter()
        .any(|&ty| matches!(ty, TypeId::ERROR | TypeId::ANY | TypeId::UNKNOWN))
    {
        return false;
    }

    if case_types_exactly_cover_switch_domain(db.as_type_database(), switch_type, case_types) {
        return true;
    }

    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_excluding_types(switch_type, case_types) == TypeId::NEVER
}

fn case_type_domain(db: &dyn TypeDatabase, case_type: TypeId) -> TypeId {
    enum_member_domain(db, case_type)
}

fn case_types_exactly_cover_switch_domain(
    db: &dyn TypeDatabase,
    switch_type: TypeId,
    case_types: &[TypeId],
) -> bool {
    let Some(members) = union_members_for_type(db, switch_type) else {
        return case_types
            .iter()
            .any(|&case_type| case_type_domain(db, case_type) == switch_type);
    };

    let mut remaining: FxHashSet<TypeId> = FxHashSet::default();
    remaining.reserve(members.len());
    remaining.extend(
        members
            .iter()
            .copied()
            .map(|member| enum_member_domain(db, member)),
    );

    for &case_type in case_types {
        remaining.remove(&case_type_domain(db, case_type));
        if remaining.is_empty() {
            return true;
        }
    }

    false
}

/// Apply a solver-owned type guard to a flow type.
///
/// The checker owns recognizing the AST condition, call predicate, or assertion
/// target. This boundary owns the reusable semantic narrowing and wires the
/// optional `TypeEnvironment` so `Lazy(DefId)` inputs resolve consistently.
pub(crate) fn narrow_with_guard(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    guard: &TypeGuard,
    is_true_branch: bool,
) -> TypeId {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_type(type_id, guard, GuardSense::from(is_true_branch))
}

/// Apply a solver-owned type guard using the caller's active flow narrowing
/// context. This preserves the flow pass's resolver and shared narrowing cache
/// while keeping the guard application behind the query boundary.
pub(crate) fn narrow_with_guard_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    guard: &TypeGuard,
    is_true_branch: bool,
) -> TypeId {
    narrowing.narrow_type(type_id, guard, GuardSense::from(is_true_branch))
}

/// Apply a runtime `typeof` result to a flow type.
///
/// The checker owns recognizing `typeof x === "..."` or switch-case syntax and
/// extracting the string result. This boundary owns the semantic narrowing for
/// both positive and negative branches.
pub(crate) fn narrow_by_typeof_result(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    typeof_result: &str,
    is_true_branch: bool,
) -> TypeId {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    if is_true_branch {
        narrowing.narrow_by_typeof(type_id, typeof_result)
    } else {
        narrowing.narrow_by_typeof_negation(type_id, typeof_result)
    }
}

/// Apply a type predicate discovered from a call-expression condition.
///
/// The checker owns matching the callee, call target, optional-chain shape, and
/// branch. This boundary owns the solver narrowing operation plus the
/// tsc-compatible false-branch exclusion fallback: first try the solver's
/// predicate guard directly, then exclude the positive result or assignable
/// union members when the direct negative guard cannot reduce the input.
pub(crate) fn narrow_call_predicate_guard(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    concrete_this_type: Option<TypeId>,
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    guard: &TypeGuard,
    is_true_branch: bool,
) -> TypeId {
    let guard_sense = match guard {
        TypeGuard::Predicate { asserts: true, .. } => GuardSense::Positive,
        _ => GuardSense::from(is_true_branch),
    };
    let result = narrowing.narrow_type(type_id, guard, guard_sense);

    // The exclusion fallbacks below are a workaround for *union* sources whose
    // shallow/structural false-branch reduction the primary `narrow_type`
    // cannot perform cheaply (recursive-schema unions: typebox / ts-morph
    // `value is T`). A non-union source (`Date`, `unknown`, `{ x }`, an
    // intersection, ...) is already narrowed correctly and cheaply by the
    // primary path, so `result` is authoritative for it. Running the fallbacks
    // on a non-union source is unsound: the member-level pass compares members
    // against `predicate_type` with `strict_null_checks = false`, under which a
    // non-nullish source like `Date`/`unknown` is spuriously "related" to a
    // nullish predicate (`null` / `undefined`) and gets excluded, collapsing
    // the false branch to `never` (`!isNull(x)` where `x: Date` must stay
    // `Date`, matching tsc's `getNarrowedType`).
    if !is_true_branch
        && result == type_id
        && union_members_for_type(db.as_type_database(), type_id).is_some()
        && let TypeGuard::Predicate {
            type_id: Some(predicate_type),
            ..
        } = *guard
    {
        let positive = narrowing.narrow_type(type_id, guard, GuardSense::Positive);
        if positive != type_id && positive != TypeId::NEVER {
            // tsc's false-branch predicate exclusion is a shallow
            // `filterType(type, t => !isTypeSubsetOf(t, trueType))` over the
            // top-level union members (identity/containment only). The general
            // `narrow_excluding_type` recurses into every intersection/union
            // member with a deep `is_assignable_to` and explodes on
            // recursive-schema unions (typebox / ts-morph `value is T`, where
            // each nested schema is a distinct `TypeId` so the
            // `(source, excluded)` memo never hits). Take tsc's cheap shallow
            // path; when it cannot reduce the source, fall through to the
            // structural top-level-member pass below (tsc's
            // `directlyRelated`/intersection step) rather than the deep recursion.
            if let Some(excluded) = narrowing.narrow_excluding_positive_subset(type_id, positive)
                && excluded != type_id
            {
                return excluded;
            }
        }

        let members = union_members_for_type(db.as_type_database(), type_id)
            .unwrap_or_else(|| vec![type_id].into());
        let excluded_members: SmallVec<[TypeId; 4]> = members
            .iter()
            .copied()
            .filter(|member| {
                flow_assignability_outcome(
                    db,
                    env,
                    concrete_this_type,
                    *member,
                    predicate_type,
                    false,
                )
                .related
            })
            .collect();
        if !excluded_members.is_empty() {
            let excluded = narrowing.narrow_excluding_types(type_id, &excluded_members);
            if excluded != type_id {
                return excluded;
            }
        }
    }

    result
}

/// Apply `prop in value` flow narrowing through the solver-owned guard path.
///
/// The checker owns recognizing the `in` expression and extracting the property
/// atom. This boundary owns the reusable semantic narrowing, including generic
/// and apparent-member behavior.
pub(crate) fn narrow_in_property(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    property_name: tsz_common::interner::Atom,
    is_true_branch: bool,
) -> TypeId {
    narrow_with_guard(
        db,
        env,
        type_id,
        &TypeGuard::InProperty(property_name),
        is_true_branch,
    )
}

/// Keep only the falsy constituents of a flow type.
///
/// This is the false-branch counterpart to truthiness guard narrowing. The
/// checker supplies the control-flow fact; the solver owns the type algebra.
pub(crate) fn narrow_to_falsy(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
) -> TypeId {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_to_falsy(type_id)
}

/// Apply truthiness narrowing with the caller's active flow narrowing context.
///
/// The checker owns deciding that the condition matches the target reference.
/// The boundary owns constructing the solver truthiness guard and applying it
/// through the semantic narrowing engine.
pub(crate) fn narrow_to_truthy_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    is_true_branch: bool,
) -> TypeId {
    narrow_with_guard_in_context(narrowing, type_id, &TypeGuard::Truthy, is_true_branch)
}

/// Narrow a union by whether a property path is truthy/falsy.
///
/// The checker owns extracting the property path from syntax and deciding
/// whether a `never` result is admissible for a non-union source. The solver
/// owns evaluating property truthiness across the type.
pub(crate) fn narrow_by_property_truthiness_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    property_path: &[tsz_common::interner::Atom],
    is_true_branch: bool,
) -> TypeId {
    narrowing.narrow_by_property_truthiness(type_id, property_path, is_true_branch)
}

/// Narrow a union flow type by a discriminant property's *nullishness* using the
/// caller's active flow narrowing context.
///
/// Mirrors tsc's `narrowTypeByOptionality` discriminant arm. The checker owns
/// recognizing that the discriminant access is the left operand of a `??`/`??=`
/// (so its branches gate on nullishness, not truthiness); the solver owns
/// filtering union members by whether the property can be (non-)nullish.
pub(crate) fn narrow_by_property_nullishness_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    property_path: &[tsz_common::interner::Atom],
    is_true_branch: bool,
) -> TypeId {
    narrowing.narrow_by_property_nullishness(type_id, property_path, is_true_branch)
}

/// Exclude known values from a flow type using the caller's active flow
/// narrowing context.
///
/// The checker owns discovering the control-flow fact and deciding when a broad
/// primitive source should remain unchanged. The solver owns the set algebra
/// used to subtract those values.
pub(crate) fn narrow_excluding_types_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    excluded_types: &[TypeId],
) -> TypeId {
    narrowing.narrow_excluding_types(type_id, excluded_types)
}

/// Exclude one known value from a flow type using the caller's active flow
/// narrowing context.
///
/// The checker owns discovering the equality/nullish fact. The solver owns the
/// semantic set subtraction.
pub(crate) fn narrow_excluding_type_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    excluded_type: TypeId,
) -> TypeId {
    narrowing.narrow_excluding_type(type_id, excluded_type)
}

/// Exclude known values from a discriminant-property flow fact using the
/// caller's active flow narrowing context.
///
/// The checker owns syntax/path discovery and optional-chain guard rails. The
/// solver owns filtering union members by discriminant value.
pub(crate) fn narrow_by_excluding_discriminant_values_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    property_path: &[tsz_common::interner::Atom],
    excluded_types: &[TypeId],
) -> TypeId {
    narrowing.narrow_by_excluding_discriminant_values(type_id, property_path, excluded_types)
}

/// Narrow a flow type by a discriminant equality fact using the caller's active
/// flow narrowing context.
///
/// The checker owns property-path discovery and branch selection. The solver
/// owns filtering by discriminant value, including constraint and intersection
/// handling.
pub(crate) fn narrow_by_discriminant_for_type_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    property_path: &[tsz_common::interner::Atom],
    literal_type: TypeId,
    is_true_branch: bool,
) -> TypeId {
    narrowing.narrow_by_discriminant_for_type(type_id, property_path, literal_type, is_true_branch)
}

/// Narrow a union-like assertion target by a discriminant equality fact.
///
/// Assertion handling owns recognizing the predicate target. The solver owns
/// the discriminant filter itself.
pub(crate) fn narrow_by_discriminant_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    property_path: &[tsz_common::interner::Atom],
    literal_type: TypeId,
) -> TypeId {
    narrowing.narrow_by_discriminant(type_id, property_path, literal_type)
}

/// Keep only values compatible with a literal equality fact.
pub(crate) fn narrow_to_type_in_context(
    narrowing: &NarrowingContext<'_>,
    type_id: TypeId,
    literal_type: TypeId,
) -> TypeId {
    narrowing.narrow_to_type(type_id, literal_type)
}

/// Return whether a literal comparison target is assignable to the flow type.
pub(crate) fn literal_assignable_to_in_context(
    narrowing: &NarrowingContext<'_>,
    literal_type: TypeId,
    type_id: TypeId,
) -> bool {
    narrowing.literal_assignable_to(literal_type, type_id)
}

/// Apply a function type-predicate fact to a flow type.
///
/// The checker owns resolving the called signature, target expression, and
/// branch sense. The boundary owns constructing the solver predicate payload
/// and applying it through the semantic narrowing engine.
pub(crate) fn narrow_type_predicate(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    predicate_type: TypeId,
    asserts: bool,
    is_true_branch: bool,
) -> TypeId {
    narrow_with_guard(
        db,
        env,
        type_id,
        &TypeGuard::Predicate {
            type_id: Some(predicate_type),
            asserts,
        },
        asserts || is_true_branch,
    )
}

/// Apply an assertion predicate without an explicit type (`asserts value`).
///
/// The checker owns recognizing that the asserted value is known true after the
/// call. The solver owns truthiness narrowing beyond plain nullish removal.
pub(crate) fn narrow_asserts_truthy(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
) -> TypeId {
    narrow_with_guard(db, env, type_id, &TypeGuard::Truthy, true)
}

/// Apply a receiver-property predicate to the property flow type.
///
/// The checker owns identifying the receiver property and extracting its
/// contextual predicate type. The solver owns the predicate guard semantics for
/// the property value.
pub(crate) fn narrow_property_type_by_predicate(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    predicate_property_type: TypeId,
) -> TypeId {
    narrow_inferred_predicate_guard(
        db,
        env,
        type_id,
        &TypeGuard::Predicate {
            type_id: Some(predicate_property_type),
            asserts: false,
        },
    )
}

/// Narrow a value to the object-like branch of an `instanceof`-style check.
pub(crate) fn narrow_to_objectish(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
) -> TypeId {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_to_objectish(type_id)
}

/// Apply an `instanceof` target or `[Symbol.hasInstance]` predicate result.
///
/// The checker owns matching the binary expression and resolving the constructor
/// to an instance target. This boundary owns the solver guard payload choice so
/// `Symbol.hasInstance` predicates and normal `instanceof` guards stay behind
/// the flow query boundary.
pub(crate) fn narrow_by_instanceof_target(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
    instance_type: TypeId,
    use_predicate_guard: bool,
    is_true_branch: bool,
) -> TypeId {
    let guard = if use_predicate_guard {
        TypeGuard::Predicate {
            type_id: Some(instance_type),
            asserts: false,
        }
    } else {
        TypeGuard::Instanceof(instance_type, false)
    };

    narrow_with_guard(db, env, type_id, &guard, is_true_branch)
}

/// Apply an inferred predicate guard to a parameter type.
///
/// The checker owns recognizing an inferable predicate body and matching the
/// guard target to a parameter. This boundary owns the reusable semantic guard
/// application and wires the optional `TypeEnvironment` so `Lazy(DefId)` inputs
/// resolve consistently during solver narrowing.
pub(crate) fn narrow_inferred_predicate_guard(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    param_type: TypeId,
    guard: &TypeGuard,
) -> TypeId {
    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_type(param_type, guard, GuardSense::Positive)
}

/// Return true when a falsy branch type contains only nullish constituents.
///
/// The checker owns recognizing double-negation truthiness in an inferable
/// predicate body. This boundary owns the reusable type-shape classification
/// that decides whether the false branch is narrow enough for tsc-style
/// inferred predicate synthesis.
pub(crate) fn is_nullish_only_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if matches!(type_id, TypeId::NEVER | TypeId::NULL | TypeId::UNDEFINED) {
        return true;
    }
    if let Some(members) = union_members_for_type(db, type_id) {
        return !members.is_empty()
            && members
                .iter()
                .all(|member| matches!(*member, TypeId::NULL | TypeId::UNDEFINED));
    }
    false
}

fn resolve_assignment_reduction_type(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    type_id: TypeId,
) -> TypeId {
    let resolved = get_lazy_def_id(db, type_id)
        .and_then(|def_id| env.and_then(|environment| environment.get_def(def_id)))
        .unwrap_or(type_id);
    env.map_or(resolved, |environment| {
        evaluate_application_type(db, environment, resolved)
    })
}

fn assignment_source_assignable_to_member(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    source: TypeId,
    member: TypeId,
) -> bool {
    flow_relation_outcome(db, env, source, member, true).related
}

fn non_nullish_constraint_reduction_for_assignment(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    initial_type: TypeId,
    assigned_type: TypeId,
) -> Option<TypeId> {
    let base_constraint = assignment_reduction_base_constraint(db, initial_type);
    if base_constraint == initial_type {
        return None;
    }

    let reduced_constraint = tsz_solver::narrowing::remove_nullish(db, base_constraint);
    if reduced_constraint == base_constraint
        || reduced_constraint == initial_type
        || reduced_constraint == TypeId::NEVER
    {
        return None;
    }

    let non_nullish_initial = tsz_solver::narrowing::remove_nullish(db, initial_type);
    let assigned_type = resolve_assignment_reduction_type(db, env, assigned_type);
    // A value that is itself possibly nullish does not prove the binding is
    // non-nullish, so it must not collapse the binding to the non-nullish
    // constraint. This covers a bare generic `T extends X | undefined` (and
    // `null! as T`, whose type is `T`), whose constraint still includes
    // `undefined`. Only a definitely-non-nullish value (e.g. `x!`, typed
    // `NonNullable<T>`) may drive this reduction. Without this guard, a
    // `let v = null! as T` access loses the possibly-undefined diagnostic.
    if tsz_solver::narrowing::split_nullish_type(db, assigned_type)
        .1
        .is_some()
    {
        return None;
    }
    let assigned_matches_non_nullish_initial = if let Some(environment) = env {
        non_nullish_initial != initial_type
            && flow_relation_outcome(
                db,
                Some(environment),
                assigned_type,
                non_nullish_initial,
                true,
            )
            .related
            && flow_relation_outcome(
                db,
                Some(environment),
                non_nullish_initial,
                assigned_type,
                true,
            )
            .related
    } else {
        non_nullish_initial != initial_type
            && flow_relation_outcome(db, None, assigned_type, non_nullish_initial, true).related
            && flow_relation_outcome(db, None, non_nullish_initial, assigned_type, true).related
    };
    let assigned_has_reduced_constraint_surface = if let Some(environment) = env {
        flow_relation_outcome(db, Some(environment), assigned_type, initial_type, true).related
            && flow_relation_outcome(
                db,
                Some(environment),
                assigned_type,
                reduced_constraint,
                true,
            )
            .related
    } else {
        flow_relation_outcome(db, None, assigned_type, initial_type, true).related
            && flow_relation_outcome(db, None, assigned_type, reduced_constraint, true).related
    };
    if !(assigned_matches_non_nullish_initial || assigned_has_reduced_constraint_surface) {
        return None;
    }

    Some(reduced_constraint)
}

fn assignment_reduction_base_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if let Some((object_type, index_type)) =
        tsz_solver::type_queries::get_index_access_types(db, type_id)
    {
        let object_constraint =
            tsz_solver::type_queries::get_base_constraint_of_type(db, object_type);
        if object_constraint != object_type
            && let Some(prop_name) =
                tsz_solver::type_queries::get_string_literal_value(db, index_type)
            && let Some(prop) =
                tsz_solver::type_queries::find_property_in_object(db, object_constraint, prop_name)
        {
            return prop.type_id;
        }
    }

    tsz_solver::type_queries::get_base_constraint_of_type(db, type_id)
}

fn assigned_value_preserves_enum_identity(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    assigned_type: TypeId,
    initial_enum_def: tsz_solver::def::DefId,
) -> bool {
    if let Some(members) = union_members_for_type(db, assigned_type) {
        return !members.is_empty()
            && members.iter().all(|&member| {
                assigned_value_preserves_enum_identity(db, env, member, initial_enum_def)
            });
    }

    let Some((def_id, _)) = tsz_solver::visitor::enum_components(db, assigned_type) else {
        return false;
    };
    def_id == initial_enum_def
        || env.is_some_and(|environment| {
            environment.get_enum_parent(def_id) == Some(initial_enum_def)
        })
}

/// Narrow an enum-typed assignment target by an assigned value while preserving
/// nominal enum identity.
///
/// Bare literals and unrelated enum values collapse back to `initial_type` so a
/// later read still reports nominal enum mismatches.
pub(crate) fn narrow_enum_assignment_target(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    initial_resolved: TypeId,
    assigned_resolved: TypeId,
    initial_type: TypeId,
) -> TypeId {
    let Some((initial_def, _)) = tsz_solver::visitor::enum_components(db, initial_resolved) else {
        return initial_type;
    };
    if assigned_value_preserves_enum_identity(db, env, assigned_resolved, initial_def) {
        assigned_resolved
    } else {
        initial_type
    }
}

/// Apply tsc-style assignment reduction for flow analysis.
///
/// The checker owns the CFG walk and chooses the assignment base. This boundary
/// owns the reusable type algebra: resolving lazy/application wrappers, keeping
/// enum identity, and filtering union members by one-way assignability from the
/// assigned type.
pub(crate) fn narrow_assignment(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    initial_type: TypeId,
    assigned_type: TypeId,
) -> TypeId {
    if initial_type == TypeId::ANY
        || initial_type == TypeId::ERROR
        || initial_type == TypeId::UNKNOWN
    {
        return initial_type;
    }

    let resolved_initial = resolve_assignment_reduction_type(db, env, initial_type);

    if let Some(reduced) =
        non_nullish_constraint_reduction_for_assignment(db, env, initial_type, assigned_type)
    {
        return reduced;
    }

    if enum_member_domain(db, resolved_initial) != resolved_initial {
        let assigned_resolved = resolve_assignment_reduction_type(db, env, assigned_type);
        if !flow_relation_outcome(db, None, assigned_resolved, resolved_initial, false).related {
            return initial_type;
        }
        return narrow_enum_assignment_target(
            db,
            env,
            resolved_initial,
            assigned_resolved,
            initial_type,
        );
    }

    let Some(members) = union_members_for_type(db, resolved_initial) else {
        return initial_type;
    };
    if members.len() <= 1 {
        return initial_type;
    }

    let assigned_type = resolve_assignment_reduction_type(db, env, assigned_type);
    let assigned_members = union_members_for_type(db, assigned_type);
    let mut kept = Vec::new();
    for &member in &members {
        let assignable_to_member =
            assigned_members.as_ref().is_some_and(|sources| {
                sources
                    .iter()
                    .any(|&source| assignment_source_assignable_to_member(db, env, source, member))
            }) || assignment_source_assignable_to_member(db, env, assigned_type, member);
        if assignable_to_member {
            kept.push(member);
        }
    }

    if kept.is_empty() {
        initial_type
    } else if kept.len() == 1 {
        kept[0]
    } else {
        union_types(db, kept)
    }
}

pub(crate) fn are_types_mutually_subtype(
    db: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    tsz_solver::relations::subtype::is_subtype_of(db, left, right)
        || tsz_solver::relations::subtype::is_subtype_of(db, right, left)
}

fn flow_relation_related(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let _span = tracing::trace_span!("flow_assignable", src = source.0, tgt = target.0,).entered();

    if let Some(env) = env {
        return is_assignable_with_env(db, env, source, target, strict_null_checks);
    }

    tsz_solver::relations::relation_queries::query_relation(
        db,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Assignable,
        tsz_solver::relations::relation_queries::RelationPolicy::default(),
        tsz_solver::relations::relation_queries::RelationContext::default(),
    )
    .is_related()
}

fn flow_relation_outcome(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> RelationOutcome {
    let related = flow_relation_related(db, env, source, target, strict_null_checks);

    RelationOutcome {
        related,
        depth_exceeded: false,
        iteration_exceeded: false,
        failure: None,
        weak_union_violation: false,
        property_classification: None,
    }
}

pub(crate) fn flow_assignability_outcome(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    concrete_this_type: Option<TypeId>,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> RelationOutcome {
    let source = substitute_flow_this_type(db, concrete_this_type, source);
    let target = substitute_flow_this_type(db, concrete_this_type, target);
    flow_relation_outcome(
        db.as_type_database(),
        env,
        source,
        target,
        strict_null_checks,
    )
}

/// Validate a whole assignment source against its target.
///
/// Flow reduction intentionally uses existential overlap when selecting the
/// surviving members of a declared union. The validity gate for the write is
/// stricter: every member of a union RHS must be assignable to the LHS before
/// that reduction may run.
pub(crate) fn whole_assignment_rhs_is_compatible(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::relations::subtype::TypeEnvironment>,
    concrete_this_type: Option<TypeId>,
    source: TypeId,
    target: TypeId,
    flags: u16,
) -> bool {
    let source = substitute_flow_this_type(db, concrete_this_type, source);
    let target = substitute_flow_this_type(db, concrete_this_type, target);
    let db = db.as_type_database();
    let policy = relation_policy::from_checker_flags_u16(flags);
    let related = |source| {
        if let Some(env) = env {
            tsz_solver::relations::relation_queries::query_relation_with_resolver(
                db,
                env,
                source,
                target,
                tsz_solver::relations::relation_queries::RelationKind::Assignable,
                policy,
                tsz_solver::relations::relation_queries::RelationContext::default(),
            )
            .is_related()
        } else {
            tsz_solver::relations::relation_queries::query_relation(
                db,
                source,
                target,
                tsz_solver::relations::relation_queries::RelationKind::Assignable,
                policy,
                tsz_solver::relations::relation_queries::RelationContext::default(),
            )
            .is_related()
        }
    };
    if let Some(members) = union_members_for_type(db, source) {
        return !members.is_empty() && members.iter().copied().all(related);
    }
    related(source)
}

fn substitute_flow_this_type(
    db: &dyn QueryDatabase,
    concrete_this_type: Option<TypeId>,
    type_id: TypeId,
) -> TypeId {
    if let Some(this_type) = concrete_this_type
        && super::common::contains_this_type(db.as_type_database(), type_id)
    {
        return super::common::substitute_this_type(db, type_id, this_type);
    }
    type_id
}

pub(crate) fn fallback_compound_assignment_result(
    db: &dyn TypeDatabase,
    operator_token: u16,
    rhs_literal_type: Option<TypeId>,
) -> Option<TypeId> {
    tsz_solver::operations::compound_assignment::fallback_compound_assignment_result(
        db,
        operator_token,
        rhs_literal_type,
    )
}

pub(crate) fn widen_literal_to_primitive(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::widen_literal_to_primitive(db, type_id)
}

pub(crate) fn function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_return_type(db, type_id)
}

pub(crate) fn instance_type_from_constructor(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::instance_type_from_constructor(db, type_id)
}

/// Return the predicate type from `[Symbol.hasInstance](v: ...): v is T` if present.
///
/// Mirrors the solver's `instance_type_from_symbol_has_instance` so the checker
/// can decide whether to use type-predicate narrowing semantics (which do not
/// exclude primitives) instead of standard instanceof semantics (which do).
pub(crate) fn instance_type_from_symbol_has_instance(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::instance_type_from_symbol_has_instance(db, type_id)
}

pub(crate) fn is_promise_like_type(db: &dyn QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_promise_like(db, type_id)
}

pub(crate) fn are_types_mutually_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    types_are_subtype_with_env(db, env, left, right, strict_null_checks)
        || types_are_subtype_with_env(db, env, right, left, strict_null_checks)
}

pub(crate) fn is_assignable_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= RelationFlags::STRICT_NULL_CHECKS;
    }

    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Assignable,
        relation_policy::from_checker_flags_u16(flags),
        tsz_solver::relations::relation_queries::RelationContext::default(),
    )
    .is_related()
}

/// Extract the `DefId` from a `Lazy(DefId)` type, if it is one.
/// Used by flow-control assignment to resolve lazy types via the `TypeEnvironment`.
pub(crate) fn get_lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}

pub(crate) fn get_application_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, Vec<TypeId>)> {
    tsz_solver::type_queries::get_application_info(db, type_id)
}

/// Evaluate a type to its structural form through the canonical flow boundary.
///
/// This covers alias/application expansion for flow-control code that needs the
/// resolved structure but should not call `tsz_solver::computation::evaluate_type()` directly.
pub(crate) fn evaluate_type_structure(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::computation::evaluate_type(db, type_id)
}

/// If `type_id` is a promise-like application type, return the inner type argument.
/// Used by flow-control assignment to unwrap `await` RHS types.
pub(crate) fn unwrap_promise_type_argument(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    if let Some((base, args)) = tsz_solver::type_queries::get_application_info(db, type_id)
        && (base == TypeId::PROMISE_BASE || tsz_solver::type_queries::is_promise_like(db, type_id))
    {
        return args.first().copied();
    }
    None
}

pub(crate) use tsz_solver::type_queries::flow::ExtractedPredicateSignature;

/// Re-export for flow narrowing: extract the predicate signature from a callable type.
pub(crate) fn extract_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ExtractedPredicateSignature> {
    tsz_solver::type_queries::flow::extract_predicate_signature(db, type_id)
}

/// Re-export for flow narrowing: extract the predicate signature from a callee
/// type while treating overloaded (multi-call-signature) callables as having no
/// statically-derivable predicate. The predicate for an overloaded call must
/// come from the signature overload resolution selected at the call site, not
/// from scanning every overload.
pub(crate) fn extract_predicate_signature_for_narrowing(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ExtractedPredicateSignature> {
    tsz_solver::type_queries::predicate_narrowing::extract_predicate_signature_for_narrowing(
        db, type_id,
    )
}

/// Check if a type is only `false` or `never` (used for assertion-function detection).
pub(crate) fn is_only_false_or_never(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_only_false_or_never(db, type_id)
}

/// Get the function shape for a type, if it is a function type.
///
/// Used by flow analysis to inspect callback parameter predicates when resolving
/// generic type predicates (e.g., inferring `ValueT` from a callback argument's
/// type predicate in `doesValueAtDeepPathSatisfy`).
pub(crate) fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

/// Get type parameter info (constraint, default, name) for a type parameter.
pub(crate) fn type_param_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::TypeParamInfo> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id)
}

/// Infer concrete bindings for a generic signature's type parameters by
/// structurally matching each `(declared parameter type, concrete argument
/// type)` pair, using the solver's call-resolution inference engine.
///
/// This is the boundary used to instantiate a type-predicate target when a
/// type parameter appears nested inside a parameter type (a generic alias or
/// wrapper such as `MaybeAsync<T> = T | AsyncIterable<T>`), where a direct
/// parameter/type-parameter identity check cannot recover the binding.
pub(crate) fn infer_type_arguments_from_param_args(
    db: &dyn QueryDatabase,
    type_params: &[tsz_solver::TypeParamInfo],
    param_arg_pairs: &[(TypeId, TypeId)],
) -> Vec<(tsz_common::interner::Atom, TypeId)> {
    tsz_solver::computation::infer_type_arguments_from_param_args(db, type_params, param_arg_pairs)
}

/// Evaluate an application type via the solver's `ApplicationEvaluator`.
///
/// This is the boundary entry point for flow-control code that needs to
/// evaluate generic application types (e.g., `Array<T>`) to their concrete
/// form. Callers should use this instead of constructing `ApplicationEvaluator`
/// directly.
pub(crate) fn evaluate_application_type(
    db: &dyn TypeDatabase,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::computation::ApplicationEvaluator::new(db, env).evaluate_or_original(type_id)
}

fn types_are_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= RelationFlags::STRICT_NULL_CHECKS;
    }

    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Subtype,
        relation_policy::from_checker_flags_u16(flags),
        tsz_solver::relations::relation_queries::RelationContext::default(),
    )
    .is_related()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_common::Visibility;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::{FunctionShape, PropertyInfo, TypeParamInfo};

    fn function_returning(db: &TypeInterner, return_type: TypeId) -> TypeId {
        db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn property(db: &TypeInterner, name: &str, type_id: TypeId) -> PropertyInfo {
        PropertyInfo {
            name: db.intern_string(name),
            type_id,
            write_type: type_id,
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
        }
    }

    fn type_param_with_constraint(db: &TypeInterner, name: &str, constraint: TypeId) -> TypeId {
        db.type_param(TypeParamInfo {
            name: db.intern_string(name),
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: tsz_solver::TypeParamOrigin::User,
        })
    }

    #[test]
    fn assignment_reduction_preserves_top_like_initial_types() {
        let db = TypeInterner::new();

        assert_eq!(
            narrow_assignment(&db, None, TypeId::ANY, TypeId::NUMBER),
            TypeId::ANY
        );
        assert_eq!(
            narrow_assignment(&db, None, TypeId::UNKNOWN, TypeId::NUMBER),
            TypeId::UNKNOWN
        );
        assert_eq!(
            narrow_assignment(&db, None, TypeId::ERROR, TypeId::NUMBER),
            TypeId::ERROR
        );
    }

    #[test]
    fn assignment_reduction_keeps_non_union_initial_type() {
        let db = TypeInterner::new();

        assert_eq!(
            narrow_assignment(&db, None, TypeId::STRING, TypeId::NUMBER),
            TypeId::STRING
        );
    }

    #[test]
    fn assignment_reduction_uses_non_nullish_type_parameter_constraint_surface() {
        let db = TypeInterner::new();
        let nullable_string = db.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let type_param = type_param_with_constraint(&db, "T", nullable_string);
        let assigned = tsz_solver::narrowing::remove_nullish(&db, type_param);

        assert_eq!(
            narrow_assignment(&db, None, type_param, assigned),
            TypeId::STRING
        );
    }

    #[test]
    fn assignment_reduction_uses_non_nullish_indexed_access_constraint_surface() {
        let db = TypeInterner::new();
        let nullable_string = db.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let object = db.object(vec![property(&db, "x", nullable_string)]);
        let type_param = type_param_with_constraint(&db, "T", object);
        let indexed = db.index_access(type_param, db.literal_string("x"));
        let assigned = db.intersection(vec![indexed, db.object(Vec::new())]);

        assert_eq!(
            narrow_assignment(&db, None, indexed, assigned),
            TypeId::STRING
        );
    }

    #[test]
    fn assignment_reduction_filters_union_by_literal_source_assignability() {
        let db = TypeInterner::new();
        let initial = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let assigned = tsz_solver::type_queries::create_number_literal_type(&db, 42.0);

        assert_eq!(
            narrow_assignment(&db, None, initial, assigned),
            TypeId::NUMBER
        );
    }

    #[test]
    fn assignment_reduction_keeps_original_union_when_no_member_matches() {
        let db = TypeInterner::new();
        let initial = db.union(vec![TypeId::STRING, TypeId::BOOLEAN]);

        assert_eq!(
            narrow_assignment(&db, None, initial, TypeId::NUMBER),
            initial
        );
    }

    #[test]
    fn typeof_switch_domain_rejects_error_operands() {
        let db = TypeInterner::new();

        assert_eq!(typeof_switch_domain(&db, None, TypeId::ERROR), None);
    }

    #[test]
    fn typeof_switch_domain_returns_single_literal_for_primitive_operand() {
        let db = TypeInterner::new();

        assert_eq!(
            typeof_switch_domain(&db, None, TypeId::STRING),
            Some(db.literal_string("string"))
        );
    }

    #[test]
    fn typeof_switch_domain_returns_union_for_union_operand() {
        let db = TypeInterner::new();
        let operand = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        let Some(domain) = typeof_switch_domain(&db, None, operand) else {
            panic!("expected typeof domain for string | number");
        };
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&db.literal_string("string")));
        assert!(members.contains(&db.literal_string("number")));
    }

    #[test]
    fn narrow_by_typeof_result_routes_positive_and_negative_branches() {
        let db = TypeInterner::new();
        let source = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

        assert_eq!(
            narrow_by_typeof_result(&db, None, source, "string", true),
            TypeId::STRING
        );

        let negative = narrow_by_typeof_result(&db, None, source, "string", false);
        let members =
            union_members_for_type(&db, negative).unwrap_or_else(|| vec![negative].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::NUMBER));
        assert!(members.contains(&TypeId::BOOLEAN));
    }

    #[test]
    fn cases_exhaust_type_uses_exact_literal_union_coverage() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let second = db.literal_string("second");
        let third = db.literal_string("third");
        let switch_type = db.union(vec![first, second, third]);

        assert!(cases_exhaust_type(
            &db,
            None,
            switch_type,
            &[second, first, third],
        ));
        assert!(!cases_exhaust_type(&db, None, switch_type, &[first, third]));
    }

    #[test]
    fn enum_member_union_domain_keeps_plain_union_identity() {
        let db = TypeInterner::new();
        let union = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        assert_eq!(enum_member_union_domain(&db, union), union);
    }

    #[test]
    fn enum_member_union_domain_rewrites_only_enum_members() {
        let db = TypeInterner::new();
        let literal = db.literal_string("ready");
        let enum_member = db.enum_type(tsz_solver::def::DefId(701), literal);
        let union = db.union(vec![enum_member, TypeId::NUMBER]);

        let domain = enum_member_union_domain(&db, union);
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());

        assert_eq!(members.len(), 2);
        assert!(members.contains(&literal));
        assert!(members.contains(&TypeId::NUMBER));
        assert!(!members.contains(&enum_member));
    }

    #[test]
    fn has_enum_components_tracks_enum_identity() {
        let db = TypeInterner::new();
        let literal = db.literal_string("ready");
        let enum_member = db.enum_type(tsz_solver::def::DefId(702), literal);

        assert!(has_enum_components(&db, enum_member));
        assert!(!has_enum_components(&db, literal));
        assert!(!has_enum_components(&db, TypeId::NUMBER));
    }

    #[test]
    fn property_access_function_returns_never_recognizes_never_returning_property() {
        let db = TypeInterner::new();
        let never_fn = function_returning(&db, TypeId::NEVER);
        let void_fn = function_returning(&db, TypeId::VOID);
        let object = db.object(vec![
            property(&db, "bail", never_fn),
            property(&db, "continue", void_fn),
        ]);

        assert!(property_access_function_returns_never(&db, object, "bail"));
        assert!(!property_access_function_returns_never(
            &db, object, "continue"
        ));
        assert!(!property_access_function_returns_never(
            &db, object, "missing"
        ));
    }

    #[test]
    fn property_access_function_returns_never_is_structural_not_name_based() {
        let db = TypeInterner::new();
        let never_fn = function_returning(&db, TypeId::NEVER);
        let first_object = db.object(vec![property(&db, "abort", never_fn)]);
        let second_object = db.object(vec![property(&db, "halt", never_fn)]);
        let value_object = db.object(vec![property(&db, "abort", TypeId::NUMBER)]);

        assert!(property_access_function_returns_never(
            &db,
            first_object,
            "abort"
        ));
        assert!(property_access_function_returns_never(
            &db,
            second_object,
            "halt"
        ));
        assert!(!property_access_function_returns_never(
            &db,
            value_object,
            "abort"
        ));
    }

    #[test]
    fn nullish_coalescing_switch_domain_rejects_error_operands() {
        let db = TypeInterner::new();

        assert_eq!(
            nullish_coalescing_switch_domain(&db, TypeId::ERROR, TypeId::STRING),
            None
        );
        assert_eq!(
            nullish_coalescing_switch_domain(&db, TypeId::STRING, TypeId::ERROR),
            None
        );
    }

    #[test]
    fn nullish_coalescing_switch_domain_uses_right_when_left_is_nullish() {
        let db = TypeInterner::new();
        let left = db.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

        assert_eq!(
            nullish_coalescing_switch_domain(&db, left, TypeId::STRING),
            Some(TypeId::STRING)
        );
    }

    #[test]
    fn nullish_coalescing_switch_domain_unions_non_nullish_left_and_right() {
        let db = TypeInterner::new();
        let left = db.union(vec![TypeId::NULL, TypeId::NUMBER]);

        let Some(domain) = nullish_coalescing_switch_domain(&db, left, TypeId::STRING) else {
            panic!("expected switch domain for number | null ?? string");
        };
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::NUMBER));
        assert!(members.contains(&TypeId::STRING));
    }
}
