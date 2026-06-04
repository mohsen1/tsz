use super::call_evaluator::{
    AssignabilityChecker, CallEvaluator, CallResult, CallWithCheckerResult, CombinedUnionSignature,
    UnionCallSignatureCompatibility,
};

use crate::construction::{QueryDatabase, TypeDatabase};

use crate::instantiation::instantiate::TypeSubstitution;

use crate::operations::GenericCallResult;

use crate::types::{
    CallSignature, CallableShape, FunctionShape, IntrinsicKind, ParamInfo, TupleElement, TypeData,
    TypeId, TypeListId,
};

use rustc_hash::FxHashSet;

/// Returns the declared `this` type only when it constrains the receiver. Per
/// tsc's `checkApplicableSignature` gate (`thisType !== voidType`, source of
/// TS2684), a callable declared `this: void` opts out of the receiver check
/// and accepts any receiver — even one not structurally assignable to `void`.
pub(crate) fn receiver_constraining_this_type(this_type: Option<TypeId>) -> Option<TypeId> {
    this_type.filter(|&t| t != TypeId::VOID)
}

include!("call_resolution_parts/part1.rs");
include!("call_resolution_parts/part2.rs");

/// Compute the result type of a call expression that matched no overload
/// signature, mirroring tsc's `createUnionOfSignaturesForOverloadFailure`:
/// `getIntersectionType(candidates.map(getReturnTypeOfSignature))`.
///
/// The intersection makes disjoint primitive returns (`string` & `number`)
/// collapse to `never` — assignable to any annotation, so it suppresses
/// downstream cascades after the TS2769 already reported — while compatible
/// object returns merge (`{ a }` & `{ b }`) so member access still resolves and
/// uniform returns survive intact for chained access.
///
/// Generic overloads need per-candidate instantiation for an accurate recovery
/// type; intersecting their un-instantiated return types (still carrying free
/// type parameters) would mislead, so those keep the simpler last-signature
/// recovery type.
pub fn overload_failure_return_type(
    db: &dyn QueryDatabase,
    signatures: &[CallSignature],
) -> TypeId {
    let Some(last) = signatures.last() else {
        return TypeId::NEVER;
    };
    if signatures.iter().any(|sig| !sig.type_params.is_empty()) {
        return last.return_type;
    }
    db.factory()
        .intersection(signatures.iter().map(|sig| sig.return_type).collect())
}

pub fn infer_call_signature<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    sig: &CallSignature,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_call_signature(sig, arg_types)
}

pub fn infer_generic_function<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func: &FunctionShape,
    arg_types: &[TypeId],
) -> TypeId {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.infer_generic_function(func, arg_types)
}

/// Named options for `resolve_call_with_checker_and_arg_sources`.
pub struct ResolveCallOptions<'a> {
    pub force_bivariant_callbacks: bool,
    pub contextual_type: Option<TypeId>,
    pub actual_this_type: Option<TypeId>,
    pub arg_source_is_type_annotation: &'a [bool],
    pub arg_source_is_readonly_annotation: &'a [bool],
}

pub fn resolve_call_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
    actual_this_type: Option<TypeId>,
) -> CallWithCheckerResult {
    resolve_call_with_checker_and_arg_sources(
        interner,
        checker,
        func_type,
        arg_types,
        &ResolveCallOptions {
            force_bivariant_callbacks,
            contextual_type,
            actual_this_type,
            arg_source_is_type_annotation: &[],
            arg_source_is_readonly_annotation: &[],
        },
    )
}

pub fn resolve_call_with_checker_and_arg_sources<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    func_type: TypeId,
    arg_types: &[TypeId],
    opts: &ResolveCallOptions<'_>,
) -> CallWithCheckerResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(opts.force_bivariant_callbacks);
    evaluator.set_contextual_type(opts.contextual_type);
    evaluator.set_actual_this_type(opts.actual_this_type);
    evaluator.set_arg_source_is_type_annotation(opts.arg_source_is_type_annotation);
    evaluator.set_arg_source_is_readonly_annotation(opts.arg_source_is_readonly_annotation);
    let result = evaluator.resolve_call(func_type, arg_types);
    let predicate = evaluator.last_instantiated_predicate.take();
    let instantiated_params = evaluator.last_instantiated_params.take();
    (result, predicate, instantiated_params)
}

pub fn resolve_new_with_checker<C: AssignabilityChecker>(
    interner: &dyn QueryDatabase,
    checker: &mut C,
    type_id: TypeId,
    arg_types: &[TypeId],
    force_bivariant_callbacks: bool,
    contextual_type: Option<TypeId>,
) -> CallResult {
    let mut evaluator = CallEvaluator::new(interner, checker);
    evaluator.set_force_bivariant_callbacks(force_bivariant_callbacks);
    evaluator.set_contextual_type(contextual_type);
    evaluator.resolve_new(type_id, arg_types)
}

pub fn compute_contextual_types_with_compat_checker<'a, R, F>(
    interner: &'a dyn QueryDatabase,
    resolver: &'a R,
    shape: &FunctionShape,
    arg_types: &[TypeId],
    contextual_type: Option<TypeId>,
    configure_checker: F,
) -> TypeSubstitution
where
    R: crate::relations::subtype::TypeResolver,
    F: FnOnce(&mut crate::relations::compat::CompatChecker<'a, R>),
{
    let mut checker = crate::relations::compat::CompatChecker::with_resolver(interner, resolver);
    configure_checker(&mut checker);

    let mut evaluator = CallEvaluator::new(interner, &mut checker);
    evaluator.set_contextual_type(contextual_type);
    evaluator.compute_contextual_types(shape, arg_types)
}

pub fn get_contextual_signature_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::relations::compat::CompatChecker>::get_contextual_signature(db, type_id)
}

pub fn get_contextual_signature_cached_with_compat_checker(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::relations::compat::CompatChecker>::get_contextual_signature_cached(
        db, type_id,
    )
}

pub fn get_contextual_signature_for_arity_with_compat_checker(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::relations::compat::CompatChecker>::get_contextual_signature_for_arity(
        db,
        type_id,
        Some(arg_count),
    )
}

pub fn get_contextual_signature_for_arity_cached_with_compat_checker(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    arg_count: usize,
) -> Option<FunctionShape> {
    CallEvaluator::<crate::relations::compat::CompatChecker>::get_contextual_signature_for_arity_cached(
        db,
        type_id,
        Some(arg_count),
    )
}
