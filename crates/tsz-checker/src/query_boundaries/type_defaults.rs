//! Query-boundary wrappers for generic type-argument defaulting.

use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// Fill omitted trailing type arguments from type-parameter defaults.
pub(crate) fn fill_application_defaults(
    db: &dyn TypeDatabase,
    args: &[TypeId],
    params: &[tsz_solver::TypeParamInfo],
) -> Option<Vec<TypeId>> {
    tsz_solver::computation::fill_application_defaults(db, args, params)
}

/// Resolve the named (signature-own) type parameters that appear free in a
/// *failed* generic call's recovered result type to their declared
/// `default → constraint → unknown`, matching tsc's instantiation of a call
/// with default type arguments when the argument-count check fails before
/// inference runs. Type parameters not in `names` (enclosing-scope parameters)
/// are preserved.
/// See [`tsz_solver::computation::resolve_named_type_params_to_defaults`].
pub(crate) fn resolve_named_type_params_to_defaults(
    db: &dyn TypeDatabase,
    ty: TypeId,
    names: &FxHashSet<Atom>,
) -> TypeId {
    tsz_solver::computation::resolve_named_type_params_to_defaults(db, ty, names)
}

/// Resolve `ty`'s references to a signature's own `type_params` to their
/// `default → constraint → unknown` fallback (a no-op when the signature is
/// non-generic). Shared by the call and constructor failed-arity recovery paths.
pub(crate) fn resolve_signature_default_type_args(
    db: &dyn TypeDatabase,
    ty: TypeId,
    type_params: &[tsz_solver::TypeParamInfo],
) -> TypeId {
    if type_params.is_empty() {
        return ty;
    }
    let names: FxHashSet<Atom> = type_params.iter().map(|tp| tp.name).collect();
    resolve_named_type_params_to_defaults(db, ty, &names)
}
