//! Query-boundary wrappers for generic type-argument defaulting.

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
