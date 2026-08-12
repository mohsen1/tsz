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

use tsz_binder::SymbolId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::{
    CallSignature, CallableShape, CallableShapeId, FunctionShape, IndexSignature, ParamInfo,
    PropertyInfo, TypeId, TypeParamInfo, TypePredicate,
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

/// Convert a `CallSignature` to a `FunctionShape` while preserving the
/// signature's method variance bit.
pub(crate) fn function_shape_from_call_signature_preserving_method(
    sig: &CallSignature,
    is_constructor: bool,
) -> FunctionShape {
    let mut shape = function_shape_from_call_signature(sig, is_constructor);
    shape.is_method = sig.is_method;
    shape
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

/// Re-intern `base` with a replacement return type.
pub(crate) fn function_type_with_return_type(
    db: &dyn TypeDatabase,
    base: &FunctionShape,
    return_type: TypeId,
) -> TypeId {
    let mut shape = base.clone();
    shape.return_type = return_type;
    db.function(shape)
}

/// Intern a simple function type from explicit parameter and return slots.
pub(crate) fn function_type_from_params_and_return(
    db: &dyn TypeDatabase,
    params: Vec<ParamInfo>,
    return_type: TypeId,
) -> TypeId {
    function_type_from_shape(db, FunctionShape::new(params, return_type))
}

/// Intern a function type from declaration-lowered signature parts.
pub(crate) fn function_type_from_parts(
    db: &dyn TypeDatabase,
    type_params: Vec<TypeParamInfo>,
    params: Vec<ParamInfo>,
    this_type: Option<TypeId>,
    return_type: TypeId,
    type_predicate: Option<TypePredicate>,
    is_constructor: bool,
    is_method: bool,
) -> TypeId {
    db.function(FunctionShape {
        type_params,
        params,
        this_type,
        return_type,
        type_predicate,
        is_constructor,
        is_method,
    })
}

/// Intern a function type preserving `shape` metadata with a new parameter list.
pub(crate) fn function_type_with_params_replaced(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    params: Vec<ParamInfo>,
) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type: shape.this_type,
            return_type: shape.return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        },
    )
}

/// Intern a function type preserving `shape` metadata with a new return type.
pub(crate) fn function_type_with_return_replaced(
    db: &dyn TypeDatabase,
    shape: &FunctionShape,
    return_type: TypeId,
) -> TypeId {
    function_type_from_shape(
        db,
        FunctionShape {
            type_params: shape.type_params.clone(),
            params: shape.params.clone(),
            this_type: shape.this_type,
            return_type,
            type_predicate: shape.type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        },
    )
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

/// Intern a function type from a signature together with a per-parameter
/// "optional only for JS call-arity leniency" display mask. `mask[i]` flags
/// `sig.params[i]` as a bare, unannotated JS parameter: it stays `optional`
/// for arity checking and subtyping but displays as required, matching tsc.
/// An empty or misaligned mask degrades to the plain intern.
pub(crate) fn function_type_from_call_signature_with_arity_optional_mask(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
    is_constructor: bool,
    mask: &[bool],
) -> TypeId {
    db.function_with_arity_optional_mask(
        function_shape_from_call_signature(sig, is_constructor),
        mask,
    )
}

/// Intern a function type from a signature while preserving the signature's
/// method variance bit.
pub(crate) fn function_type_from_call_signature_preserving_method(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
    is_constructor: bool,
) -> TypeId {
    db.function(function_shape_from_call_signature_preserving_method(
        sig,
        is_constructor,
    ))
}

/// Intern a method function type for a type-literal method signature.
///
/// Method-ness is represented on the `FunctionShape` for single-signature
/// method properties; overload sets keep their method flag on each
/// `CallSignature` inside the callable shape.
pub(crate) fn method_function_type_from_call_signature(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
) -> TypeId {
    let mut shape = function_shape_from_call_signature(sig, false);
    shape.is_method = true;
    db.function(shape)
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

/// Intern the callable/object hybrid produced by an inline type literal.
///
/// The checker assembles `CallSignature`, `PropertyInfo`, and `IndexSignature`
/// facts from the AST; this boundary owns the solver shape convention for the
/// resulting callable, including the single index slot where a symbol index is
/// carried in `string_index` when no string index is present.
pub(crate) fn type_literal_callable_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol_index: Option<IndexSignature>,
    is_abstract: bool,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures,
        properties,
        string_index: string_index.or(symbol_index),
        number_index,
        symbol: None,
        is_abstract,
    })
}

/// Intern a callable type from an explicit, helper-built shape.
pub(crate) fn callable_type_from_shape(db: &dyn TypeDatabase, shape: CallableShape) -> TypeId {
    db.callable(shape)
}

pub(crate) fn declared_method_function_type(db: &dyn TypeDatabase, sig: CallSignature) -> TypeId {
    db.function(FunctionShape {
        type_params: sig.type_params,
        params: sig.params,
        this_type: sig.this_type,
        return_type: sig.return_type,
        type_predicate: sig.type_predicate,
        is_constructor: false,
        is_method: true,
    })
}

pub(crate) fn declared_callable_surface_type(
    db: &dyn TypeDatabase,
    call_signatures: Vec<CallSignature>,
    construct_signatures: Vec<CallSignature>,
    properties: Vec<PropertyInfo>,
    string_index: Option<IndexSignature>,
    number_index: Option<IndexSignature>,
    symbol: Option<SymbolId>,
    is_abstract: bool,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures,
        construct_signatures,
        properties,
        string_index,
        number_index,
        symbol,
        is_abstract,
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

/// Re-intern `base` with a replacement abstractness bit, preserving every
/// other callable-shape field.
pub(crate) fn callable_with_abstract_flag(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    is_abstract: bool,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: base.call_signatures.clone(),
        construct_signatures: base.construct_signatures.clone(),
        properties: base.properties.clone(),
        string_index: base.string_index,
        number_index: base.number_index,
        symbol: base.symbol,
        is_abstract,
    })
}

/// Re-intern `base` with all construct signature returns replaced.
pub(crate) fn callable_with_construct_return_type(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    return_type: TypeId,
) -> TypeId {
    let mut construct_signatures = base.construct_signatures.clone();
    for sig in &mut construct_signatures {
        sig.return_type = return_type;
    }
    callable_with_signatures_replaced(db, base, base.call_signatures.clone(), construct_signatures)
}

/// Re-intern `base` with replacement properties, preserving callable metadata
/// and signatures.
pub(crate) fn callable_with_properties_replaced(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    properties: Vec<PropertyInfo>,
) -> TypeId {
    db.callable(CallableShape {
        call_signatures: base.call_signatures.clone(),
        construct_signatures: base.construct_signatures.clone(),
        properties,
        string_index: base.string_index,
        number_index: base.number_index,
        symbol: base.symbol,
        is_abstract: base.is_abstract,
    })
}

/// Re-intern `base` with replacement call signatures and detached nominal
/// callable metadata.
pub(crate) fn callable_with_call_signatures_and_erased_metadata(
    db: &dyn TypeDatabase,
    base: &CallableShape,
    call_signatures: Vec<CallSignature>,
) -> TypeId {
    instantiated_callable_from_base(db, base, call_signatures, base.construct_signatures.clone())
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

/// Resolve a conditional whose check type is `any` to the union of both
/// branches (tsc `getConditionalType` wildcard rule). Used on erased
/// overload/implementation returns before the return relation; see the
/// solver-side doc for details.
pub(crate) fn distribute_any_check_conditional(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::distribute_any_check_conditional(db, type_id)
}
