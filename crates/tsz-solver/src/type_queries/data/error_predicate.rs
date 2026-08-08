//! `contains_error_type_db` cache wiring, split out of `content_predicates`
//! to stay under that file's size ratchet (mirrors `free_infer_predicate`).

use super::content_predicates::{ContentPredicate, contains_content_cached};
use crate::construction::TypeDatabase;
use crate::types::{TypeData, TypeId};
use crate::visitors::child_policy::ChildPolicy;

/// `Error`/`UnresolvedTypeName` content predicate over the
/// [`ChildPolicy::ERROR_CONTAINMENT`] surface, plus its dedicated project-wide
/// cache slot. The intrinsic-range `TypeId::ERROR` sentinel is reported via
/// [`ContentPredicate::sentinel`] so the shared cached walk detects it both at
/// the root and when nested, without the walker hardcoding an error id.
struct ErrorPredicate;
impl ContentPredicate for ErrorPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_error_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_error_cache(type_id, result);
    }
    fn child_policy(&self) -> ChildPolicy {
        ChildPolicy::ERROR_CONTAINMENT
    }
    fn sentinel(&self) -> Option<TypeId> {
        Some(TypeId::ERROR)
    }
}

/// Check if a type contains the error type anywhere in its structure.
///
/// The single canonical error-containment answer: an error (the `TypeId::ERROR`
/// sentinel, a `TypeData::Error` node, or an `UnresolvedTypeName`) is detected
/// anywhere on the committed [`ChildPolicy::ERROR_CONTAINMENT`] surface —
/// including `Application` bases, but excluding type-parameter declaration
/// metadata and the operands of deferred type-level operations. The answer is
/// immutable per `TypeId` within one interner (the policy treats `Lazy`/
/// `Recursive` as opaque leaves and never resolves def bodies), so the deep
/// walk is memoized per node in the project-wide `ContainsError` cache slot,
/// mirroring the sibling `Contains*` predicates instead of re-walking on every
/// cascade-suppression query (#15729). `visitor_predicates::contains_error_type`
/// delegates here so both query paths share the memo and give one answer.
///
/// The root `TypeId::ERROR` sentinel is handled inside `contains_content_cached`
/// (via [`ErrorPredicate::sentinel`]), so this is a bare delegate.
pub fn contains_error_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_content_cached(db, type_id, &ErrorPredicate)
}
