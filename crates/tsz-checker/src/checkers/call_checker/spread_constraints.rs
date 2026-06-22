//! Spread-argument constraint helpers for call checking.

use crate::query_boundaries::checkers::call::{
    array_element_type_for_type, tuple_elements_for_type,
};
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// Whether a type-parameter spread constraint is array- or tuple-like, so that
/// `...params` (where `params: P`) is a safe variadic spread rather than a
/// destructured argument list.
///
/// The direct constraint is tested first. When it is a *deferred* type
/// expression — e.g. `P extends Parameters<F>`, whose constraint is an
/// unevaluated alias application that resolves to a `Conditional` — the direct
/// tuple/array probes both miss. The fallback evaluates the constraint to its
/// structural form and, for a deferred conditional, resolves it to its apparent
/// base constraint (the union of branches, tsc's
/// `getDefaultConstraintOfConditionalType`) before re-probing. This only ever
/// *recognizes more* array/tuple-like constraints; a non-array constraint
/// (e.g. `T extends number`) still fails every probe, so a genuinely invalid
/// spread keeps its diagnostic.
pub(super) fn constraint_is_array_or_tuple_like(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    let is_array_or_tuple = |ty: TypeId| {
        array_element_type_for_type(db, ty).is_some() || tuple_elements_for_type(db, ty).is_some()
    };
    if is_array_or_tuple(constraint) {
        return true;
    }
    // Resolve a deferred constraint (e.g. an unevaluated `Parameters<F>`
    // application) to its structural form and re-probe.
    let evaluated = crate::query_boundaries::common::evaluate_type(db, constraint);
    if evaluated != constraint && is_array_or_tuple(evaluated) {
        return true;
    }
    // A deferred conditional stays deferred after evaluation; use its apparent
    // base constraint (union of branches) for the array/tuple probe.
    crate::query_boundaries::common::conditional_default_constraint(db, evaluated)
        .is_some_and(is_array_or_tuple)
}
