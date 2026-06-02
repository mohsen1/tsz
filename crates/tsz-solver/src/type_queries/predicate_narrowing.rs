//! Call-site-aware type predicate extraction for control-flow narrowing.
//!
//! A type predicate is a property of a single call signature. For an overloaded
//! callee the predicate that applies depends on which overload resolution
//! selected, so it cannot be recovered by scanning the overload set the way
//! [`extract_predicate_signature`] does. Narrowing a call must instead read the
//! predicate recorded for the resolved signature at the call site; this module
//! provides the gate that keeps the overload set out of that decision.

use crate::construction::TypeDatabase;
use crate::type_queries::flow::{
    ExtractedPredicateSignature, PredicateSignatureKind, classify_for_predicate_signature,
    extract_predicate_signature,
};
use crate::types::TypeId;

/// Extract a type predicate for control-flow narrowing, rejecting overloaded
/// callables (more than one call signature) as having no statically-derivable
/// predicate. The applicable predicate of an overloaded call depends on which
/// overload resolution selected, so it must be read from the resolved call site
/// rather than recovered by scanning the overload set. Single-signature
/// callables, functions, unions, and intersections behave as in
/// [`extract_predicate_signature`].
pub fn extract_predicate_signature_for_narrowing(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<ExtractedPredicateSignature> {
    if let PredicateSignatureKind::Callable(shape_id) =
        classify_for_predicate_signature(db, type_id)
        && db.callable_shape(shape_id).call_signatures.len() > 1
    {
        return None;
    }
    extract_predicate_signature(db, type_id)
}
