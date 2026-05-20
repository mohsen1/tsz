use tsz_solver::{QueryDatabase, TypeDatabase, TypeId};

pub(crate) use super::common::{
    LiteralValueKind, PredicateSignatureKind, array_element_type as get_array_element_type,
    call_signatures_for_type, classify_for_literal_value, classify_for_predicate_signature,
    construct_signatures_for_type, contains_type_parameters, function_shape_for_type,
    is_keyof_type, is_narrowing_literal, is_type_parameter_like, is_unit_type,
    is_unknown_narrowing_literal, stringify_literal_type,
    tuple_elements as tuple_elements_for_type, union_members as union_members_for_type,
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

pub(crate) fn enum_member_domain(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::visitor::enum_components(db, type_id)
        .map(|(_def_id, members)| members)
        .unwrap_or(type_id)
}

pub(crate) fn type_has_typeof_result(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::TypeEnvironment>,
    type_id: TypeId,
    typeof_result: &str,
) -> bool {
    let mut narrowing = tsz_solver::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_by_typeof(type_id, typeof_result) != TypeId::NEVER
}

pub(crate) fn cases_exhaust_type(
    db: &dyn QueryDatabase,
    env: Option<&tsz_solver::TypeEnvironment>,
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

    let mut narrowing = tsz_solver::NarrowingContext::new(db);
    if let Some(environment) = env {
        narrowing = narrowing.with_resolver(environment);
    }
    narrowing.narrow_excluding_types(switch_type, case_types) == TypeId::NEVER
}

fn resolve_assignment_reduction_type(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::TypeEnvironment>,
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
    env: Option<&tsz_solver::TypeEnvironment>,
    source: TypeId,
    member: TypeId,
) -> bool {
    if let Some(environment) = env {
        is_assignable_with_env(db, environment, source, member, true)
    } else {
        is_assignable_strict_null(db, source, member)
    }
}

fn assigned_value_preserves_enum_identity(
    db: &dyn TypeDatabase,
    env: Option<&tsz_solver::TypeEnvironment>,
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
    env: Option<&tsz_solver::TypeEnvironment>,
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
    env: Option<&tsz_solver::TypeEnvironment>,
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
    tsz_solver::is_subtype_of(db, left, right) || tsz_solver::is_subtype_of(db, right, left)
}

pub(crate) fn is_assignable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    let _span = tracing::trace_span!("flow_assignable", src = source.0, tgt = target.0,).entered();

    tsz_solver::query_relation(
        db,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::default(),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

pub(crate) fn is_assignable_strict_null(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::query_relation(
        db,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::from_flags(
            tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
        ),
        tsz_solver::RelationContext::default(),
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
    env: &tsz_solver::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    types_are_subtype_with_env(db, env, left, right, strict_null_checks)
        || types_are_subtype_with_env(db, env, right, left, strict_null_checks)
}

pub(crate) fn is_assignable_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::RelationKind::Assignable,
        tsz_solver::RelationPolicy::from_flags(flags),
        tsz_solver::RelationContext::default(),
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
/// resolved structure but should not call `tsz_solver::evaluate_type()` directly.
pub(crate) fn evaluate_type_structure(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::evaluate_type(db, type_id)
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
    env: &tsz_solver::TypeEnvironment,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::ApplicationEvaluator::new(db, env).evaluate_or_original(type_id)
}

fn types_are_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::RelationKind::Subtype,
        tsz_solver::RelationPolicy::from_flags(flags),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TypeInterner;

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
}
