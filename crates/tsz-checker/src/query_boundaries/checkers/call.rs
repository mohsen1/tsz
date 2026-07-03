use std::sync::Arc;

use tsz_common::Atom;
use tsz_solver::computation::{ContextualTypeContext, TypeSubstitution};
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::operations::{AssignabilityChecker, CallResult};
use tsz_solver::relations::subtype::{TypeEnvironment, TypeResolver};
use tsz_solver::{FunctionShape, ObjectShape, PropertyInfo, TupleElement, TypeId};

pub(crate) use super::super::common::array_element_type as array_element_type_for_type;
pub(crate) use super::super::common::is_type_parameter_like as is_type_parameter_type;
pub(crate) use super::super::common::lazy_def_id as lazy_def_id_for_type;
pub(crate) use super::super::common::tuple_elements as tuple_elements_for_type;

/// Positional offset of the first variable-length rest element in a tuple
/// spread, or `None` for a fully fixed-length tuple (or non-tuple). See
/// `tsz_solver::type_queries::tuple_variable_rest_offset`.
pub(crate) fn tuple_variable_rest_offset(db: &dyn TypeDatabase, type_id: TypeId) -> Option<usize> {
    tsz_solver::type_queries::tuple_variable_rest_offset(db, type_id)
}

/// Slice-taking form of [`tuple_variable_rest_offset`] for callers that already
/// hold the tuple's elements (avoids a second tuple lookup).
pub(crate) fn tuple_slice_variable_rest_offset(
    db: &dyn TypeDatabase,
    elements: &[TupleElement],
) -> Option<usize> {
    tsz_solver::type_queries::tuple_slice_variable_rest_offset(db, elements)
}

pub(crate) fn rest_array_element_type_for_type(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    array_element_type_for_type(db, type_id).or_else(|| {
        tsz_solver::type_queries::mutable_array_element_for_redeclaration(
            db,
            type_id,
            db.get_array_base_type(),
            Some(def_store),
        )
    })
}

pub(crate) fn rest_type_needs_aggregate_argument_check(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::rest_type_needs_aggregate_argument_check(db, type_id)
}

pub(crate) fn get_contextual_signature(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    tsz_solver::operations::get_contextual_signature_cached_with_compat_checker(db, type_id)
}

pub(crate) fn get_contextual_signature_for_arity(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    tsz_solver::operations::get_contextual_signature_for_arity_cached_with_compat_checker(
        db, type_id, arg_count,
    )
}

/// Get the call signature of a type, preferring a generic one.
///
/// Used by the checker's two-pass call path when overloaded callables mix
/// generic and non-generic signatures. `get_contextual_signature_for_arity`
/// intentionally returns `None` for that case to avoid unsafe contextual typing
/// of callbacks, but we still need to know whether there is an arity-compatible
/// generic signature so generic inference/sanitization can run.
pub(crate) fn get_call_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    let sigs = tsz_solver::type_queries::get_call_signatures(db, type_id)?;
    let signature_accepts_arg_count = |params: &[tsz_solver::ParamInfo], count: usize| {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            count >= required_count
        } else {
            count >= required_count && count <= params.len()
        }
    };
    let applicable: Vec<_> = sigs
        .iter()
        .filter(|s| signature_accepts_arg_count(&s.params, arg_count))
        .collect();

    let sig = if !applicable.is_empty() {
        applicable
            .iter()
            .find(|s| !s.type_params.is_empty())
            .copied()
            .or_else(|| applicable.first().copied())?
    } else {
        sigs.iter()
            .find(|s| !s.type_params.is_empty())
            .or_else(|| sigs.first())?
    };

    Some(FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: sig.is_method,
    })
}

pub(crate) fn get_function_parameter_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    tsz_solver::type_queries::get_function_parameter_types(db, type_id)
}

pub(crate) fn contains_index_access_with_type_parameter_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_index_access_with_type_parameter_object(db, type_id)
}

pub(crate) fn contains_index_access_with_variadic_tuple_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_index_access_with_variadic_tuple_object(db, type_id)
}

/// Whether `type_id`'s base constraint resolves to an array or tuple type
/// (type-parameter / `infer` constraint chains and deferred-conditional default
/// constraints are followed first). The caller must pass an env-evaluated type
/// so a generic-alias `Application` like `Parameters<F>` is already expanded to
/// its deferred-conditional body.
pub(crate) fn base_constraint_is_array_or_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::base_constraint_is_array_or_tuple(db, type_id)
}

pub(crate) fn contains_generic_indexed_access_surface_for_call(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    super::super::common::contains_generic_indexed_access_surface(db, type_id)
}

pub(crate) fn object_shape_for_call(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Arc<ObjectShape>> {
    super::super::common::object_shape_for_type(db, type_id)
}

pub(crate) const fn property_access_is_present_for_call(
    result: &super::super::common::PropertyAccessResult,
) -> bool {
    matches!(
        result,
        super::super::common::PropertyAccessResult::Success { .. }
            | super::super::common::PropertyAccessResult::PossiblyNullOrUndefined { .. }
    )
}

pub(crate) fn has_property_by_str_for_call(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> bool {
    super::super::common::has_property_by_str(db, type_id, name)
}

pub(crate) fn stable_call_recovery_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    // Identity leaf: return the signature's raw return type, type parameters
    // left intact (callers that want default type arguments use the variant
    // below).
    stable_call_recovery_return_type_impl(db, type_id, &|_db, return_type, _type_params| {
        return_type
    })
}

/// Like [`stable_call_recovery_return_type`], but instantiates the recovered
/// signature's *own* type parameters with their `default → constraint →
/// unknown` fallback.
///
/// When a generic call fails the argument-count check, tsc reports TS2554/TS2555
/// yet still produces a best-effort result type by substituting each signature
/// type parameter with its default type argument (`getInferredTypes` →
/// `getDefaultTypeArgumentType`). The plain recovery above returns the raw
/// return type, leaking the bare type parameter (e.g. `T`) into the result and
/// drawing a spurious `TS2322`/`TS2339` at the use site. Only the signature's
/// own parameters are resolved; an enclosing-scope parameter the return type
/// legitimately mentions stays abstract.
pub(crate) fn stable_call_recovery_return_type_with_default_type_args(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    stable_call_recovery_return_type_impl(
        db,
        type_id,
        &super::super::type_defaults::resolve_signature_default_type_args,
    )
}

/// Shared walk for [`stable_call_recovery_return_type`] and its
/// default-type-argument variant: peel a function / callable / intersection
/// type down to a single, agreed-upon recovery return type, applying `leaf` to
/// each signature's `(return_type, type_params)`. An intersection recovers only
/// when every callable member agrees on the (leaf-transformed) return type.
fn stable_call_recovery_return_type_impl(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    leaf: &impl Fn(&dyn TypeDatabase, TypeId, &[tsz_solver::TypeParamInfo]) -> TypeId,
) -> Option<TypeId> {
    if let Some(shape) = tsz_solver::type_queries::get_function_shape(db, type_id) {
        return Some(leaf(db, shape.return_type, &shape.type_params));
    }

    if let Some(shape) = tsz_solver::type_queries::get_callable_shape(db, type_id) {
        let first = shape.call_signatures.first()?;
        if shape
            .call_signatures
            .iter()
            .any(|sig| sig.return_type != first.return_type)
        {
            return None;
        }
        return Some(leaf(db, first.return_type, &first.type_params));
    }

    let members = tsz_solver::type_queries::get_intersection_members(db, type_id)?;
    let mut candidate = None;
    for member in members {
        let Some(return_type) = stable_call_recovery_return_type_impl(db, member, leaf) else {
            continue;
        };
        if let Some(existing) = candidate {
            if existing != return_type {
                return None;
            }
        } else {
            candidate = Some(return_type);
        }
    }
    candidate
}

/// Resolve an application base to the declaration identity used for generic
/// call inference.
///
/// Cross-file alias lowering can leave application bases in any of the same
/// reference forms that evaluator normalization accepts. Keep that solver-shape
/// decoding behind this boundary so call checking only asks for the declaration
/// identity it needs.
pub(crate) fn resolve_application_base_def_id<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    base: TypeId,
) -> Option<tsz_solver::DefId> {
    lazy_def_id_for_type(db, base)
        .or_else(|| {
            tsz_solver::type_queries::get_type_query_symbol_ref(db, base)
                .and_then(|sym_ref| resolver.symbol_to_def_id(sym_ref))
        })
        .or_else(|| {
            tsz_solver::visitor::unresolved_type_name_atom(db, base).and_then(|atom| {
                let name = db.resolve_atom(atom);
                resolver.resolve_unresolved_type_name(&name)
            })
        })
}

/// Get the construct signature of a type, preferring a generic one.
/// Used for two-pass inference in `new` expressions where the construct
/// signature may have type parameters that need to be inferred.
///
/// For overloaded constructors (e.g. `Map` with `new()` and `new<K,V>(entries?)`),
/// we prefer the generic signature so that `is_generic_new` is set correctly
/// and proper contextual types are provided to array/object literal arguments.
pub(crate) fn get_construct_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    let sigs = tsz_solver::type_queries::get_construct_signatures(db, type_id)?;
    let signature_accepts_arg_count = |params: &[tsz_solver::ParamInfo], count: usize| {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            count >= required_count
        } else {
            count >= required_count && count <= params.len()
        }
    };
    let applicable: Vec<_> = sigs
        .iter()
        .filter(|s| signature_accepts_arg_count(&s.params, arg_count))
        .collect();

    // Prefer generic signatures among arity-compatible candidates.
    let sig = if !applicable.is_empty() {
        applicable
            .iter()
            .find(|s| !s.type_params.is_empty())
            .copied()
            .or_else(|| applicable.first().copied())?
    } else {
        // Fallback to previous behavior when no signature matches arity.
        sigs.iter()
            .find(|s| !s.type_params.is_empty())
            .or_else(|| sigs.first())?
    };
    Some(FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: true,
        is_method: false,
    })
}

pub(crate) fn resolve_call<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
) -> tsz_solver::operations::CallWithCheckerResult {
    tsz_solver::operations::resolve_call_with_checker(
        db,
        checker,
        func_type,
        arg_types,
        force_bivariant_callbacks,
        contextual_type,
        actual_this_type,
    )
}

pub(crate) struct CallArgSourceOptions<'a> {
    pub(crate) force_bivariant_callbacks: bool,
    pub(crate) contextual_type: Option<TypeId>,
    pub(crate) actual_this_type: Option<TypeId>,
    pub(crate) arg_source_is_type_annotation: &'a [bool],
    pub(crate) arg_source_is_readonly_annotation: &'a [bool],
}

pub(crate) fn resolve_call_with_arg_sources<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    opts: &CallArgSourceOptions<'_>,
) -> tsz_solver::operations::CallWithCheckerResult {
    tsz_solver::operations::resolve_call_with_checker_and_arg_sources(
        db,
        checker,
        func_type,
        arg_types,
        &tsz_solver::operations::ResolveCallOptions {
            force_bivariant_callbacks: opts.force_bivariant_callbacks,
            contextual_type: opts.contextual_type,
            actual_this_type: opts.actual_this_type,
            arg_source_is_type_annotation: opts.arg_source_is_type_annotation,
            arg_source_is_readonly_annotation: opts.arg_source_is_readonly_annotation,
        },
    )
}

pub(crate) fn resolve_new<C: AssignabilityChecker>(
    db: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
) -> CallResult {
    tsz_solver::operations::resolve_new_with_checker(
        db,
        checker,
        type_id,
        arg_types,
        force_bivariant_callbacks,
        contextual_type,
    )
}

pub(crate) fn compute_contextual_types_with_context(
    db: &dyn QueryDatabase,
    ctx: &crate::context::CheckerContext<'_>,
    env: &tsz_solver::relations::subtype::TypeEnvironment,
    shape: &tsz_solver::FunctionShape,
    arg_types: &[TypeId],
    contextual_type: Option<TypeId>,
) -> TypeSubstitution {
    tsz_solver::operations::compute_contextual_types_with_compat_checker(
        db,
        env,
        shape,
        arg_types,
        contextual_type,
        |checker| ctx.configure_compat_checker(checker),
    )
}

pub(crate) fn expanded_this_type_from_application(
    db: &dyn TypeDatabase,
    env: &TypeEnvironment,
    type_id: TypeId,
    no_implicit_any: bool,
) -> Option<TypeId> {
    let app = tsz_solver::type_queries::get_type_application(db, type_id)?;
    let def_id = tsz_solver::type_queries::get_lazy_def_id(db, app.base)?;
    let body = env.resolve_lazy(def_id, db)?;
    let type_params = env.get_lazy_type_params(def_id).unwrap_or_default();
    let expanded = tsz_solver::computation::instantiate_generic(db, body, &type_params, &app.args);
    let expanded_ctx =
        ContextualTypeContext::with_expected_and_options(db, expanded, no_implicit_any);
    expanded_ctx.get_this_type_from_marker()
}

pub(crate) fn get_overload_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::CallSignature>> {
    tsz_solver::type_queries::data::get_overload_call_signatures(db, type_id)
}

pub(crate) fn is_valid_union_predicate(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::flow::is_valid_union_predicate(db, type_id)
}

pub(crate) fn extract_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::type_queries::flow::ExtractedPredicateSignature> {
    tsz_solver::type_queries::flow::extract_predicate_signature(db, type_id)
}

pub(crate) fn call_inference_partial_object_type(
    db: &dyn TypeDatabase,
    properties: Vec<(Atom, TypeId)>,
) -> TypeId {
    db.object_fresh(
        properties
            .into_iter()
            .map(|(name, type_id)| PropertyInfo::new(name, type_id))
            .collect(),
    )
}

pub(crate) fn call_inference_zero_arg_function_type(
    db: &dyn TypeDatabase,
    return_type: TypeId,
) -> TypeId {
    db.function(FunctionShape::new(Vec::new(), return_type))
}

pub(crate) fn call_inference_string_key_type(db: &dyn TypeDatabase, name: &str) -> TypeId {
    db.literal_string(name)
}

pub(crate) fn call_inference_tuple_type(db: &dyn TypeDatabase, elements: Vec<TypeId>) -> TypeId {
    db.tuple(
        elements
            .into_iter()
            .map(|type_id| TupleElement {
                type_id,
                optional: false,
                rest: false,
                name: None,
            })
            .collect(),
    )
}

pub(crate) fn call_result_correlated_union(db: &dyn TypeDatabase, members: Vec<TypeId>) -> TypeId {
    db.union(members)
}

pub(crate) fn call_result_optional_chain_return(
    db: &dyn TypeDatabase,
    return_type: TypeId,
) -> TypeId {
    db.union2(return_type, TypeId::UNDEFINED)
}

pub(crate) fn recursive_call_result_type(
    db: &dyn TypeDatabase,
    def_id: tsz_solver::DefId,
    type_args: Vec<TypeId>,
) -> TypeId {
    let lazy = db.lazy(def_id);
    if type_args.is_empty() {
        lazy
    } else {
        db.application(lazy, type_args)
    }
}
