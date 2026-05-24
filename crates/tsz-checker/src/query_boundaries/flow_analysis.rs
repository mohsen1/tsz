use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};

pub(crate) use super::common::{
    LiteralValueKind, PredicateSignatureKind, TypeResolver,
    array_element_type as get_array_element_type, call_signatures_for_type,
    classify_for_literal_value, classify_for_predicate_signature, construct_signatures_for_type,
    contains_type_parameters, function_shape_for_type, is_keyof_type,
    is_literal_type_through_type_constraints, is_narrowing_literal, is_type_parameter_like,
    is_union_type, is_unit_type, is_unknown_narrowing_literal, object_shape_for_type,
    stringify_literal_type, tuple_elements as tuple_elements_for_type,
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

pub(crate) fn empty_object_type(db: &dyn QueryDatabase) -> TypeId {
    db.object(Vec::new())
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
        super::property_access::resolve_property_access(db, object_type, property_name),
        super::common::PropertyAccessResult::Success { type_id, .. }
            if function_return_type(db.as_type_database(), type_id) == Some(TypeId::NEVER)
    )
}

pub(crate) fn enum_member_domain(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::visitor::enum_components(db, type_id)
        .map(|(_def_id, members)| members)
        .unwrap_or(type_id)
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

    let mut narrowing = tsz_solver::narrowing::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_excluding_types(switch_type, case_types) == TypeId::NEVER
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
    if let Some(environment) = env {
        is_assignable_with_env(db, environment, source, member, true)
    } else {
        is_assignable_strict_null(db, source, member)
    }
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
    let assigned_matches_non_nullish_initial = if let Some(environment) = env {
        non_nullish_initial != initial_type
            && is_assignable_with_env(db, environment, assigned_type, non_nullish_initial, true)
            && is_assignable_with_env(db, environment, non_nullish_initial, assigned_type, true)
    } else {
        non_nullish_initial != initial_type
            && is_assignable_strict_null(db, assigned_type, non_nullish_initial)
            && is_assignable_strict_null(db, non_nullish_initial, assigned_type)
    };
    let assigned_has_reduced_constraint_surface = if let Some(environment) = env {
        is_assignable_with_env(db, environment, assigned_type, initial_type, true)
            && is_assignable_with_env(db, environment, assigned_type, reduced_constraint, true)
    } else {
        is_assignable_strict_null(db, assigned_type, initial_type)
            && is_assignable_strict_null(db, assigned_type, reduced_constraint)
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
        if !is_assignable(db, assigned_resolved, resolved_initial) {
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

pub(crate) fn is_assignable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    let _span = tracing::trace_span!("flow_assignable", src = source.0, tgt = target.0,).entered();

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

pub(crate) fn is_assignable_strict_null(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::relations::relation_queries::query_relation(
        db,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Assignable,
        tsz_solver::relations::relation_queries::RelationPolicy::from_flags(
            tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
        ),
        tsz_solver::relations::relation_queries::RelationContext::default(),
    )
    .is_related()
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
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Assignable,
        tsz_solver::relations::relation_queries::RelationPolicy::from_flags(flags),
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
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::relations::relation_queries::RelationKind::Subtype,
        tsz_solver::relations::relation_queries::RelationPolicy::from_flags(flags),
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
        }
    }

    fn type_param_with_constraint(db: &TypeInterner, name: &str, constraint: TypeId) -> TypeId {
        db.type_param(TypeParamInfo {
            name: db.intern_string(name),
            constraint: Some(constraint),
            default: None,
            is_const: false,
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
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain]);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&db.literal_string("string")));
        assert!(members.contains(&db.literal_string("number")));
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
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain]);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::NUMBER));
        assert!(members.contains(&TypeId::STRING));
    }
}
