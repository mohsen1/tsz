//! Signature-construction query boundary (issue #13022).
//!
//! Owns both directions of the checker's signature-shape traffic with the
//! solver:
//!
//! - reading construct signatures off arbitrary types
//!   ([`construct_signatures_for_type`]), and
//! - constructing signature-bearing solver types (functions and callables)
//!   from checker-assembled [`CallSignature`] data.
//!
//! Production checker modules must route function/callable interning and
//! `CallSignature` <-> `FunctionShape` conversion through these helpers
//! instead of hand-building `FunctionShape`/`CallableShape` literals at call
//! sites. The shape structs stay importable as read-only data (see the SAFE
//! import list in `architecture_contract_tests`); what is quarantined here is
//! shape *construction* and interning. `checkers/signature_builder.rs` remains
//! the one checker module that assembles `CallSignature` data from AST
//! signatures.

use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, ParamInfo, TypeId, TypePredicate,
};

pub(crate) fn construct_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    if let Some(signatures) = tsz_solver::type_queries::get_construct_signatures(db, type_id) {
        return Some(signatures);
    }
    let shape = tsz_solver::type_queries::get_function_shape(db, type_id)?;
    if !shape.is_constructor {
        return None;
    }
    Some(vec![CallSignature {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: shape.return_type,
        type_predicate: shape.type_predicate,
        is_method: shape.is_method,
    }])
}

pub(crate) fn has_construct_overloads(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    construct_signatures_for_type(db, type_id).is_some_and(|sigs| sigs.len() > 1)
}

/// tsc's `getInstantiatedConstructorsForTypeArguments(...)[0]` return type: the
/// arity-aware base type of a class extending a class-like constructor function,
/// keyed on the extends clause's type-argument count. See
/// [`tsz_solver::type_queries::get_base_construct_return_type`]. Returns `None`
/// when no construct signature is applicable to `type_arg_count`.
pub(crate) fn get_base_construct_return_type(
    db: &dyn TypeDatabase,
    shape_id: CallableShapeId,
    type_arg_count: usize,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_base_construct_return_type(db, shape_id, type_arg_count)
}

/// View one call/construct signature as a standalone, non-method
/// `FunctionShape`.
///
/// Signature-level method-ness stays on the `CallSignature` (callers that
/// round-trip restore it via [`call_signature_from_function_shape`]); the
/// produced shape is always `is_method: false`, matching every checker site
/// that previously expanded a signature inline.
pub(crate) fn function_shape_from_call_signature(
    sig: &CallSignature,
    is_constructor: bool,
) -> FunctionShape {
    FunctionShape {
        type_params: sig.type_params.clone(),
        params: sig.params.clone(),
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor,
        is_method: false,
    }
}

/// Collapse a `FunctionShape` back into a `CallSignature`.
///
/// Constructor-ness is positional in a `CallableShape` (which signature list
/// the result joins), so `shape.is_constructor` is dropped. `is_method` is
/// explicit because round-tripping callers restore the original signature's
/// flag rather than the shape's.
pub(crate) fn call_signature_from_function_shape(
    shape: FunctionShape,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params: shape.type_params,
        params: shape.params,
        this_type: shape.this_type,
        return_type: shape.return_type,
        type_predicate: shape.type_predicate,
        is_method,
    }
}

/// Intern a function type from an explicit, helper-built shape.
pub(crate) fn function_type_from_shape(db: &dyn TypeDatabase, shape: FunctionShape) -> TypeId {
    db.function(shape)
}

/// Intern the standalone function type for one signature (a constructor
/// function when `is_constructor` is set).
pub(crate) fn function_type_from_call_signature(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
    is_constructor: bool,
) -> TypeId {
    db.function(function_shape_from_call_signature(sig, is_constructor))
}

/// Intern a bare callable carrying only call signatures.
pub(crate) fn call_only_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        ..CallableShape::default()
    })
}

/// Intern a bare callable carrying only construct signatures.
pub(crate) fn construct_only_callable_type(
    db: &dyn TypeDatabase,
    construct_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        construct_signatures,
        ..CallableShape::default()
    })
}

/// Re-intern `base` with the given signature lists, preserving properties,
/// index signatures, nominal `symbol`, and abstractness.
pub(crate) fn callable_with_signatures_replaced(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures,
        properties: base.properties.clone(),
        string_index: base.string_index,
        number_index: base.number_index,
        symbol: base.symbol,
        is_abstract: base.is_abstract,
    })
}

/// Re-intern `base` with instantiated signature lists for type-argument
/// application.
///
/// Properties and index signatures carry over; the nominal `symbol` and
/// `is_abstract` markers are detached because the instantiation result is a
/// structural callable, not the original class-constructor identity.
pub(crate) fn instantiated_callable_from_base(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures,
        properties: base.properties.clone(),
        string_index: base.string_index,
        number_index: base.number_index,
        symbol: None,
        is_abstract: false,
    })
}

/// The component-type slots of a `FunctionShape`, used by
/// [`map_function_shape_types`] callers to apply per-slot policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FunctionShapeTypeSlot {
    /// A parameter's declared type.
    Param,
    /// The `this` parameter type.
    This,
    /// The return type.
    Return,
    /// The asserted type of a type predicate (`x is T`).
    PredicateTarget,
}

/// Rebuild `shape` with every component type passed through `map_type`,
/// preserving all non-type metadata (names, optionality, rest/method/
/// constructor flags, type parameters, predicate target).
///
/// Returns `None` when no component type changed, so callers can keep the
/// original interned `TypeId` for the unchanged case.
pub(crate) fn map_function_shape_types(
    shape: &FunctionShape,
    mut map_type: impl FnMut(FunctionShapeTypeSlot, TypeId) -> TypeId,
) -> Option<FunctionShape> {
    let mut params: Option<Vec<ParamInfo>> = None;
    for (index, param) in shape.params.iter().enumerate() {
        let mapped = map_type(FunctionShapeTypeSlot::Param, param.type_id);
        if let Some(params) = &mut params {
            params.push(ParamInfo {
                type_id: mapped,
                ..*param
            });
        } else if mapped != param.type_id {
            let mut changed_params = Vec::with_capacity(shape.params.len());
            changed_params.extend(shape.params[..index].iter().copied());
            changed_params.push(ParamInfo {
                type_id: mapped,
                ..*param
            });
            params = Some(changed_params);
        }
    }

    let mut changed = params.is_some();
    let this_type = shape.this_type.map(|this_type| {
        let mapped = map_type(FunctionShapeTypeSlot::This, this_type);
        changed |= mapped != this_type;
        mapped
    });
    let return_type = map_type(FunctionShapeTypeSlot::Return, shape.return_type);
    changed |= return_type != shape.return_type;
    let type_predicate = shape.type_predicate.map(|predicate| TypePredicate {
        type_id: predicate.type_id.map(|type_id| {
            let mapped = map_type(FunctionShapeTypeSlot::PredicateTarget, type_id);
            changed |= mapped != type_id;
            mapped
        }),
        ..predicate
    });

    changed.then(|| FunctionShape {
        type_params: shape.type_params.clone(),
        params: params.unwrap_or_else(|| shape.params.clone()),
        this_type,
        return_type,
        type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}
