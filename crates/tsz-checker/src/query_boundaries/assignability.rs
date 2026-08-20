use tsz_common::Atom;
use tsz_solver::classes::inheritance::InheritanceGraph;
use tsz_solver::computation::{TypeSubstitution, evaluate_type, instantiate_type_cached};
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::relations::subtype::TypeResolver;
use tsz_solver::{
    ObjectShape, ParamInfo, PropertyInfo, SubtypeFailureReason, TypeId, TypeParamInfo,
};

use crate::context::CachedAssignabilityAnalysis;
use crate::state::CheckerState;
use tsz_solver::relations::relation_queries::{
    RelationContext, RelationKind as SolverRelationKind, RelationPolicy, RelationQueryInputs,
    query_assignability_with_failure_analysis,
};

use super::relation_policy;
pub(crate) use super::relation_request::RelationRequest;

pub(crate) use super::common::{
    contains_type_parameters, is_callable_type, is_generic_mapped_type, object_shape_for_type,
};

/// Build the `(policy, context)` pair shared by the assignability-failure query
/// paths from packed checker relation flags. Centralizing it keeps the gate
/// decision and the single-pass failure analysis on identical policy/context.
fn assignability_policy_and_context<'a>(
    db: &'a dyn QueryDatabase,
    inheritance_graph: &'a InheritanceGraph,
    flags: u16,
    sound_mode: bool,
    evaluation_session: Option<&'a tsz_solver::evaluation::session::EvaluationSession>,
) -> (RelationPolicy, RelationContext<'a>) {
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = RelationContext {
        query_db: Some(db),
        evaluation_session,
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    (policy, context)
}

pub(crate) fn are_types_structurally_identical<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    left: TypeId,
    right: TypeId,
) -> bool {
    tsz_solver::relations::subtype::are_types_structurally_identical(db, resolver, left, right)
}

/// The union member `tsc`'s `getBestMatchingType` elaborates a failed
/// object-to-union relation against: discriminant match, then same-generic-base
/// reference, then `findMostOverlappyType`'s key-overlap scan (ties to the
/// LAST member, in the union's written member order — `union_type_id` is the
/// union `members` was read from, so the solver can restore declaration order
/// when canonical interning reordered it). Used by per-property object-literal
/// elaboration when the indexed access over the full union is undefined (some
/// constituent lacks the key). `None` means no member is selected and the
/// drill-in is skipped.
pub(crate) fn union_target_best_elaboration_member<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    union_type_id: TypeId,
    members: &[TypeId],
) -> Option<TypeId> {
    tsz_solver::relations::subtype::union_target_best_elaboration_member(
        db,
        resolver,
        source,
        union_type_id,
        members,
    )
}

pub(crate) fn intersection_source_contains_target_member<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let Some(members) = super::common::intersection_members(db, source) else {
        return false;
    };

    members.iter().any(|&member| {
        member == target || are_types_structurally_identical(db, resolver, member, target)
    })
}

/// Check structural identity with an outer type-parameter scope visible to
/// both sides. Used by declaration-merge compatibility to compare type-param
/// constraints across declarations whose own `T`s resolve to distinct
/// `TypeId`s.
pub(crate) fn are_types_structurally_identical_in_param_scope<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    left: TypeId,
    right: TypeId,
    param_names: &[Atom],
) -> bool {
    tsz_solver::computation::are_types_structurally_identical_in_param_scope(
        db,
        resolver,
        left,
        right,
        param_names,
    )
}

pub(crate) fn recursive_heritage_property_types_conflict(
    checker: &mut CheckerState<'_>,
    member_type: TypeId,
    constraint_type: TypeId,
) -> bool {
    if are_types_structurally_identical(
        checker.ctx.types,
        &checker.ctx,
        member_type,
        constraint_type,
    ) {
        return false;
    }
    if checker
        .recursive_heritage_property_relation_outcome(member_type, constraint_type)
        .related
        || checker
            .recursive_heritage_property_relation_outcome(constraint_type, member_type)
            .related
    {
        return false;
    }
    true
}

/// Return the element type when `type_id` is a mutable `Array<T>` form used for
/// redeclaration identity.
pub(crate) fn mutable_array_element_for_redeclaration(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    def_store: Option<&tsz_solver::def::DefinitionStore>,
) -> Option<TypeId> {
    tsz_solver::type_queries::mutable_array_element_for_redeclaration(
        db,
        type_id,
        db.get_array_base_type(),
        def_store,
    )
}

pub(crate) fn remapped_mapped_type_has_no_outer_type_params(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::remapped_mapped_type_has_no_outer_type_params(db, type_id)
}

pub(crate) fn optional_mapped_type_adds_implicit_undefined<R: TypeResolver>(
    db: &dyn QueryDatabase,
    _resolver: &R,
    type_id: TypeId,
) -> bool {
    let type_db = db.as_type_database();
    if type_db.get_display_alias(type_id).is_some()
        || tsz_solver::type_queries::get_type_application(type_db, type_id).is_some()
    {
        return false;
    }
    tsz_solver::type_queries::optional_mapped_type_adds_implicit_undefined(type_db, type_id)
}

/// Classify target surfaces where checker diagnostics should preserve the
/// outer assignment instead of elaborating through an unresolved projection.
pub(crate) fn target_prefers_outer_assignment_diagnostic<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    candidates: &[TypeId],
) -> bool {
    let type_db = db.as_type_database();
    let expanded = alias_application_surface_candidates(db, resolver, candidates);

    expanded.into_iter().any(|candidate| {
        candidate != TypeId::ERROR
            && candidate != TypeId::ANY
            && (candidate_has_deferred_evaluation_surface(db, resolver, candidate)
                || super::common::is_generic_application_with_type_params(type_db, candidate))
    })
}

/// Whether a target carries a **deferred** evaluation surface — a generic
/// indexed access, a generic mapped type/application, or a conditional that
/// still mentions type parameters. These are the surfaces where a property type
/// cannot be resolved to a concrete member, so `tsc` keeps the outer
/// whole-object diagnostic (with its nested relation-reason chain) instead of
/// drilling a fresh object literal per-property. Distinguished from a *plain*
/// generic application like `A<T>` (a simple interface/object instantiation),
/// whose members resolve concretely and which `tsc`'s `elaborateObjectLiteral`
/// does drill.
pub(crate) fn target_has_deferred_evaluation_surface<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    candidates: &[TypeId],
) -> bool {
    let expanded = alias_application_surface_candidates(db, resolver, candidates);
    expanded
        .into_iter()
        .any(|candidate| candidate_has_deferred_evaluation_surface(db, resolver, candidate))
}

fn candidate_has_deferred_evaluation_surface<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    candidate: TypeId,
) -> bool {
    if candidate == TypeId::ERROR || candidate == TypeId::ANY {
        return false;
    }
    let type_db = db.as_type_database();
    super::common::contains_generic_indexed_access_surface(type_db, candidate)
        || super::common::is_generic_mapped_type(type_db, candidate)
        || super::common::is_generic_mapped_application(db, resolver, candidate)
        || (super::common::contains_conditional_type(type_db, candidate)
            && super::common::contains_type_parameters(type_db, candidate))
}

/// Expand checker-facing type surfaces through display aliases and instantiated
/// alias applications while keeping type-shape details behind this boundary.
pub(crate) fn alias_application_surface_candidates<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    candidates: &[TypeId],
) -> Vec<TypeId> {
    let type_db = db.as_type_database();
    let mut expanded = candidates.to_vec();

    for candidate in candidates.iter().copied() {
        if let Some(alias) = type_db.get_display_alias(candidate) {
            expanded.push(alias);
        }
    }

    let alias_candidates = expanded.clone();
    for candidate in alias_candidates {
        if let Some(instantiated) = instantiate_alias_candidate(db, resolver, candidate) {
            expanded.push(instantiated);
            expanded.push(evaluate_type(type_db, instantiated));
        }
    }

    expanded
}

/// Return an instantiated homomorphic mapped target that projects over `source`.
///
/// This preserves deferred targets such as `{ [P in keyof S]?: S[P] }` through
/// checker-side assignability preparation so the solver relation can decide the
/// mapped comparison structurally.
pub(crate) fn homomorphic_mapped_projection_target<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    _source: TypeId,
    target: TypeId,
) -> Option<TypeId> {
    let type_db = db.as_type_database();
    let candidate = if tsz_solver::type_queries::get_mapped_type(type_db, target).is_some() {
        target
    } else {
        let app = tsz_solver::type_queries::get_type_application(type_db, target)?;
        let def_id = tsz_solver::type_queries::get_lazy_def_id(type_db, app.base)?;
        let type_params = resolver.get_lazy_type_params(def_id)?;
        if type_params.is_empty() {
            return None;
        }
        let body = resolver.resolve_lazy(def_id, type_db)?;
        let substitution = TypeSubstitution::from_args(type_db, &type_params, &app.args);
        tsz_solver::computation::instantiate_type_cached(type_db, Some(db), body, &substitution)
    };

    let mapped = tsz_solver::type_queries::get_mapped_type(type_db, candidate)?;
    if mapped.name_type.is_some()
        || mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove)
    {
        return None;
    }

    let mapped_source = tsz_solver::keyof_inner_type(type_db, mapped.constraint)?;
    let (template_obj, template_idx) = tsz_solver::index_access_parts(type_db, mapped.template)?;
    let idx_param = tsz_solver::type_param_info(type_db, template_idx)?;
    if idx_param.name == mapped.type_param.name && template_obj == mapped_source {
        Some(candidate)
    } else {
        None
    }
}

/// Returns callable union members that a contextually typed function expression
/// may be checked against directly.
///
/// This models tsc's applicability path for shapes such as
/// `ComponentClass<P> | StatelessComponent<P>`: the returned function is allowed
/// to satisfy the callable member even when generic mapped props expand to a
/// different but equivalent structural form during contextual typing.
pub(crate) fn contextual_function_callable_union_members(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> Vec<TypeId> {
    if !tsz_solver::contains_type_parameters(db, source)
        || !tsz_solver::contains_type_parameters(db, target)
    {
        return Vec::new();
    }

    let source_is_callable =
        tsz_solver::type_queries::get_callable_shape_for_type(db, source).is_some_and(|shape| {
            !shape.call_signatures.is_empty() && shape.construct_signatures.is_empty()
        }) || tsz_solver::type_queries::get_function_shape(db, source)
            .is_some_and(|shape| !shape.is_constructor);
    if !source_is_callable {
        return Vec::new();
    }

    let mut callable_members = Vec::new();
    let evaluated_target = evaluate_type(db, target);
    for candidate in [target, evaluated_target] {
        if let Some(members) = tsz_solver::type_queries::get_union_members(db, candidate) {
            for &member in members.iter() {
                let evaluated_member = evaluate_type(db, member);
                let callable_member =
                    tsz_solver::type_queries::get_callable_shape_for_type(db, member)
                        .map(|shape| (member, shape))
                        .or_else(|| {
                            (evaluated_member != member)
                                .then(|| {
                                    tsz_solver::type_queries::get_callable_shape_for_type(
                                        db,
                                        evaluated_member,
                                    )
                                    .map(|shape| (evaluated_member, shape))
                                })
                                .flatten()
                        });
                if let Some((callable_member, shape)) = callable_member
                    && !shape.call_signatures.is_empty()
                    && !callable_members.contains(&callable_member)
                {
                    callable_members.push(callable_member);
                }
            }
        }
    }
    callable_members
}

pub(crate) fn callable_pair_contains_type_parameters(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    fn is_callable_or_function(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        super::common::callable_shape_for_type(db, type_id).is_some()
            || super::common::function_shape_for_type(db, type_id).is_some()
    }

    is_callable_or_function(db, source)
        && is_callable_or_function(db, target)
        && super::common::contains_type_parameters(db, source)
        && super::common::contains_type_parameters(db, target)
}

/// Return true when the source is an intersection that directly contains the
/// target as one of its constituents.
///
/// This preserves the solver intersection law `(A & B) <: A` before checker
/// assignability preparation evaluates a large alias intersection into an
/// expansive representation.
pub(crate) fn intersection_source_has_target_constituent(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    tsz_solver::type_queries::get_intersection_members(db, source)
        .is_some_and(|members| members.contains(&target))
}

pub(crate) fn contextual_callable_member_failure_is_generic_parameter_drift(
    db: &dyn TypeDatabase,
    failure: Option<&super::relation_types::RelationFailure>,
) -> bool {
    let Some(super::relation_types::RelationFailure::ParameterTypeMismatch {
        param_index: 0,
        source_param,
        target_param,
        inner: Some(inner),
    }) = failure
    else {
        return false;
    };

    if !tsz_solver::contains_type_parameters(db, *source_param)
        || !tsz_solver::contains_type_parameters(db, *target_param)
    {
        return false;
    }

    matches!(
        inner.as_ref(),
        super::relation_types::RelationFailure::TypeMismatch {
            source_type,
            target_type,
        } if *source_type == *target_param && *target_type == *source_param
    )
}

pub(crate) fn contextual_callable_member_has_unclassified_generic_parameter_drift(
    db: &dyn TypeDatabase,
    source: TypeId,
    member: TypeId,
) -> bool {
    let source_params = tsz_solver::type_queries::get_callable_shape_for_type(db, source)
        .and_then(|shape| shape.call_signatures.first().map(|sig| sig.params.clone()))
        .or_else(|| {
            tsz_solver::type_queries::get_function_shape(db, source)
                .map(|shape| shape.params.clone())
        });
    let Some(member_shape) = tsz_solver::type_queries::get_callable_shape_for_type(db, member)
    else {
        return false;
    };
    let Some(source_params) = source_params else {
        return false;
    };
    let Some(member_sig) = member_shape.call_signatures.first() else {
        return false;
    };
    let Some(source_param) = source_params.first() else {
        return false;
    };
    let Some(member_param) = member_sig.params.first() else {
        return false;
    };

    source_params.len() <= member_sig.params.len()
        && tsz_solver::contains_type_parameters(db, source_param.type_id)
        && tsz_solver::contains_type_parameters(db, member_param.type_id)
}

fn direct_type_param_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    tsz_solver::type_queries::get_type_parameter_info(db, type_id).map(|info| info.name)
}

fn signature_uses_only_naked_type_params(
    db: &dyn TypeDatabase,
    params: &[ParamInfo],
    return_type: TypeId,
    type_params: &[TypeParamInfo],
) -> bool {
    if params.is_empty() {
        return false;
    }
    let names: Vec<_> = type_params.iter().map(|tp| tp.name).collect();
    let params_are_naked = params.iter().all(|param| {
        direct_type_param_name(db, param.type_id).is_some_and(|name| names.contains(&name))
    });
    params_are_naked
        && direct_type_param_name(db, return_type).is_some_and(|name| names.contains(&name))
}

fn generic_signature_constraints_and_naked_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(Vec<Option<TypeId>>, bool)> {
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id) {
        let constraints = shape.type_params.iter().map(|tp| tp.constraint).collect();
        let naked = signature_uses_only_naked_type_params(
            db,
            &shape.params,
            shape.return_type,
            &shape.type_params,
        );
        return Some((constraints, naked));
    }
    let shape = tsz_solver::type_queries::get_callable_shape_for_type(db, type_id)?;
    let sig = shape.call_signatures.first()?;
    let constraints = sig.type_params.iter().map(|tp| tp.constraint).collect();
    let naked =
        signature_uses_only_naked_type_params(db, &sig.params, sig.return_type, &sig.type_params);
    Some((constraints, naked))
}

/// Return `true` when `actual` is a generic function whose every type parameter
/// has a constraint, while `expected` is a same-arity generic function whose
/// type parameters are all unconstrained.
///
/// In that shape the mismatch is a constraint-strictness incompatibility that
/// outer inference cannot repair, so contextual-call diagnostics must not be
/// deferred away.
pub(crate) fn generic_arg_constraint_mismatch_is_structural(
    db: &dyn TypeDatabase,
    actual: TypeId,
    expected: TypeId,
) -> bool {
    let Some((source_constraints, source_is_naked)) =
        generic_signature_constraints_and_naked_shape(db, actual)
    else {
        return false;
    };
    if source_constraints.is_empty()
        || !source_is_naked
        || !source_constraints.iter().all(Option::is_some)
        || !source_constraints
            .iter()
            .filter_map(|constraint| *constraint)
            .any(|constraint| constraint != TypeId::UNKNOWN)
    {
        return false;
    }

    let Some((target_constraints, target_is_naked)) =
        generic_signature_constraints_and_naked_shape(db, expected)
    else {
        return false;
    };
    target_is_naked
        && target_constraints.len() == source_constraints.len()
        && target_constraints.iter().all(Option::is_none)
}

pub(crate) fn homomorphic_mapped_source_assignable_to_target<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> bool {
    let type_db = db.as_type_database();
    let Some(source_candidate) = instantiate_mapped_candidate(db, resolver, source) else {
        return false;
    };
    let Some(source_mapped) = tsz_solver::type_queries::get_mapped_type(type_db, source_candidate)
    else {
        return false;
    };

    if source_mapped.name_type.is_some()
        || source_mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add)
    {
        return false;
    }

    let Some(source_base) = tsz_solver::keyof_inner_type(type_db, source_mapped.constraint) else {
        return false;
    };

    let source_key = type_db.type_param(source_mapped.type_param);
    let source_indexed_value = type_db.index_access(source_base, source_key);
    let source_indexed_value_eval = db.evaluate_type(source_indexed_value);
    let source_template = source_mapped.template;
    let source_template_eval = db.evaluate_type(source_template);
    let source_template_expanded = instantiate_alias_candidate(db, resolver, source_template);
    let source_template_expanded_eval =
        source_template_expanded.map(|expanded| db.evaluate_type(expanded));

    if let Some(target_candidate) = instantiate_mapped_candidate(db, resolver, target)
        && let Some(target_mapped) =
            tsz_solver::type_queries::get_mapped_type(type_db, target_candidate)
    {
        let Some(target_base) = tsz_solver::keyof_inner_type(type_db, target_mapped.constraint)
        else {
            return false;
        };
        if !homomorphic_sources_match(type_db, source_base, target_base) {
            return false;
        }
        let mut target_template = target_mapped.template;
        let target_key_substitution =
            TypeSubstitution::single(target_mapped.type_param.name, source_key);
        target_template =
            instantiate_type_cached(type_db, Some(db), target_template, &target_key_substitution);
        if target_mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add)
            && source_mapped.optional_modifier != Some(tsz_solver::MappedModifier::Add)
        {
            target_template = type_db.union2(target_template, TypeId::UNDEFINED);
        }
        let target_template_eval = db.evaluate_type(target_template);
        return mapped_templates_structurally_assignable(
            type_db,
            source_template,
            source_template_eval,
            target_template,
            target_template_eval,
        ) || source_template_expanded.is_some_and(|expanded| {
            mapped_templates_structurally_assignable(
                type_db,
                expanded,
                source_template_expanded_eval.unwrap_or(expanded),
                target_template,
                target_template_eval,
            )
        });
    }

    homomorphic_sources_match(type_db, source_base, target)
        && mapped_templates_structurally_assignable(
            type_db,
            source_template,
            source_template_eval,
            source_indexed_value,
            source_indexed_value_eval,
        )
        || (homomorphic_sources_match(type_db, source_base, target)
            && source_template_expanded.is_some_and(|expanded| {
                mapped_templates_structurally_assignable(
                    type_db,
                    expanded,
                    source_template_expanded_eval.unwrap_or(expanded),
                    source_indexed_value,
                    source_indexed_value_eval,
                )
            }))
}

fn instantiate_alias_candidate<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<TypeId> {
    let type_db = db.as_type_database();
    let app = tsz_solver::type_queries::get_type_application(type_db, type_id)?;
    let def_id = tsz_solver::type_queries::get_lazy_def_id(type_db, app.base)?;
    let type_params = resolver.get_lazy_type_params(def_id)?;
    if type_params.is_empty() {
        return None;
    }
    let body = resolver.resolve_lazy(def_id, type_db)?;
    let substitution = TypeSubstitution::from_args(type_db, &type_params, &app.args);
    Some(instantiate_type_cached(
        type_db,
        Some(db),
        body,
        &substitution,
    ))
}

fn instantiate_mapped_candidate<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<TypeId> {
    let type_db = db.as_type_database();
    if tsz_solver::type_queries::get_mapped_type(type_db, type_id).is_some() {
        return Some(type_id);
    }
    instantiate_alias_candidate(db, resolver, type_id)
}

fn homomorphic_sources_match(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> bool {
    if left == right {
        return true;
    }
    if let (Some((left_obj, left_idx)), Some((right_obj, right_idx))) = (
        tsz_solver::index_access_parts(db, left),
        tsz_solver::index_access_parts(db, right),
    ) {
        return homomorphic_sources_match(db, left_obj, right_obj)
            && homomorphic_sources_match(db, left_idx, right_idx);
    }
    false
}

fn mapped_template_structurally_assignable(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    if source == target {
        return true;
    }
    if source == TypeId::NEVER {
        return true;
    }
    if let Some(cond) = tsz_solver::type_queries::get_conditional_type(db, source) {
        return mapped_template_structurally_assignable(db, cond.true_type, target)
            && mapped_template_structurally_assignable(db, cond.false_type, target);
    }
    if let (Some((source_obj, source_idx)), Some((target_obj, target_idx))) = (
        tsz_solver::index_access_parts(db, source),
        tsz_solver::index_access_parts(db, target),
    ) {
        return homomorphic_sources_match(db, source_obj, target_obj)
            && homomorphic_sources_match(db, source_idx, target_idx);
    }
    if let Some(target_members_id) = tsz_solver::union_list_id(db, target) {
        return db
            .type_list(target_members_id)
            .iter()
            .any(|member| mapped_template_structurally_assignable(db, source, *member));
    }
    if let Some(source_members_id) = tsz_solver::intersection_list_id(db, source) {
        return db
            .type_list(source_members_id)
            .iter()
            .any(|member| mapped_template_structurally_assignable(db, *member, target));
    }
    false
}

fn mapped_templates_structurally_assignable(
    db: &dyn TypeDatabase,
    source: TypeId,
    source_eval: TypeId,
    target: TypeId,
    target_eval: TypeId,
) -> bool {
    mapped_template_structurally_assignable(db, source, target)
        || mapped_template_structurally_assignable(db, source_eval, target)
        || mapped_template_structurally_assignable(db, source, target_eval)
        || mapped_template_structurally_assignable(db, source_eval, target_eval)
}

pub(crate) use tsz_solver::type_queries::{
    AssignabilityEvalKind, ExcessPropertiesKind, get_allowed_keys, get_keyof_type,
    get_string_literal_value, get_union_members, is_keyof_type, is_type_parameter_like,
    keyof_object_properties, map_compound_members,
};

/// Submodules keep this file under its LOC ceiling while the assignability
/// boundary still owns the helpers: relation cache-key construction, the
/// overload subtype pass, indexed-access normalization shape probes, and the
/// non-default relation-kind query variants.
mod cache_key;
mod construction;
mod final_relation;
mod overload_subtype_pass;
mod relation_kind_variants;
mod shape;
pub(crate) use cache_key::{
    RelationFlags, assignability_cache_key, assignability_cache_key_for_policy,
    checker_final_assignability_cache_key, subtype_cache_key,
};
pub(crate) use construction::{
    assignability_array_type, assignability_contextual_pattern_property,
    assignability_empty_object_type, assignability_function_with_return_type,
    assignability_index_access_type, assignability_intersection_type,
    assignability_namespace_export_property, assignability_noinfer_type, assignability_object_type,
    assignability_readonly_type, assignability_resolved_property,
    assignability_resolved_tuple_element, assignability_tuple_element, assignability_tuple_type,
    assignability_union_preserve_members, assignability_union_type,
};
pub(crate) use final_relation::cached_final_assignability;
pub(crate) use overload_subtype_pass::{
    cached_overload_subtype_pass_assignability,
    cached_overload_subtype_pass_provisional_rest_union_assignability,
    cached_provisional_rest_union_assignability,
};
pub(crate) use relation_kind_variants::{
    cached_bivariant_assignability_with_resolver, is_redeclaration_identical_with_resolver,
    is_subtype_with_resolver,
};
pub(crate) use shape::*;

pub(crate) fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    tsz_solver::type_queries::classify_for_assignability_eval(db, type_id)
}

pub(crate) fn is_relation_cacheable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    !tsz_solver::type_queries::contains_infer_types_db(db, source)
        && !tsz_solver::type_queries::contains_infer_types_db(db, target)
        // ThisType results are context-dependent (resolver's this_type_stack).
        // Caching them would poison the checker-level cache with results computed
        // outside of any class body context, causing incorrect False results later.
        && !tsz_solver::contains_this_type(db, source)
        && !tsz_solver::contains_this_type(db, target)
}

pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

pub(crate) fn contains_free_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_free_infer_types(db, type_id)
}

pub(crate) fn contains_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_any_type(db, type_id)
}

pub(crate) fn has_recursive_type_parameter_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::visitor::type_param_info(db, type_id).is_some_and(|info| {
        info.constraint.is_some_and(|constraint| {
            tsz_solver::visitor::contains_type_parameter_named_shallow(db, constraint, info.name)
        })
    })
}

/// Detect the `S[T1]` vs `S[T2]` mismatch pattern where T1/T2 are
/// distinct type parameters and the object halves share a TypeId.
/// Returns the failure reason that elaborates the TS2322 + TS5075
/// chain, or `None` for any other pair.
///
/// Operates on the unevaluated pair so callers can short-circuit
/// before `prepare_assignability_inputs` collapses both halves to
/// the same evaluated shape. The same-object check is intentionally
/// strict (TypeId equality) here; deeper unification is owned by the
/// solver-side recognizer to keep this boundary helper free of a fresh
/// subtype context.
pub(crate) fn index_access_pair_distinct_type_param_keys_failure_reason(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    source: TypeId,
    target: TypeId,
) -> Option<SubtypeFailureReason> {
    let (s_obj, s_idx) = tsz_solver::index_access_parts(db, source)?;
    let (t_obj, t_idx) = tsz_solver::index_access_parts(db, target)?;
    let same_object = s_obj == t_obj
        || def_store
            .find_def_for_type(s_obj)
            .zip(def_store.find_def_for_type(t_obj))
            .is_some_and(|(source_def, target_def)| source_def == target_def);
    if !same_object {
        return None;
    }
    tsz_solver::type_param_info(db, s_idx)?;
    let t_param = tsz_solver::type_param_info(db, t_idx)?;
    let same_identity = s_idx == t_idx
        || def_store
            .find_def_for_type(s_idx)
            .zip(def_store.find_def_for_type(t_idx))
            .is_some_and(|(source_def, target_def)| source_def == target_def);
    if same_identity {
        return None;
    }
    Some(SubtypeFailureReason::IndexAccessTypeParameterMismatch {
        source_param: s_idx,
        target_param: t_idx,
        target_constraint: t_param.constraint,
    })
}

pub(crate) fn has_deferred_conditional_member(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::has_deferred_conditional_member(db, type_id)
}

pub(crate) fn is_any_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_any_type(db, type_id)
}

pub(crate) fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    tsz_solver::type_queries::classify_for_excess_properties(db, type_id)
}

/// Perform a fresh subtype check that bypasses the `QueryDatabase` cache.
/// This is needed after generic inference when the cache may contain stale
/// entries from intermediate inference steps.
pub(crate) fn is_fresh_subtype_of(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    tsz_solver::relations::subtype::is_subtype_of(db, source, target)
}

pub(crate) fn get_function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_return_type(db, type_id)
}

pub(crate) fn strip_function_type_predicate(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::strip_function_type_predicate(db, type_id)
}

pub(crate) fn rewrite_function_error_slots_to_any(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::rewrite_function_error_slots_to_any(db, type_id)
}

pub(crate) fn replace_function_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    new_return: TypeId,
) -> TypeId {
    tsz_solver::type_queries::replace_function_return_type(db, type_id, new_return)
}

pub(crate) fn erase_function_type_params_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::erase_function_type_params_to_any(db, type_id)
}

pub(crate) fn are_types_overlapping_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags: u16 = 0;
    if strict_null_checks {
        flags |= RelationFlags::STRICT_NULL_CHECKS;
    }

    let policy = relation_policy::from_checker_flags_u16(flags);
    tsz_solver::relations::relation_queries::query_relation_with_resolver(
        db,
        env,
        left,
        right,
        tsz_solver::relations::relation_queries::RelationKind::Overlap,
        policy,
        tsz_solver::relations::relation_queries::RelationContext::default(),
    )
    .is_related()
}

pub(crate) fn is_assignable_with_overrides<R: tsz_solver::relations::subtype::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(inputs.flags)
        .with_strict_subtype_checking(inputs.sound_mode)
        .with_strict_any_propagation(inputs.sound_mode);
    overload_subtype_pass::is_assignable_with_policy_and_overrides(inputs, policy, overrides)
}

pub(crate) fn cached_assignability_with_overrides<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(inputs.flags)
        .with_strict_subtype_checking(inputs.sound_mode)
        .with_strict_any_propagation(inputs.sound_mode);
    overload_subtype_pass::cached_assignability_with_policy_and_overrides(inputs, policy, overrides)
}

/// Like `is_assignable_with_overrides` but skips weak type checks (TS2559).
///
/// This matches tsc's `isTypeAssignableTo` behavior, which does NOT
/// include the weak type check. Used by the flow narrowing guard.
pub(crate) fn is_assignable_no_weak_checks<R: tsz_solver::relations::subtype::TypeResolver>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> bool {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
        evaluation_session,
    } = *inputs;
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode)
        .with_skip_weak_type_checks(true);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        evaluation_session,
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::query_relation_with_overrides(
        tsz_solver::relations::relation_queries::RelationQueryInputs {
            interner: db.as_type_database(),
            resolver,
            source,
            target,
            kind: tsz_solver::relations::relation_queries::RelationKind::Assignable,
            policy,
            context,
            overrides,
        },
    )
    .is_related()
}

#[derive(Clone, Copy)]
pub(crate) struct AssignabilityQueryInputs<'a, R: tsz_solver::relations::subtype::TypeResolver> {
    pub db: &'a dyn QueryDatabase,
    pub resolver: &'a R,
    pub source: TypeId,
    pub target: TypeId,
    pub flags: u16,
    pub inheritance_graph: &'a InheritanceGraph,
    pub sound_mode: bool,
    /// The checker's shared `EvaluationSession`, so relation probes entered
    /// through this boundary accrue cross-evaluator guard state (conditional
    /// depth, cross-eval active set, query memo) on the same session as
    /// evaluation-entered probes instead of the thread-local fallback
    /// session (issue #14346 split-brain).
    pub evaluation_session: Option<&'a tsz_solver::evaluation::session::EvaluationSession>,
}

pub(crate) struct AssignabilityFailureAnalysis {
    pub weak_union_violation: bool,
    pub failure_reason: Option<SubtypeFailureReason>,
}

pub(crate) struct AssignabilityGateResult {
    pub related: bool,
    pub analysis: Option<AssignabilityFailureAnalysis>,
}

/// Like [`execute_relation`], `precomputed` replays a prior reason-collecting
/// pass over the same `(source, target, flags, sound_mode)` memo key instead
/// of re-running the solver, and the second return value is the raw analysis
/// of a freshly executed collecting pass for the caller to memoize.
pub(crate) fn check_assignable_gate_with_overrides<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
    collect_failure_analysis: bool,
    precomputed: Option<&CachedAssignabilityAnalysis>,
) -> (AssignabilityGateResult, Option<CachedAssignabilityAnalysis>) {
    // When the caller only needs the boolean, take the cheap single-decision path.
    if !collect_failure_analysis {
        let related = is_assignable_with_overrides(inputs, overrides).is_related();
        return (
            AssignabilityGateResult {
                related,
                analysis: None,
            },
            None,
        );
    }

    // A memoized pass over the same key replays without re-running the solver.
    if let Some(cached) = precomputed {
        if !cached.related {
            tsz_common::perf_counters::record_relation_failure_memo_hit();
        }
        let analysis = (!cached.related).then(|| AssignabilityFailureAnalysis {
            weak_union_violation: cached.weak_union_violation,
            failure_reason: cached.failure_reason.clone(),
        });
        return (
            AssignabilityGateResult {
                related: cached.related,
                analysis,
            },
            None,
        );
    }

    // Otherwise decide and (on failure) explain in a single configured-checker
    // pass so the structured analysis observes exactly the same relation policy,
    // overrides, and cached sub-results as the decision and cannot contradict it.
    let (policy, context) = assignability_policy_and_context(
        inputs.db,
        inputs.inheritance_graph,
        inputs.flags,
        inputs.sound_mode,
        inputs.evaluation_session,
    );
    let outcome = query_assignability_with_failure_analysis(RelationQueryInputs {
        interner: inputs.db.as_type_database(),
        resolver: inputs.resolver,
        source: inputs.source,
        target: inputs.target,
        kind: SolverRelationKind::Assignable,
        policy,
        context,
        overrides,
    });
    let related = outcome.result.is_related();
    let capture = CachedAssignabilityAnalysis {
        related,
        depth_exceeded: outcome.result.depth_exceeded(),
        iteration_exceeded: outcome.result.iteration_exceeded(),
        weak_union_violation: outcome
            .analysis
            .as_ref()
            .is_some_and(|a| a.weak_union_violation),
        failure_reason: outcome
            .analysis
            .as_ref()
            .and_then(|a| a.failure_reason.clone()),
    };
    let analysis = outcome.analysis.map(|a| AssignabilityFailureAnalysis {
        weak_union_violation: a.weak_union_violation,
        failure_reason: a.failure_reason,
    });
    (AssignabilityGateResult { related, analysis }, Some(capture))
}

// ---------------------------------------------------------------------------
// RelationOutcome: structured result from executing a RelationRequest
// ---------------------------------------------------------------------------

/// Structured outcome from executing a `RelationRequest` through the
/// canonical boundary.
///
/// Combines the relation result, structured failure classification, and
/// weak-union violation detection into a single response so that callers
/// do not need to issue multiple solver round-trips for the same logical
/// question.
#[derive(Debug)]
pub(crate) struct RelationOutcome {
    /// Whether the relation holds (source is assignable to target).
    pub related: bool,
    /// Stack-depth limit (nesting) was exceeded → TS2321 "Excessive stack depth".
    pub depth_exceeded: bool,
    /// Iteration-count budget was exhausted → TS2859 "Excessive complexity".
    pub iteration_exceeded: bool,
    /// Structured failure classification when `related` is false.
    /// Converted from the solver's `SubtypeFailureReason`.
    pub failure: Option<super::relation_types::RelationFailure>,
    /// Whether the failure is a weak-union violation (TS2559).
    /// When true, the checker should emit excess-property diagnostics
    /// instead of the standard assignability error.
    pub weak_union_violation: bool,
    /// Structured property-level classification for object compatibility.
    /// Populated when the request has `source_is_fresh` set and the source/target
    /// are object types. Provides the canonical excess/missing/incompatible lists
    /// so the checker does not need to re-derive them.
    pub property_classification: Option<super::relation_types::PropertyClassification>,
}

/// Execute a `RelationRequest` through the canonical boundary.
///
/// This is the single authoritative entry point for relation queries that need
/// structured failure information: it runs the solver assignability check,
/// collects a structured failure reason when not related, and detects
/// weak-union violations.
///
/// The boundary translates request policy into solver flags and the
/// property-classification work the diagnostic layer can consume. Existing
/// caller-side EPC/missing-property emission still owns source anchors and
/// diagnostic wording, but the request decides whether property classification
/// is part of the relation outcome.
///
/// `precomputed` replays a prior reason-collecting solver pass over the same
/// memo key (issue #13243): the solver is not re-run; the outcome is rebuilt
/// from the captured analysis. The second return value is the fresh solver
/// analysis to memoize — `None` on the decision-only path and on memo replays.
/// The checker-environment slice of a relation execution: everything the
/// boundary needs besides the request itself. Mirrors
/// `AssignabilityQueryInputs` for the request-driven `execute_relation` path.
pub(crate) struct RelationExecutionEnv<'a, R: tsz_solver::relations::subtype::TypeResolver> {
    pub db: &'a dyn QueryDatabase,
    pub resolver: &'a R,
    pub flags: u16,
    pub inheritance_graph: &'a InheritanceGraph,
    pub sound_mode: bool,
    /// See `AssignabilityQueryInputs::evaluation_session` (issue #14346).
    pub evaluation_session: Option<&'a tsz_solver::evaluation::session::EvaluationSession>,
}

pub(crate) fn execute_relation<R: tsz_solver::relations::subtype::TypeResolver>(
    request: &RelationRequest,
    env: &RelationExecutionEnv<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
    precomputed: Option<&CachedAssignabilityAnalysis>,
) -> (RelationOutcome, Option<CachedAssignabilityAnalysis>) {
    let RelationExecutionEnv {
        db,
        resolver,
        flags,
        inheritance_graph,
        sound_mode,
        evaluation_session,
    } = *env;
    let _span = tracing::debug_span!(
        "execute_relation",
        src = request.source.0,
        tgt = request.target.0,
        kind = ?request.kind,
    )
    .entered();
    debug_assert!(
        precomputed.is_none() || request.failure_memo_key(flags, sound_mode).is_some(),
        "precomputed analysis passed for a memo-ineligible request"
    );

    // BivariantCallbacks treats callback parameter types bivariantly by stripping
    // strict-function-types. The decision and the failure reason both run under
    // this policy so they cannot diverge.
    let (solver_kind, solver_flags) = request.solver_relation_policy(flags);

    let (related, depth_exceeded, iteration_exceeded, analysis, capture) =
        if let Some(cached) = precomputed {
            if !cached.related {
                tsz_common::perf_counters::record_relation_failure_memo_hit();
            }
            let analysis = (!cached.related).then(|| {
                tsz_solver::relations::relation_queries::AssignabilityFailureAnalysis {
                    weak_union_violation: cached.weak_union_violation,
                    failure_reason: cached.failure_reason.clone(),
                }
            });
            (
                cached.related,
                cached.depth_exceeded,
                cached.iteration_exceeded,
                analysis,
                None,
            )
        } else {
            // Decide the relation and, on failure, capture the structured reason
            // from the SAME configured checker (single pass). This is the
            // canonical fix for the boundary's previous double evaluation, where
            // the pass/fail decision and the failure reason were computed by two
            // independently configured checkers and could contradict each other
            // (or drop the reason entirely when a checker override forced the
            // failure).
            let (policy, context) = assignability_policy_and_context(
                db,
                inheritance_graph,
                solver_flags,
                sound_mode,
                evaluation_session,
            );
            // The overload subtype pass rides on the typed `any`-propagation mode
            // (not the packed `u16` flags, which are saturated). The mode
            // participates in `RelationPolicy::cache_config`, so pass-1 results
            // cannot share relation cache slots with the default assignable
            // relation.
            let policy = if request.overload_subtype_pass {
                policy.with_any_propagation_mode(
                    tsz_solver::relations::subtype::AnyPropagationMode::AnySourceNotRelated,
                )
            } else {
                policy
            };
            let inputs = RelationQueryInputs {
                interner: db.as_type_database(),
                resolver,
                source: request.source,
                target: request.target,
                kind: solver_kind,
                policy,
                context,
                overrides,
            };
            let solver_outcome = if request.decision_only {
                // The caller reads only the pass/fail bit: run the identical
                // decision pass but skip the failure-reason walk on failure.
                tsz_solver::relations::relation_queries::query_assignability_decision_only(inputs)
            } else {
                query_assignability_with_failure_analysis(inputs)
            };
            let related = solver_outcome.result.is_related();
            let depth_exceeded = solver_outcome.result.depth_exceeded();
            let iteration_exceeded = solver_outcome.result.iteration_exceeded();
            let capture = (!request.decision_only).then(|| CachedAssignabilityAnalysis {
                related,
                depth_exceeded,
                iteration_exceeded,
                weak_union_violation: solver_outcome
                    .analysis
                    .as_ref()
                    .is_some_and(|a| a.weak_union_violation),
                failure_reason: solver_outcome
                    .analysis
                    .as_ref()
                    .and_then(|a| a.failure_reason.clone()),
            });
            (
                related,
                depth_exceeded,
                iteration_exceeded,
                solver_outcome.analysis,
                capture,
            )
        };

    if related {
        return (
            RelationOutcome {
                related: true,
                depth_exceeded,
                iteration_exceeded,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            },
            capture,
        );
    }

    let (weak_union_violation, failure) = match analysis {
        Some(a) => (
            a.weak_union_violation,
            a.failure_reason
                .map(super::relation_types::RelationFailure::from_solver_reason),
        ),
        None => (false, None),
    };

    let property_classification =
        if !request.decision_only && request.requires_property_classification() {
            classify_object_properties(db.as_type_database(), request.source, request.target)
        } else {
            None
        };

    // Suppress ExcessProperty failure when the target has structural features
    // that make EPC inapplicable.
    let failure =
        suppress_excess_property_failure_if_needed(failure, db.as_type_database(), request.target);

    (
        RelationOutcome {
            related: false,
            depth_exceeded,
            iteration_exceeded,
            failure,
            weak_union_violation,
            property_classification,
        },
        capture,
    )
}

/// Suppress an `ExcessProperty` failure reason when the target's structure
/// makes EPC inapplicable:
/// 1. Target contains a deferred conditional type → structural mismatch, not EPC.
/// 2. Target intersection has primitive or type-parameter members → EPC skipped.
///
/// This is the canonical boundary-level policy, replacing the checker-local
/// re-analysis that was in `analyze_assignability_failure`.
fn suppress_excess_property_failure_if_needed(
    failure: Option<super::relation_types::RelationFailure>,
    db: &dyn TypeDatabase,
    target: TypeId,
) -> Option<super::relation_types::RelationFailure> {
    let is_excess = matches!(
        &failure,
        Some(super::relation_types::RelationFailure::ExcessProperty { .. })
    );
    if !is_excess {
        return failure;
    }

    if target_suppresses_excess_property_failure(db, [target], |member| member) {
        return None;
    }

    failure
}

/// Apply the boundary-owned `ExcessProperty` suppression policy to raw failure
/// analysis collected outside [`execute_relation`].
///
/// Some legacy callers still use [`check_assignable_gate_with_overrides`] for
/// structured failure analysis. They may need checker-specific type evaluation
/// before inspecting intersection members, so the boundary owns the decision
/// while callers provide the member normalization callback.
pub(crate) fn suppress_raw_excess_property_failure_if_needed<I, F>(
    mut analysis: AssignabilityFailureAnalysis,
    db: &dyn TypeDatabase,
    target_candidates: I,
    normalize_member: F,
) -> AssignabilityFailureAnalysis
where
    I: IntoIterator<Item = TypeId>,
    F: FnMut(TypeId) -> TypeId,
{
    if matches!(
        &analysis.failure_reason,
        Some(SubtypeFailureReason::ExcessProperty { .. })
    ) && target_suppresses_excess_property_failure(db, target_candidates, normalize_member)
    {
        analysis.failure_reason = None;
    }

    analysis
}

fn target_suppresses_excess_property_failure<I, F>(
    db: &dyn TypeDatabase,
    target_candidates: I,
    mut normalize_member: F,
) -> bool
where
    I: IntoIterator<Item = TypeId>,
    F: FnMut(TypeId) -> TypeId,
{
    use super::common::is_type_parameter_like;

    for target in target_candidates {
        if tsz_solver::has_deferred_conditional_member(db, target) {
            return true;
        }

        if let Some(members) = tsz_solver::type_queries::data::get_intersection_members(db, target)
            && members.iter().any(|member| {
                let member = normalize_member(*member);
                tsz_solver::is_primitive_type(db, member) || is_type_parameter_like(db, member)
            })
        {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// classify_object_properties: canonical property-level classification
// ---------------------------------------------------------------------------

/// Classify properties between source and target object types.
///
/// This is the authoritative boundary function for property-level analysis.
/// It replaces the duplicated property enumeration logic that was previously
/// spread across `state_checking/property.rs` (excess checking) and
/// `assignability_diagnostics.rs` (`should_skip_weak_union_error`).
///
/// Returns `None` when the source or target is not an object type with
/// extractable properties.
pub(crate) fn classify_object_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> Option<super::relation_types::PropertyClassification> {
    use super::common::{intersection_members, is_type_parameter_like, union_members};
    use super::relation_types::PropertyClassification;

    tsz_common::perf_counters::record_property_classification_call();

    // Cannot classify if target is a type parameter.
    if is_type_parameter_like(db, target) {
        return Some(PropertyClassification {
            target_is_type_parameter: true,
            ..Default::default()
        });
    }

    let source_shape =
        crate::query_boundaries::common::get_merged_object_shape_for_type(db, source)?;
    let source_props = source_shape.properties.as_slice();

    if source_props.is_empty() {
        return Some(PropertyClassification::default());
    }

    let mut classification = PropertyClassification::default();

    // Check for index signatures, empty object targets, and special shapes.
    if let Some(target_shape) =
        crate::query_boundaries::common::get_merged_object_shape_for_type(db, target)
    {
        if shape_has_non_symbol_index_signature(&target_shape) {
            classification.target_has_index_signature = true;
        }
        if target_shape.properties.is_empty()
            && target_shape.string_index.is_none()
            && target_shape.number_index.is_none()
        {
            classification.target_is_empty_object = true;
        }
        if target_shape.number_index.is_some() {
            classification.target_has_index_signature = true;
            classification.target_has_number_index = true;
        }
        if is_global_object_or_function_shape(db, &target_shape) {
            classification.target_is_global_object_or_function = true;
        }
    } else if let Some(members) = union_members(db, target) {
        // For unions, check if any member has index signatures or is special.
        for &member in &members {
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                if shape_has_non_symbol_index_signature(&shape) {
                    classification.target_has_index_signature = true;
                }
                if shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
                {
                    classification.target_is_empty_object = true;
                }
                if shape.number_index.is_some() {
                    classification.target_has_number_index = true;
                }
                if is_global_object_or_function_shape(db, &shape) {
                    classification.target_is_global_object_or_function = true;
                }
            }
            if member == TypeId::OBJECT {
                classification.target_is_empty_object = true;
            }
        }
    } else if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if is_type_parameter_like(db, member) {
                classification.target_is_type_parameter = true;
            }
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                if shape_has_non_symbol_index_signature(&shape) {
                    classification.target_has_index_signature = true;
                }
                if shape.number_index.is_some() {
                    classification.target_has_number_index = true;
                }
            }
        }
    }

    // Collect target properties for presence and compatibility checking.
    let target_props = collect_target_property_index(db, target);

    // Classify each source property and check compatibility of matching ones.
    let mut all_matching_compatible = true;
    let mut matching_props = Vec::new();

    for source_prop in source_props {
        if let Some(target_prop_type) = target_props.matching_type_for(db, source_prop) {
            // Property exists in target — check type compatibility.
            // Account for optional properties: target `prop?: T` accepts `T | undefined`.
            let effective_target_type = target_prop_type;
            if !tsz_solver::relations::subtype::is_subtype_of(
                db,
                source_prop.type_id,
                effective_target_type,
            ) {
                all_matching_compatible = false;
                classification.incompatible_properties.push((
                    source_prop.name,
                    source_prop.type_id,
                    effective_target_type,
                ));
            } else {
                matching_props.push(source_prop.clone());
            }
        } else if !target_index_signature_accepts_source_property(db, target, source_prop)
            && !classification.target_is_empty_object
            && !classification.target_is_global_object_or_function
            && !classification.target_is_type_parameter
        {
            classification.excess_properties.push(source_prop.name);
        }
    }

    classification.all_matching_compatible = all_matching_compatible;

    // When there are excess properties and all matching ones are compatible,
    // check if a trimmed source (only matching properties) would be assignable.
    // This catches structural incompatibilities beyond property names (e.g.,
    // deferred conditional types in the target).
    if !classification.excess_properties.is_empty() && all_matching_compatible {
        let trimmed_source = db.object(matching_props);
        classification.trimmed_source_assignable =
            tsz_solver::relations::subtype::is_subtype_of(db, trimmed_source, target);
    }

    Some(classification)
}

fn shape_has_non_symbol_index_signature(shape: &ObjectShape) -> bool {
    shape
        .string_index
        .as_ref()
        .is_some_and(|idx| idx.key_type != TypeId::SYMBOL)
        || shape.number_index.is_some()
}

pub(crate) fn target_index_signature_accepts_source_property(
    db: &dyn TypeDatabase,
    target: TypeId,
    source_prop: &PropertyInfo,
) -> bool {
    use super::common::{intersection_members, union_members};

    if let Some(shape) =
        crate::query_boundaries::common::get_merged_object_shape_for_type(db, target)
    {
        return shape_index_signature_accepts_property(db, &shape, source_prop);
    }

    if let Some(members) = union_members(db, target) {
        return members.iter().any(|&member| {
            target_index_signature_accepts_source_property(db, member, source_prop)
        });
    }

    if let Some(members) = intersection_members(db, target) {
        return members.iter().any(|&member| {
            target_index_signature_accepts_source_property(db, member, source_prop)
        });
    }

    false
}

fn shape_index_signature_accepts_property(
    db: &dyn TypeDatabase,
    shape: &ObjectShape,
    source_prop: &PropertyInfo,
) -> bool {
    let string_index = shape.string_index_signature();
    let symbol_index = shape.symbol_index_signature();

    if source_prop.is_symbol_named {
        return symbol_index.is_some()
            || string_index.is_some_and(|idx| index_signature_key_accepts_symbol(db, idx));
    }

    let name = db.resolve_atom_ref(source_prop.name);
    if shape.number_index.is_some() && tsz_solver::utils::is_numeric_literal_name(name.as_ref()) {
        return true;
    }

    string_index.is_some()
}

fn index_signature_key_accepts_symbol(
    db: &dyn TypeDatabase,
    index: &tsz_solver::IndexSignature,
) -> bool {
    index_signature_key_type_accepts_symbol(db, index.key_type)
}

pub(crate) fn index_signature_key_type_accepts_symbol(
    db: &dyn TypeDatabase,
    key_type: TypeId,
) -> bool {
    key_type == TypeId::SYMBOL
        || tsz_solver::relations::subtype::is_subtype_of(db, TypeId::SYMBOL, key_type)
}

/// Property-name index for assignability failure classification.
///
/// The normal path keys by `Atom`, which is the stable property-name identity
/// available on `PropertyInfo`. The fallback string scan keeps behavior intact
/// if a source property arrives with a name identity that cannot be matched by
/// atom alone.
#[derive(Default)]
struct TargetPropertyIndex {
    by_atom: std::collections::HashMap<Atom, TypeId>,
    fallback_order: Vec<(Atom, TypeId)>,
}

impl TargetPropertyIndex {
    fn insert(&mut self, prop: &PropertyInfo) {
        self.by_atom.entry(prop.name).or_insert(prop.type_id);
        self.fallback_order.push((prop.name, prop.type_id));
    }

    fn matching_type_for(
        &self,
        db: &dyn TypeDatabase,
        source_prop: &PropertyInfo,
    ) -> Option<TypeId> {
        if let Some(target_type) = self.by_atom.get(&source_prop.name).copied() {
            return Some(target_type);
        }

        tsz_common::perf_counters::record_property_classification_string_fallback_source_lookup();
        self.matching_type_by_resolved_name(db, source_prop.name)
    }

    fn matching_type_by_resolved_name(
        &self,
        db: &dyn TypeDatabase,
        source_name: Atom,
    ) -> Option<TypeId> {
        let source_text = db.resolve_atom_ref(source_name);
        self.fallback_order
            .iter()
            .find_map(|(target_name, target_type)| {
                tsz_common::perf_counters::record_property_classification_string_fallback_target_name();
                let target_text = db.resolve_atom_ref(*target_name);
                if target_text.as_ref() == source_text.as_ref() {
                    tsz_common::perf_counters::record_property_classification_string_fallback_target_type();
                    Some(*target_type)
                } else {
                    None
                }
            })
    }
}

/// Collect all property names and their types from a target type.
///
/// For unions, uses the type from the first member that has the property.
/// For intersections, uses the type from the first member that has the property.
fn collect_target_property_index(db: &dyn TypeDatabase, target: TypeId) -> TargetPropertyIndex {
    use super::common::{intersection_members, union_members};
    let mut props = TargetPropertyIndex::default();

    if let Some(shape) =
        crate::query_boundaries::common::get_merged_object_shape_for_type(db, target)
    {
        for prop in shape.properties.iter() {
            props.insert(prop);
        }
    }

    if let Some(members) = union_members(db, target) {
        for &member in &members {
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                for prop in shape.properties.iter() {
                    props.insert(prop);
                }
            }
        }
    }

    if let Some(members) = intersection_members(db, target) {
        for &member in members.iter() {
            if let Some(shape) =
                crate::query_boundaries::common::get_merged_object_shape_for_type(db, member)
            {
                for prop in shape.properties.iter() {
                    props.insert(prop);
                }
            }
        }
    }

    props
}

/// Check if an object shape represents the global Object or Function interface.
///
/// These types have only inherited method properties and should suppress
/// excess property checking. This is the canonical boundary-level check,
/// replacing the checker-local `is_global_object_or_function_shape`.
///
/// Public boundary variant for checker code that needs to check a pre-resolved shape.
pub(crate) fn is_global_object_or_function_shape_boundary(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    is_global_object_or_function_shape(db, shape)
}

fn is_global_object_or_function_shape(
    db: &dyn TypeDatabase,
    shape: &tsz_solver::ObjectShape,
) -> bool {
    // Delegate to the canonical shared sniff (issue #13090), which requires the
    // *full* distinguishing member set under a property-count cap. The previous
    // local copy instead asked whether *every* property name was drawn from a
    // merged Object/Function prototype list — a subset test, so a user type
    // whose sole property is a prototype-member name (`{ length: number }`,
    // `{ toString: number }`) was misclassified as the global interface and had
    // its fresh-literal excess-property check (TS2353) suppressed (#14849).
    tsz_solver::type_queries::object_shape_matches_global_object_interface(db, shape)
        || tsz_solver::type_queries::object_shape_matches_global_function_interface(db, shape)
}

/// Explain a same-generic application failure (`C<A..>` vs `C<B..>`) via the
/// differing type arguments, mirroring tsc.
///
/// Must be called on the **raw** (unevaluated) operands so the application
/// structure survives; returns `None` unless the failure reliably reduces to a
/// concrete type argument, in which case the structural analysis should run.
pub(crate) fn same_generic_application_failure_reason<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    db: &dyn TypeDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    resolver: &R,
    source: TypeId,
    target: TypeId,
) -> Option<SubtypeFailureReason> {
    tsz_solver::relations::relation_queries::explain_same_generic_application_with_resolver(
        db,
        resolver,
        source,
        target,
        |checker| ctx.configure_compat_checker(checker),
    )
}

/// Variance-aware Application-to-Application assignability check.
///
/// When both source and target are Applications with the same base type,
/// uses computed variance to check arguments without structural expansion.
/// Must be called BEFORE types are evaluated/expanded.
///
/// Returns `Some(true/false)` if conclusive, `None` to fall through.
pub(crate) fn check_application_variance_assignability<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
) -> Option<bool> {
    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        flags,
        inheritance_graph,
        sound_mode,
        evaluation_session,
    } = *inputs;
    let policy = relation_policy::from_checker_flags_u16(flags)
        .with_strict_subtype_checking(sound_mode)
        .with_strict_any_propagation(sound_mode);
    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(db),
        evaluation_session,
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::check_application_variance(
        db.as_type_database(),
        resolver,
        Some(db),
        source,
        target,
        policy,
        context,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::DefId;
    use tsz_solver::{IndexSignature, MappedModifier, MappedType, TypeParamInfo};

    #[test]
    fn target_property_index_uses_first_atom_match() {
        let db = TypeInterner::new();
        let name = db.intern_string("renamed");
        let mut index = TargetPropertyIndex::default();

        index.insert(&PropertyInfo::new(name, TypeId::STRING));
        index.insert(&PropertyInfo::new(name, TypeId::NUMBER));

        let source = PropertyInfo::new(name, TypeId::BOOLEAN);
        assert_eq!(index.matching_type_for(&db, &source), Some(TypeId::STRING));
    }

    #[test]
    fn target_property_index_keeps_string_fallback() {
        let db = TypeInterner::new();
        let name = db.intern_string("fallbackName");
        let mut index = TargetPropertyIndex::default();

        index.fallback_order.push((name, TypeId::NUMBER));

        assert_eq!(
            index.matching_type_by_resolved_name(&db, name),
            Some(TypeId::NUMBER)
        );
    }

    #[test]
    fn symbol_named_source_property_is_accepted_by_property_key_index_signature() {
        let db = TypeInterner::new();
        let property_key = db.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: property_key,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert!(classification.excess_properties.is_empty());
    }

    #[test]
    fn symbol_named_source_property_is_excess_for_plain_string_index_signature() {
        let db = TypeInterner::new();
        let target = db.object_with_index(ObjectShape {
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            ..ObjectShape::default()
        });
        let mut source_prop =
            PropertyInfo::new(db.intern_string("[Symbol.iterator]"), TypeId::STRING);
        source_prop.is_symbol_named = true;
        let source = db.object(vec![source_prop]);

        let classification =
            classify_object_properties(&db, source, target).expect("object classification");

        assert_eq!(classification.excess_properties.len(), 1);
    }

    #[test]
    fn optional_mapped_implicit_undefined_is_structural_across_param_names() {
        let db = TypeInterner::new();

        for name in ["K", "Prop"] {
            let mapped = db.mapped(MappedType {
                type_param: TypeParamInfo::simple(db.intern_string(name)),
                constraint: TypeId::STRING,
                template: TypeId::NUMBER,
                name_type: None,
                readonly_modifier: None,
                optional_modifier: Some(MappedModifier::Add),
            });

            assert!(optional_mapped_type_adds_implicit_undefined(
                &db, &db, mapped
            ));
        }
    }

    #[test]
    fn optional_mapped_implicit_undefined_rejects_existing_undefined_template() {
        let db = TypeInterner::new();
        let template = db.union2(TypeId::NUMBER, TypeId::UNDEFINED);
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }

    #[test]
    fn optional_mapped_implicit_undefined_respects_display_alias_surface() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string("K")),
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });
        let alias = db.application(db.lazy(DefId(1)), vec![TypeId::STRING]);
        db.store_display_alias(mapped, alias);

        assert!(!optional_mapped_type_adds_implicit_undefined(
            &db, &db, mapped
        ));
    }
}
