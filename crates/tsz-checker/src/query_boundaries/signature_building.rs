//! Boundary-owned solver record construction for checker signature building.
//!
//! `checkers/signature_builder.rs` owns syntax traversal, scope updates, and
//! diagnostics. This module owns the raw solver records assembled from those
//! facts so checker code does not rebuild `CallSignature`, `ParamInfo`,
//! `TypeParamInfo`, or `TypePredicate` directly.

use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use tsz_common::Atom;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::{
    CallSignature, CallableShape, ParamInfo, TupleElement, TypeId, TypeParamInfo, TypeParamOrigin,
    TypePredicate, TypePredicateTarget,
};

pub(crate) const fn type_param_info(
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
    origin: TypeParamOrigin,
) -> TypeParamInfo {
    TypeParamInfo {
        name,
        constraint,
        default,
        is_const,
        origin,
    }
}

pub(crate) const fn user_type_param_info(
    name: Atom,
    constraint: Option<TypeId>,
    default: Option<TypeId>,
    is_const: bool,
) -> TypeParamInfo {
    type_param_info(name, constraint, default, is_const, TypeParamOrigin::User)
}

/// Construct the [`TypeParamOrigin::OverloadRenamed`] origin for an overload
/// signature type parameter that `overload_signature_for_inference` renamed to
/// a program-unique `__overload_sig_*` atom for name-keyed inference.
///
/// `source_origin` is the parameter's origin before this rename; when it is
/// itself an already-renamed origin the earliest declared display name is
/// preserved so a chain of renames never leaks the synthetic atom into a
/// diagnostic. `fallback_name` is the source parameter's own declared name,
/// used when it carried no prior display name.
pub(crate) fn overload_renamed_type_param_origin(
    source_origin: TypeParamOrigin,
    fallback_name: Atom,
) -> TypeParamOrigin {
    TypeParamOrigin::OverloadRenamed {
        display_name: source_origin
            .overload_rename_display_name()
            .unwrap_or(fallback_name),
    }
}

pub(crate) fn type_param(db: &dyn TypeDatabase, info: TypeParamInfo) -> TypeId {
    db.type_param(info)
}

pub(crate) fn user_type_param(db: &dyn TypeDatabase, info: TypeParamInfo) -> TypeId {
    type_param(db, info)
}

pub(crate) fn param_array_type(db: &dyn TypeDatabase, element: TypeId) -> TypeId {
    db.array(element)
}

pub(crate) fn optional_param_type_with_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    db.union2(type_id, TypeId::UNDEFINED)
}

pub(crate) const fn param_info(
    name: Option<Atom>,
    type_id: TypeId,
    optional: bool,
    rest: bool,
) -> ParamInfo {
    param_info_with_display(name, type_id, optional, rest, false)
}

/// Like [`param_info`], but records whether the parameter's optional marker is
/// display-suppressed. `suppress_display_optional` must be `true` only for a
/// bare, unannotated JS parameter (optional for weak call-arity but rendered
/// required by `tsc`); `false` for a real `?`, initializer, or JSDoc-optional
/// parameter. Arity and subtyping are unaffected — they keep reading `optional`.
pub(crate) const fn param_info_with_display(
    name: Option<Atom>,
    type_id: TypeId,
    optional: bool,
    rest: bool,
    suppress_display_optional: bool,
) -> ParamInfo {
    ParamInfo {
        name,
        type_id,
        optional,
        rest,
        suppress_display_optional,
    }
}

pub(crate) const fn call_signature(
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_method,
    }
}

pub(crate) const fn type_predicate(
    asserts: bool,
    target: TypePredicateTarget,
    type_id: Option<TypeId>,
    parameter_index: Option<usize>,
) -> TypePredicate {
    TypePredicate {
        asserts,
        target,
        type_id,
        parameter_index,
    }
}

pub(crate) fn instantiate_signature(
    db: &dyn QueryDatabase,
    sig: &CallSignature,
    type_args: &[TypeId],
) -> CallSignature {
    let substitution = TypeSubstitution::from_signature_args(db, &sig.type_params, type_args);
    call_signature(
        Vec::new(),
        instantiate_params(db, &sig.params, &substitution),
        sig.this_type
            .map(|type_id| instantiate_type(db, type_id, &substitution)),
        instantiate_type(db, sig.return_type, &substitution),
        instantiate_predicate(db, sig.type_predicate.as_ref(), &substitution),
        sig.is_method,
    )
}

pub(crate) fn partially_instantiate_signature(
    db: &dyn QueryDatabase,
    sig: &CallSignature,
    supplied_args: &[TypeId],
) -> CallSignature {
    debug_assert!(supplied_args.len() < sig.type_params.len());

    let substitution = TypeSubstitution::from_signature_args(
        db,
        &sig.type_params[..supplied_args.len()],
        supplied_args,
    );

    let remaining_type_params = sig.type_params[supplied_args.len()..]
        .iter()
        .map(|tp| {
            type_param_info(
                tp.name,
                tp.constraint
                    .map(|constraint| instantiate_type(db, constraint, &substitution)),
                tp.default
                    .map(|default| instantiate_type(db, default, &substitution)),
                tp.is_const,
                tp.origin,
            )
        })
        .collect();

    call_signature(
        remaining_type_params,
        instantiate_params(db, &sig.params, &substitution),
        sig.this_type
            .map(|type_id| instantiate_type(db, type_id, &substitution)),
        instantiate_type(db, sig.return_type, &substitution),
        instantiate_predicate(db, sig.type_predicate.as_ref(), &substitution),
        sig.is_method,
    )
}

fn instantiate_params(
    db: &dyn QueryDatabase,
    params: &[ParamInfo],
    substitution: &TypeSubstitution,
) -> Vec<ParamInfo> {
    params
        .iter()
        .map(|param| {
            param_info_with_display(
                param.name,
                instantiate_type(db, param.type_id, substitution),
                param.optional,
                param.rest,
                param.suppress_display_optional,
            )
        })
        .collect()
}

fn instantiate_predicate(
    db: &dyn QueryDatabase,
    predicate: Option<&TypePredicate>,
    substitution: &TypeSubstitution,
) -> Option<TypePredicate> {
    predicate.map(|predicate| {
        type_predicate(
            predicate.asserts,
            predicate.target,
            predicate
                .type_id
                .map(|type_id| instantiate_type(db, type_id, substitution)),
            predicate.parameter_index,
        )
    })
}

// ── Parameter-list transformation helpers ──

/// Replace parameter types at the given positions with a replacement type.
///
/// Used to sanitize binding-pattern parameters during generic inference:
/// destructured parameters contribute no inference candidates, so their
/// types are replaced with `unknown` to avoid polluting the constraint.
pub(crate) fn sanitize_params_at_positions(
    params: &[ParamInfo],
    positions: &[usize],
    replacement: TypeId,
) -> Vec<ParamInfo> {
    let mut result = params.to_vec();
    for &index in positions {
        if let Some(param) = result.get_mut(index) {
            param.type_id = replacement;
        }
    }
    result
}

/// Convert a slice of function parameters to tuple elements.
///
/// Each parameter's `type_id`, `optional`, `rest`, and `name` fields are
/// transferred directly.  Used when synthesizing a tuple type that mirrors
/// a parameter list (e.g. collecting remaining params for a rest argument).
pub(crate) fn params_to_tuple_elements(params: &[ParamInfo]) -> Vec<TupleElement> {
    params
        .iter()
        .map(|param| TupleElement {
            type_id: param.type_id,
            optional: param.optional,
            rest: param.rest,
            name: param.name,
        })
        .collect()
}

/// Sanitize binding-pattern parameters in a callable shape.
///
/// Like [`sanitize_params_at_positions`] but operates on a [`CallableShape`]:
/// each call signature's parameters at the given positions are replaced with
/// `replacement`.  Returns a new `CallableShape` ready for interning.
pub(crate) fn sanitize_callable_shape_binding_pattern_params(
    shape: &CallableShape,
    positions: &[usize],
    replacement: TypeId,
) -> CallableShape {
    let mut sanitized = shape.clone();
    sanitized.call_signatures = sanitized
        .call_signatures
        .iter()
        .map(|sig| {
            let mut new_sig = sig.clone();
            new_sig.params = sanitize_params_at_positions(&sig.params, positions, replacement);
            new_sig
        })
        .collect();
    sanitized
}
