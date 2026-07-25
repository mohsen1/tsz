//! `elaborateDidYouMeanToCallOrConstruct` predicate.
//!
//! Split out of `assignability.rs` to keep that file under the 2000-line
//! architecture limit.

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// tsc's `elaborateDidYouMeanToCallOrConstruct` predicate: does `source` have a
/// construct or call signature whose return type would have satisfied `target`?
///
/// When it does, tsc re-reports the assignability failure on the source
/// *expression* and suggests calling it (or using `new`), instead of anchoring
/// at the declaration name:
///
/// ```ts
/// declare function getRover(): Dog;
/// export let x: Dog = getRover;   // reported at `getRover`, not at `x`
/// ```
///
/// Construct signatures are consulted before call signatures, matching tsc's
/// ordering. `any`/`unknown`/error/`never` return types are skipped: they relate
/// to everything, so including them would make the suggestion fire on
/// completely unrelated mismatches.
pub(crate) fn did_you_mean_call_or_construct(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    let mut return_types: Vec<TypeId> = Vec::new();
    if let Some(signatures) = super::construct_signatures::construct_signatures_for_type(db, source)
    {
        return_types.extend(signatures.iter().map(|signature| signature.return_type));
    }
    if let Some(signatures) = super::common::call_signatures_for_type(db, source) {
        return_types.extend(signatures.iter().map(|signature| signature.return_type));
    }
    // `call_signatures_for_type` collects call signatures carried by an object
    // or callable shape, but a bare function type (`() => A`) has none to
    // collect -- its signature lives on the function shape itself. The construct
    // lookup above already has the equivalent `get_function_shape` fallback, and
    // without the matching one here the predicate fired for
    // `declare function getA(): A` and for `declare const Ctor: { new(): A }`
    // yet silently missed `declare const Fn: () => A`.
    if return_types.is_empty()
        && let Some(shape) = tsz_solver::type_queries::get_function_shape(db, source)
        && !shape.is_constructor
    {
        return_types.push(shape.return_type);
    }
    return_types.into_iter().any(|return_type| {
        !return_type.is_any_unknown_or_error()
            && return_type != TypeId::NEVER
            && tsz_solver::relations::subtype::is_subtype_of(db, return_type, target)
    })
}
