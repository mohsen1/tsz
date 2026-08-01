//! `contains_free_infer_types` cache wiring, split out of `content_predicates`
//! to stay under that file's size ratchet.

use super::content_predicates::{ContentPredicate, contains_content_cached};
use crate::construction::TypeDatabase;
use crate::types::{TypeData, TypeId};
use crate::visitors::child_policy::ChildPolicy;

/// `Infer` containment over the [`ChildPolicy::FREE_INFER`] child set: skips
/// generic signature bodies and the operands of deferred conditional/mapped/
/// indexed-access/keyof operations, so only a genuinely free inference
/// placeholder counts. See [`ChildPolicy::FREE_INFER`]'s doc for the full
/// rationale (issue #14784). Freeness under this policy is a pure structural
/// function of the `TypeId` within one interner, so the deep walk is memoized
/// per node in its own project-wide cache slot, distinct from the plain
/// `InferPredicate` and from `FreeTypeParamPredicate` (which also matches
/// `TypeParameter`/`ThisType`/`BoundParameter` over the wider
/// `FREE_TYPE_PARAMS` policy).
struct FreeInferPredicate;
impl ContentPredicate for FreeInferPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::Infer(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_free_infer_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_free_infer_cache(type_id, result);
    }
    fn child_policy(&self) -> ChildPolicy {
        ChildPolicy::FREE_INFER
    }
}

/// Check if a type contains a *free* `infer` type — see
/// `visitor_predicates::contains_free_infer_types` for the full semantics
/// this must agree with (used by `should_suppress_assignability_diagnostic`
/// to avoid suppressing real errors when the only `infer` types are in type
/// parameter constraint chains). The deep walk is memoized per node in the
/// project-wide predicate cache, so repeated checks over shared closed
/// subtrees stay O(1) (#15729).
pub fn contains_free_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_content_cached(db, type_id, &FreeInferPredicate)
}
