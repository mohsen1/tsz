//! Type content predicates and compound type extraction helpers.
//!
//! Contains `contains_*`, `is_*` predicates, union/intersection member access,
//! array/tuple extraction, and compound member mapping.

use std::ops::ControlFlow;

use super::type_id_list::TypeIdList;
use crate::construction::TypeDatabase;
use crate::def::DefinitionStore;
use crate::types::{IntrinsicKind, TypeData, TypeId};
use crate::visitors::child_policy::{
    ChildPolicy, has_policy_children, try_for_each_child_with_policy,
};
use crate::visitors::visitor_predicates::contains_type_matching;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

// =============================================================================
// Type Content Queries
// =============================================================================

/// Check if a type contains any type parameters.
///
/// Unlike the solver-internal `visitor::contains_type_parameters`, this version
/// also treats `ThisType` (polymorphic `this`) and `BoundParameter` (generic
/// signature-index parameters) as type parameters. This is the correct semantic
/// for checker use cases that need to decide whether a type requires instantiation.
pub fn contains_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types never contain type parameters
    if type_id.is_intrinsic() {
        return false;
    }
    // Fast path: check top-level type directly before any cache/walk work.
    match db.lookup(type_id) {
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_),
        ) => return true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Recursive(_)
            | TypeData::Enum(_, _),
        ) => return false,
        _ => {}
    }
    // The "sticky bit" decision (does this type still need instantiation?) is
    // asked thousands of times for the same closed subtrees while recursive
    // mapped/conditional bodies expand. The answer is immutable per `TypeId`
    // within one interner, so memoize the deep walk in a project-wide cache and
    // consult it for every child `TypeId` too. Without this the fan-out is
    // dominated by re-walking shared closed leaves across fresh evaluators.
    if let Some(cached) = db.contains_type_params_cached(type_id) {
        return cached;
    }
    contains_content_cached(db, type_id, &TypeParamPredicate)
}

/// A project-stable content predicate over a single type node, plus the
/// interner cache slot that memoizes the deep walk that uses it.
///
/// All implementors check a property that is immutable for a `TypeId` within
/// one interner (e.g. "contains a type parameter", "contains `infer`"), so the
/// deep walk's per-node answer can be cached project-wide and shared across the
/// many fresh evaluators created during instantiation. See
/// [`contains_content_cached`].
pub(super) trait ContentPredicate {
    /// Whether this node *itself* satisfies the predicate. When `true`, the
    /// walker short-circuits without descending into children.
    fn matches_node(&self, db: &dyn TypeDatabase, key: &TypeData) -> bool;
    /// Look up a cached deep-walk result for `type_id`.
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool>;
    /// Store a deep-walk result for `type_id`.
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool);
}

pub(super) struct TypeParamPredicate;
impl ContentPredicate for TypeParamPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        )
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_type_params_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_type_params_cache(type_id, result);
    }
}

struct ParamOrInferPredicate;
impl ContentPredicate for ParamOrInferPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_param_or_infer_root_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_param_or_infer_root_cache(type_id, result);
    }
}

/// Check whether `type_id` contains the legacy solver-internal
/// `TypeParameter | Infer` predicate.
///
/// This preserves `visitor_predicates::contains_type_parameters` semantics:
/// `ThisType` and `BoundParameter` are not considered matches here. The answer
/// is immutable per `TypeId`, so repeated instantiation/evaluation gates share
/// the project-wide content-predicate cache instead of re-walking each subtree.
pub fn contains_param_or_infer_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_)) => return true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_),
        ) => return false,
        _ => {}
    }
    contains_content_cached(db, type_id, &ParamOrInferPredicate)
}

pub(super) struct InferPredicate;
impl ContentPredicate for InferPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        match key {
            TypeData::Infer(_) => true,
            TypeData::TypeParameter(tp) => tp.is_infer_placeholder(),
            _ => false,
        }
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_infer_types_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_infer_types_cache(type_id, result);
    }
}

pub(super) struct TypeQueryPredicate;
impl ContentPredicate for TypeQueryPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::TypeQuery(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_type_query_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_type_query_cache(type_id, result);
    }
}

pub(super) struct LazyOrRecursivePredicate;
impl ContentPredicate for LazyOrRecursivePredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::Lazy(_) | TypeData::Recursive(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_lazy_or_recursive_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_lazy_or_recursive_cache(type_id, result);
    }
}

pub(super) struct ThisTypePredicate;
impl ContentPredicate for ThisTypePredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::ThisType)
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_this_type_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_this_type_cache(type_id, result);
    }
}

/// Deeply-cached `contains ThisType` walk. Backs
/// `visitor_predicates::contains_this_type` so the per-node answers are
/// memoized in the shared `contains_this` cache rather than only the top level.
pub fn contains_this_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_content_cached(db, type_id, &ThisTypePredicate)
}

/// Whether evaluating `type_id` is a no-op under *every* evaluator and resolver
/// in a project run — i.e. the type contains no node whose evaluation depends on
/// the resolver (alias/`typeof` resolution) or the substitution environment
/// (type-parameter mapper, bound `this`).
///
/// A `true` answer means `evaluate(type_id)` returns `type_id` unchanged for any
/// `TypeEvaluator`, because the type holds none of the kinds the evaluator's
/// `visit_type_key` rewrites nor any substitution-dependent leaf:
/// `Conditional`, `IndexAccess`, `Mapped`, `KeyOf`, `TypeQuery`, `Application`,
/// `TemplateLiteral`, `Lazy`, `Recursive`, `StringIntrinsic`, `NoInfer`,
/// `UnresolvedTypeName`, `TypeParameter`, `Infer`, `ThisType`, `BoundParameter`.
///
/// The walk descends the *entire* structural surface
/// ([`ChildPolicy::EVERYTHING`], including `Application` bases, write types,
/// index keys, and callable index-signature values) — narrower child policies
/// could hide an unresolved `Lazy` in a skipped position and wrongly classify a
/// deferral as inert. The per-node answer is immutable per `TypeId` (it asks a
/// purely structural question), so it is memoized in the shared
/// [`PredicateCacheKind::StructurallyEvalInert`] bit and amortized O(1) after the
/// first walk.
///
/// [`PredicateCacheKind::StructurallyEvalInert`]:
///     crate::intern::core::interner::PredicateCacheKind
pub fn is_structurally_eval_inert(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return true;
    }
    if let Some(cached) = db.structurally_eval_inert_cached(type_id) {
        return cached;
    }
    let mut walker = EvalInertWalker {
        db,
        visiting: FxHashSet::default(),
    };
    !walker.contains_eval_affecting(type_id).0
}

/// Whether `key` is itself an evaluation-affecting node (resolver- or
/// substitution-dependent). Mirrors the kinds the evaluator's `visit_type_key`
/// rewrites plus the substitution-dependent leaves.
const fn is_eval_affecting_node(key: &TypeData) -> bool {
    matches!(
        key,
        TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::Mapped(_)
            | TypeData::KeyOf(_)
            | TypeData::TypeQuery(_)
            | TypeData::Application(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::StringIntrinsic { .. }
            | TypeData::NoInfer(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
    )
}

/// Cycle-tracking walker backing [`is_structurally_eval_inert`]. Descends the
/// full structural surface and writes only fully-resolved (untainted) per-node
/// answers to the shared cache.
struct EvalInertWalker<'a> {
    db: &'a dyn TypeDatabase,
    visiting: FxHashSet<TypeId>,
}

impl EvalInertWalker<'_> {
    /// Returns `(contains_eval_affecting, cycle_tainted)`.
    fn contains_eval_affecting(&mut self, type_id: TypeId) -> (bool, bool) {
        if type_id.is_intrinsic() {
            return (false, false);
        }
        // The cache stores inertness (the negation), so a cached `true` means
        // "no eval-affecting node".
        if let Some(inert) = self.db.structurally_eval_inert_cached(type_id) {
            return (!inert, false);
        }
        let Some(key) = self.db.lookup(type_id) else {
            return (false, false);
        };
        if is_eval_affecting_node(&key) {
            // A matching node is a definite fact: not inert, untainted.
            self.db.set_structurally_eval_inert_cache(type_id, false);
            return (true, false);
        }
        if !has_policy_children(&key, &ChildPolicy::EVERYTHING) {
            // A childless inert leaf: definitely inert.
            self.db.set_structurally_eval_inert_cache(type_id, true);
            return (false, false);
        }
        if !self.visiting.insert(type_id) {
            // Re-entering an in-progress node contributes nothing new; mark
            // tainted so the ancestor does not persist a provisional answer.
            return (false, true);
        }
        let mut tainted = false;
        let found = try_for_each_child_with_policy::<(), _>(
            self.db,
            &key,
            &ChildPolicy::EVERYTHING,
            &mut |child| {
                let (child_found, child_tainted) = self.contains_eval_affecting(child);
                tainted |= child_tainted;
                if child_found {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .is_break();
        self.visiting.remove(&type_id);
        if found {
            // A found eval-affecting node is a definite, untainted fact.
            self.db.set_structurally_eval_inert_cache(type_id, false);
            (true, false)
        } else {
            if !tainted {
                // Only persist fully-resolved (untainted) inert answers.
                self.db.set_structurally_eval_inert_cache(type_id, true);
            }
            (false, tainted)
        }
    }
}

pub(super) struct SubstitutionDependentPredicate;
impl ContentPredicate for SubstitutionDependentPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        // Nodes whose evaluation depends on the *substitution environment* (the
        // bound `this`, the active type-parameter mapper). Unlike `Lazy`/
        // `TypeQuery`/`UnresolvedTypeName` — which resolve identically for the
        // single fixed resolver of one project run — these can evaluate to
        // different results for the same `TypeId` depending on the enclosing
        // instantiation, so a type containing them is not safely cacheable by
        // `TypeId` alone.
        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        )
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_resolver_dependent_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_resolver_dependent_cache(type_id, result);
    }
}

pub(super) struct ConditionalPredicate;
impl ContentPredicate for ConditionalPredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(key, TypeData::Conditional(_))
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_conditional_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_conditional_cache(type_id, result);
    }
}

/// Whether the alias-opaque structure of `type_id` contains a `Conditional`.
///
/// Like the generic `contains_type_matching(.., Conditional)` walk, this treats
/// nested `Lazy`/`Application` bases as opaque leaves (it never resolves
/// aliases), so the result is immutable per `TypeId`. Routing it through
/// `contains_content_cached` memoizes every visited node in the project-wide
/// `contains_conditional_cache`, turning the `closed_eval_cache` eligibility
/// gate (`is_closed_cacheable_kind`) from an O(subtree) walk on every
/// cache-miss evaluation into an amortized O(1) lookup.
pub fn contains_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_content_cached(db, type_id, &ConditionalPredicate)
}

/// Whether the body contains a `Conditional` node reachable through its full
/// content surface, **resolving alias `Application`/`Lazy` bases** via
/// `resolve_lazy`.
///
/// Unlike [`contains_conditional_type`] — which treats `Lazy`/`Application`
/// bases as opaque leaves — this walk follows an applied alias
/// (`MappedResponseType<R, T>`) into its registered body so a conditional
/// buried behind a generic alias *inside a method signature* is still detected.
///
/// Used by the cross-module interface-heritage consumption gate: the #13232
/// resolver-less union-normalization defect is triggered specifically by a
/// still-generic conditional surfacing during contextual inference. A published
/// body whose callable members carry no conditional (directly or through an
/// applied alias) cannot feed that path, so it is safe to consume — which is
/// what lets an importing file resolve members inherited through a method-
/// bearing generic interface (`interface D<T> extends Base<T>` where `Base` has
/// a method member). Bodies that do reach a conditional stay gated.
///
/// `resolve_lazy` maps an alias/interface `DefId` to its registered body; it
/// returns `None` when no body is registered (treated as "no conditional behind
/// this alias"). The walk is bounded by a visited set and a depth limit so
/// recursive aliases terminate.
pub fn contains_conditional_through_aliases(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(crate::def::DefId) -> Option<TypeId>,
) -> bool {
    let mut visited = FxHashSet::default();
    contains_conditional_through_aliases_inner(db, type_id, resolve_lazy, &mut visited, 0)
}

const CONDITIONAL_THROUGH_ALIAS_DEPTH_LIMIT: usize = 64;

fn contains_conditional_through_aliases_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    resolve_lazy: &mut dyn FnMut(crate::def::DefId) -> Option<TypeId>,
    visited: &mut FxHashSet<TypeId>,
    depth: usize,
) -> bool {
    if depth > CONDITIONAL_THROUGH_ALIAS_DEPTH_LIMIT || type_id.is_intrinsic() {
        return false;
    }
    if !visited.insert(type_id) {
        return false;
    }
    let Some(data) = db.lookup(type_id) else {
        return false;
    };
    if matches!(data, TypeData::Conditional(_)) {
        return true;
    }
    // Follow an applied alias / bare lazy reference into its registered body so
    // a conditional hidden behind `Alias<Args>` is not missed (the standard
    // content child policy treats application bases as opaque leaves).
    let alias_base_def = match &data {
        TypeData::Application(app_id) => {
            crate::visitors::visitor_extract::lazy_def_id(db, db.type_application(*app_id).base)
        }
        TypeData::Lazy(def_id) => Some(*def_id),
        _ => None,
    };
    if let Some(def_id) = alias_base_def
        && let Some(body) = resolve_lazy(def_id)
        && contains_conditional_through_aliases_inner(db, body, resolve_lazy, visited, depth + 1)
    {
        return true;
    }
    // Descend the standard content surface (object properties, callable/function
    // signature params + returns, application args, union/intersection members,
    // conditional arms), short-circuiting on the first conditional found.
    try_for_each_child_with_policy::<(), _>(
        db,
        &data,
        &ChildPolicy::CONTENT_PREDICATE,
        &mut |child| {
            if contains_conditional_through_aliases_inner(
                db,
                child,
                resolve_lazy,
                visited,
                depth + 1,
            ) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        },
    )
    .is_break()
}

/// Whether `superset`'s named object properties include every named property
/// of `subset`'s object shape.
///
/// Both types must be plain `Object`/`ObjectWithIndex` shapes; any other
/// shape returns `false` (conservative). Used by the cross-module
/// interface-heritage consumption gate: a published definition body must
/// carry at least every member the local lowering derived (it adds heritage
/// members on top of the same own members) — a published body missing own
/// members is a mid-resolution partial that must not be consumed.
pub fn object_property_names_cover(
    db: &dyn TypeDatabase,
    superset: TypeId,
    subset: TypeId,
) -> bool {
    let shape_of = |ty: TypeId| match db.lookup(ty) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            Some(db.object_shape(shape_id))
        }
        _ => None,
    };
    let Some(subset_shape) = shape_of(subset) else {
        return false;
    };
    let Some(superset_shape) = shape_of(superset) else {
        return false;
    };
    subset_shape.properties.iter().all(|needed| {
        superset_shape
            .properties
            .iter()
            .any(|have| have.name == needed.name)
    })
}

/// Whether evaluating `type_id` depends on the substitution environment.
///
/// Returns `true` if the type (recursively) contains any `TypeParameter`/
/// `Infer`/`ThisType`/`BoundParameter`. A `false` answer means the type's
/// evaluation depends only on the project's fixed resolver (via any `Lazy`/
/// `TypeQuery`/`UnresolvedTypeName` refs it contains), so the result for this
/// `TypeId` is stable across evaluator instances — the input gate for the
/// project-wide `closed_eval_cache`.
pub fn is_substitution_dependent_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_content_cached(db, type_id, &SubstitutionDependentPredicate)
}

/// Run a deep, project-cached content walk for `predicate` over `type_id`.
///
/// Descends the same [`ChildPolicy::CONTENT_PREDICATE`] child set as the
/// generic `contains_type_matching` walker, but consults and populates the
/// predicate's persistent project-wide cache at every node. A subtree result is
/// only written to the persistent cache when its computation did NOT touch an
/// in-progress (cycle) node — the `cycle_tainted` flag tracks this so a
/// provisional cycle-break answer is never cached as if it were final.
fn contains_content_cached<P: ContentPredicate>(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    predicate: &P,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(cached) = predicate.cached(db, type_id) {
        return cached;
    }
    let mut walker = CachedContentWalker {
        db,
        predicate,
        visiting: FxHashSet::default(),
    };
    walker.check(type_id)
}

struct CachedContentWalker<'a, P: ContentPredicate> {
    db: &'a dyn TypeDatabase,
    predicate: &'a P,
    visiting: FxHashSet<TypeId>,
}

impl<P: ContentPredicate> CachedContentWalker<'_, P> {
    /// Returns `(predicate_holds, cycle_tainted)`.
    fn check_tracked(&mut self, type_id: TypeId) -> (bool, bool) {
        if type_id.is_intrinsic() {
            return (false, false);
        }
        if let Some(cached) = self.predicate.cached(self.db, type_id) {
            return (cached, false);
        }
        let Some(key) = self.db.lookup(type_id) else {
            return (false, false);
        };
        // Direct match on the node itself short-circuits: the answer is `true`
        // and untainted regardless of any child subtree.
        if self.predicate.matches_node(self.db, &key) {
            self.predicate.set_cache(self.db, type_id, true);
            return (true, false);
        }
        // Terminal fast path: a node with no children under the walker's
        // policy cannot match below itself; skip the visiting-set round-trip.
        if !has_policy_children(&key, &ChildPolicy::CONTENT_PREDICATE) {
            self.predicate.set_cache(self.db, type_id, false);
            return (false, false);
        }
        if !self.visiting.insert(type_id) {
            // Re-entering an in-progress node: this path contributes nothing new
            // (the matching node, if any, is found on the ancestor still being
            // computed). Mark tainted so the ancestor does not persist a
            // possibly-incomplete answer.
            return (false, true);
        }
        let result = self.walk_children(&key);
        self.visiting.remove(&type_id);
        if !result.1 {
            // Only persist fully-resolved (untainted) subtree results.
            self.predicate.set_cache(self.db, type_id, result.0);
        }
        result
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        self.check_tracked(type_id).0
    }

    /// Walk the node's children under the same child set as the generic
    /// uncached walker, by construction.
    fn walk_children(&mut self, key: &TypeData) -> (bool, bool) {
        let db = self.db;
        let mut tainted = false;
        let found = try_for_each_child_with_policy::<(), _>(
            db,
            key,
            &ChildPolicy::CONTENT_PREDICATE,
            &mut |child| {
                let (child_found, child_tainted) = self.check_tracked(child);
                tainted |= child_tainted;
                if child_found {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        )
        .is_break();
        if found {
            // A `true` answer is never tainted: a found match is a definite
            // fact independent of any in-flight cycle node.
            (true, false)
        } else {
            (false, tainted)
        }
    }
}

/// Check if a type contains named type parameters or canonical bound
/// parameters, excluding in-flight `infer` placeholders and polymorphic `this`.
pub fn contains_named_or_bound_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_) | TypeData::BoundParameter(_)
        )
    })
}

/// Like `contains_type_parameters_db`, but ignores references to a known
/// locally-bound mapped key parameter. See
/// [`contains_free_type_parameters_except_name`] for the leaf-treatment
/// rationale.
///
/// [`contains_free_type_parameters_except_name`]:
///     crate::visitors::visitor_predicates::contains_free_type_parameters_except_name
pub fn contains_type_parameters_except_name_db(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    excluded_name: Atom,
) -> bool {
    crate::visitors::visitor_predicates::contains_free_type_parameters_except_name(
        db,
        type_id,
        excluded_name,
    )
}

/// Check if a type's structural surface contains any `keyof` operator
/// (deep walk).
///
/// Structural analogue of `format_type(t).contains("keyof ")`. Returns
/// `true` when the type tree includes a `TypeData::KeyOf` node, including
/// nested inside unions/intersections/applications/conditionals/mapped.
pub fn contains_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| matches!(key, TypeData::KeyOf(_)))
}

/// Check if a type contains an indexed access whose object is a type parameter.
pub fn contains_index_access_with_type_parameter_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    contains_type_matching(
        db,
        type_id,
        |key| matches!(key, TypeData::IndexAccess(object, _) if crate::type_queries::is_type_parameter_like(db, *object)),
    )
}

/// Check if a type contains a generic indexed access surface.
pub fn contains_generic_indexed_access_surface(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(TypeData::IndexAccess(object, index)) = db.lookup(type_id) else {
        return false;
    };
    crate::type_queries::is_type_parameter_like(db, object)
        || contains_type_parameters_db(db, index)
}

/// Check if a type contains an indexed access whose object is a variadic tuple
/// rest element containing a type parameter.
pub fn contains_index_access_with_variadic_tuple_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::IndexAccess(object, _)
                if variadic_tuple_object_contains_type_parameter(db, *object)
        )
    })
}

/// Returns true when a type's structural or display-alias surface contains an
/// indexed access whose object operand is `never`.
pub fn contains_never_index_access_surface(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    max_depth: usize,
) -> bool {
    let mut visited = FxHashSet::default();
    contains_never_index_access_surface_inner(
        db,
        def_store,
        type_id,
        max_depth.saturating_add(1),
        &mut visited,
    )
}

fn contains_never_index_access_surface_inner(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
    remaining_depth: usize,
    visited: &mut FxHashSet<TypeId>,
) -> bool {
    if remaining_depth == 0 || type_id.is_intrinsic() || !visited.insert(type_id) {
        return false;
    }

    if let Some(TypeData::IndexAccess(object, _)) = db.lookup(type_id)
        && object == TypeId::NEVER
    {
        return true;
    }

    if let Some(alias) = db.get_display_alias(type_id)
        && alias != type_id
        && contains_never_index_access_surface_inner(
            db,
            def_store,
            alias,
            remaining_depth - 1,
            visited,
        )
    {
        return true;
    }

    if let Some(def_id) = crate::type_queries::get_application_lazy_def_id(db, type_id)
        && let Some(def) = def_store.get(def_id)
        && def.kind == crate::def::DefKind::TypeAlias
        && let Some(body) = def.body
        && contains_never_index_access_surface_inner(
            db,
            def_store,
            body,
            remaining_depth - 1,
            visited,
        )
    {
        return true;
    }

    let mut found = false;
    crate::visitors::visitor::for_each_child_by_id(db, type_id, |child| {
        if !found {
            found = contains_never_index_access_surface_inner(
                db,
                def_store,
                child,
                remaining_depth - 1,
                visited,
            );
        }
    });
    found
}

fn variadic_tuple_object_contains_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    get_tuple_elements(db, type_id).is_some_and(|elems| {
        elems
            .iter()
            .any(|elem| elem.rest && contains_type_parameters_db(db, elem.type_id))
    })
}

/// Check if a type contains *free* type parameters — type parameters that are
/// not bound by an enclosing function/callable signature's own type parameter list.
///
/// When an object type (interface) has method members like `bar<W>(): Inner<W>`,
/// the `W` type parameter inside the method body is bound by `bar`'s signature.
/// The standard `contains_type_parameters_db` traverses into these bodies and
/// finds `W`, incorrectly reporting that the object type "contains type parameters".
///
/// This variant skips function/callable bodies that have their own type parameters,
/// since any type parameter references inside those bodies are (or should be) bound
/// by the function's own generic declaration, not free from an outer scope.
///
/// Used by TS2344 constraint validation to decide whether a base constraint can
/// be checked eagerly or must be deferred to instantiation time.
pub fn contains_free_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_),
        ) => return true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Recursive(_)
            | TypeData::Enum(_, _),
        ) => return false,
        _ => {}
    }
    crate::visitors::visitor_predicates::contains_free_type_parameters(db, type_id)
}

/// Check if a type contains generic type parameters, excluding `ThisType`.
///
/// Like `contains_type_parameters_db`, but does NOT treat `ThisType` as a type
/// parameter. This is appropriate for TS2352 (type assertion overlap) checking,
/// where `this` resolves to the enclosing class type and should still be checked
/// for overlap — tsc does not suppress type assertion checks for `this`.
pub fn contains_generic_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_)) => {
            return true;
        }
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Recursive(_)
            | TypeData::Enum(_, _)
            | TypeData::ThisType,
        ) => return false,
        _ => {}
    }
    // The depth-limited walk always starts from a fresh recursion guard, so
    // the answer is a pure function of the root `TypeId`. Display-alias
    // bookkeeping and assignability gates re-ask this for the same
    // application args after every evaluation; memoize the root result
    // project-wide.
    if let Some(cached) = db.contains_generic_params_root_cached(type_id) {
        return cached;
    }
    let result = contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_)
        )
    });
    db.set_contains_generic_params_root_cache(type_id, result);
    result
}

struct FileRelativePredicate;
impl ContentPredicate for FileRelativePredicate {
    fn matches_node(&self, _db: &dyn TypeDatabase, key: &TypeData) -> bool {
        matches!(
            key,
            TypeData::UnresolvedTypeName(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::ThisType
                | TypeData::Recursive(_)
        )
    }
    fn cached(&self, db: &dyn TypeDatabase, type_id: TypeId) -> Option<bool> {
        db.contains_file_relative_cached(type_id)
    }
    fn set_cache(&self, db: &dyn TypeDatabase, type_id: TypeId, result: bool) {
        db.set_contains_file_relative_cache(type_id, result);
    }
}

/// Check if a type contains content whose meaning is relative to the file or
/// lexical scope that produced it, rather than to the program-wide type
/// universe.
///
/// Returns `true` when the type (transitively) contains:
/// - `UnresolvedTypeName`: resolved by name against the *current* file, so the
///   same `TypeId` can denote different declarations in different files;
/// - `TypeQuery` / `UniqueSymbol` / `ModuleNamespace`: carry raw `SymbolRef`
///   ids, which are arena-local in project checks;
/// - `ThisType`: bound by the enclosing class/interface context;
/// - `Recursive`: a structural back-reference that is only meaningful relative
///   to an enclosing type, so a bare subtree containing one is not closed.
///
/// `Lazy(DefId)` and `Enum(DefId, _)` references are *not* file-relative: the
/// shared `DefinitionStore` gives them one program-wide meaning. This walk
/// does not chase def bodies; callers that resolve lazily must pair this
/// predicate with an unresolved-`Lazy` taint snapshot
/// (`lazy_resolve_failure_count`) to detect bodies that were not yet
/// registered while a result was computed.
///
/// Used to decide whether a per-file proof (e.g. a TS2344 constraint
/// validation success) may be published to a program-wide cache shared by all
/// file checkers. The answer is immutable per `TypeId`, so the deep walk's
/// per-node results are memoized project-wide.
pub fn contains_file_relative_content_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(
            TypeData::UnresolvedTypeName(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::ThisType
            | TypeData::Recursive(_),
        ) => return true,
        Some(
            TypeData::Literal(_) | TypeData::Intrinsic(_) | TypeData::Error | TypeData::Enum(_, _),
        ) => return false,
        _ => {}
    }
    contains_content_cached(db, type_id, &FileRelativePredicate)
}

/// Check if a type is directly an `Infer` type (not recursive).
///
/// This is a lightweight O(1) check that only inspects the top-level type.
/// Use this when you need to guard against caching leaked Infer results
/// without the cost of a full recursive walk.
pub fn is_infer_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(db.lookup(type_id), Some(TypeData::Infer(_)))
}

/// Check if a type contains any `infer` types.
///
/// Delegates to `visitor_predicates::contains_type_matching` with an `Infer`-only
/// predicate.
pub fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // The deep walk is memoized per node in the project-wide `contains_infer`
    // cache, so repeated checks over the same shapes stay O(1).
    contains_content_cached(db, type_id, &InferPredicate)
}

/// Check if a type contains any unresolved `TypeQuery` references.
///
/// `TypeQuery` types represent `typeof X` that haven't been resolved to concrete types yet.
/// Evaluation results containing unresolved `TypeQuery` refs should not be cached, as the
/// `TypeQuery` may resolve to a different type once the referenced symbol's type is available
/// in the `TypeEnvironment`.
pub fn contains_type_query_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // The deep walk is memoized per node in the project-wide `contains_type_query`
    // cache, so repeated checks over the same shapes stay O(1).
    contains_content_cached(db, type_id, &TypeQueryPredicate)
}

/// Check if a type contains unresolved type parameters other than tsz's internal
/// `__infer_*` placeholders.
///
/// This is useful when a structural contextual type like `[__infer_0, __infer_1]`
/// should still be allowed to guide recontextualization, while real generic
/// type parameters (`T`, `U`, `this`, bound params) should still block it.
pub fn contains_non_infer_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => !tp.is_infer_placeholder(),
        TypeData::Infer(_) | TypeData::ThisType | TypeData::BoundParameter(_) => true,
        _ => false,
    })
}

/// Check if a type contains any lazy or recursive references.
///
/// This is used by checker query boundaries that need to reason about deferred
/// or cyclic types without matching on `TypeData` directly.
pub fn contains_lazy_or_recursive_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // The deep walk is memoized per node in the project-wide
    // `contains_lazy_or_recursive` cache, so repeated checks over the same
    // shapes stay O(1).
    contains_content_cached(db, type_id, &LazyOrRecursivePredicate)
}

/// Check whether a type is itself a bare unresolved infer placeholder, not a
/// larger structural type that merely contains placeholders.
pub fn is_bare_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => tp.is_infer_placeholder(),
        _ => false,
    }
}

/// Check whether a type is itself a bare call-local inference placeholder.
///
/// Higher-order generic function inference also creates `__infer_src_*`
/// placeholders for the generic parameters of a source function argument. Those
/// are not stale call-local placeholders: when they survive into a returned
/// function type they represent type parameters that should be hoisted.
pub fn is_bare_current_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => tp.is_current_infer_placeholder(),
        _ => false,
    }
}

/// Check if a type is a spread marker tuple created by the checker.
pub fn is_spread_marker_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(TypeData::Tuple(elems_id)) = db.lookup(type_id) {
        let elems = db.tuple_list(elems_id);
        if elems.len() != 1 || !elems[0].rest {
            return false;
        }
        elems[0]
            .name
            .is_some_and(|name| db.resolve_atom(name) == "__tsz_spread_argument__")
            || matches!(
                db.lookup(elems[0].type_id),
                Some(TypeData::TypeParameter(_))
            )
    } else {
        false
    }
}

pub fn rest_type_needs_aggregate_argument_check(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            rest_type_needs_aggregate_argument_check(db, inner)
        }
        Some(TypeData::Union(members)) => db.type_list(members).iter().any(|&member| {
            let member = match db.lookup(member) {
                Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => inner,
                _ => member,
            };
            matches!(db.lookup(member), Some(TypeData::Tuple(_)))
                || rest_type_needs_aggregate_argument_check(db, member)
        }),
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Application(_)
            | TypeData::Conditional(_)
            | TypeData::Intersection(_)
            | TypeData::Lazy(_)
            | TypeData::Mapped(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::IndexAccess(_, _),
        ) => true,
        _ => false,
    }
}

/// This detects both bare placeholders and structural types that contain them
/// (e.g., unions like `__infer_0 | PromiseLike<__infer_0>`).
pub fn contains_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_bare_infer_placeholder_db(db, type_id) {
        return true;
    }
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => tp.is_infer_placeholder(),
        TypeData::Infer(_) => true,
        _ => false,
    })
}

/// Check if a type contains a call-local inference placeholder.
///
/// This intentionally excludes `__infer_src_*` placeholders because those carry
/// higher-order source generic parameters and are normalized or hoisted later.
pub fn contains_current_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_bare_current_infer_placeholder_db(db, type_id) {
        return true;
    }
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => tp.is_current_infer_placeholder(),
        TypeData::Infer(_) => true,
        _ => false,
    })
}

/// Check if a type contains the error type.
///
/// Delegates to the canonical `visitor_predicates::contains_error_type` walk,
/// so this checker-facing query and the visitor query give one answer: an
/// error is detected anywhere in the full structural surface, including
/// `Application` bases and the nested raw `TypeId::ERROR` sentinel.
pub fn contains_error_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::visitors::visitor_predicates::contains_error_type(db, type_id)
}

/// Check if a type contains a generic application with an `unknown` argument.
pub fn contains_application_unknown_arg(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| {
        let TypeData::Application(app_id) = key else {
            return false;
        };
        db.type_application(*app_id).args.contains(&TypeId::UNKNOWN)
    })
}

/// Check if a type contains the `never` intrinsic.
///
/// Delegates to `visitor_predicates::contains_type_matching` with a `Never`-only
/// predicate, plus a fast path for the well-known `TypeId::NEVER`.
pub fn contains_never_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NEVER {
        return true;
    }
    contains_type_matching(db, type_id, |key| {
        matches!(key, TypeData::Intrinsic(IntrinsicKind::Never))
    })
}

/// Check whether a type is "deeply any" — i.e. `any` itself, or a composite
/// (array, tuple, union, intersection) whose leaf elements are all `any`.
///
/// This is used during generic inference to detect when a round-1 inference
/// result is effectively `any` so the checker can fall back to better
/// contextual information.
pub fn is_type_deeply_any(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    fn walk(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        visiting: &mut FxHashSet<TypeId>,
        memo: &mut FxHashMap<TypeId, bool>,
    ) -> bool {
        if let Some(&cached) = memo.get(&type_id) {
            return cached;
        }
        if !visiting.insert(type_id) {
            // Cycle while evaluating "all leaves are any" is conservatively false.
            return false;
        }
        let result = if type_id == TypeId::ANY {
            true
        } else if type_id.is_intrinsic() {
            // Non-ANY intrinsics resolve to TypeData::Intrinsic and are
            // never Array/Tuple/Union/Intersection — skip the dyn lookup.
            false
        } else {
            match db.lookup(type_id) {
                Some(TypeData::Array(elem)) => walk(db, elem, visiting, memo),
                Some(TypeData::Tuple(list_id)) => {
                    let elems = db.tuple_list(list_id);
                    elems.iter().all(|e| walk(db, e.type_id, visiting, memo))
                }
                Some(TypeData::Union(list_id)) => {
                    let members = db.type_list(list_id);
                    !members.is_empty() && members.iter().all(|&m| walk(db, m, visiting, memo))
                }
                Some(TypeData::Intersection(list_id)) => {
                    let members = db.type_list(list_id);
                    !members.is_empty() && members.iter().all(|&m| walk(db, m, visiting, memo))
                }
                _ => false,
            }
        };
        visiting.remove(&type_id);
        memo.insert(type_id, result);
        result
    }
    let mut visiting = FxHashSet::default();
    let mut memo = FxHashMap::default();
    walk(db, type_id, &mut visiting, &mut memo)
}

/// Check whether a type (or any union/intersection/readonly/noinfer wrapper)
/// contains an `Application` type.
///
/// Used to decide whether contextual instantiation results should be preserved
/// in their unevaluated form so that generic type argument structure is retained
/// for downstream inference.
pub fn contains_application_in_structure(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Application(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_application_in_structure(db, m))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_application_in_structure(db, m))
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            contains_application_in_structure(db, inner)
        }
        _ => false,
    }
}

/// Check whether a type contains an `Application` along base-constraint
/// resolution paths.
///
/// This is intentionally narrower than a full structural traversal and broader
/// than `contains_application_in_structure`: circular-constraint checking needs
/// to know whether alias expansion may affect mapped key sources or indexed
/// access object/index operands, while contextual inference must not treat those
/// nested surfaces as a reason to preserve an unevaluated application shape.
pub fn contains_application_in_constraint_resolution_path(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Application(_)) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_application_in_constraint_resolution_path(db, m))
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            contains_application_in_constraint_resolution_path(db, inner)
        }
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.get_mapped(mapped_id);
            contains_application_in_constraint_resolution_path(db, mapped.constraint)
        }
        Some(TypeData::IndexAccess(object_type, index_type)) => {
            contains_application_in_constraint_resolution_path(db, object_type)
                || contains_application_in_constraint_resolution_path(db, index_type)
        }
        _ => false,
    }
}

/// Return true when `type_id` contains an application of a generic alias whose
/// body both references itself and requires concrete evaluation. This identifies
/// recursive conditional/mapped aliases such as
/// `Schema<T> = ... { [P in keyof T]: Schema<T[P]> } ...` without exposing
/// `TypeData` matching to checker code.
pub fn contains_recursive_operation_application_db(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let mut found = false;
    crate::visitor::walk_referenced_types(db, type_id, |current| {
        if found {
            return;
        }
        let Some(TypeData::Application(app_id)) = db.lookup(current) else {
            return;
        };
        let app = db.type_application(app_id);
        let Some(TypeData::Lazy(def_id)) = db.lookup(app.base) else {
            return;
        };
        let Some(body) = def_store.get_body(def_id) else {
            return;
        };
        if super::signatures_and_advanced::body_arg_requires_concrete_form(db, body)
            && crate::visitor::contains_lazy_def_id(db, body, def_id)
        {
            found = true;
        }
    });
    found
}

/// Return true when `type_id` itself is an application of a recursive generic
/// alias whose body requires concrete evaluation.
pub fn is_recursive_operation_application_db(
    db: &dyn TypeDatabase,
    def_store: &DefinitionStore,
    type_id: TypeId,
) -> bool {
    let Some(TypeData::Application(app_id)) = db.lookup(type_id) else {
        return false;
    };
    let app = db.type_application(app_id);
    let Some(TypeData::Lazy(def_id)) = db.lookup(app.base) else {
        return false;
    };
    let Some(body) = def_store.get_body(def_id) else {
        return false;
    };
    super::signatures_and_advanced::body_arg_requires_concrete_form(db, body)
        && crate::visitor::contains_lazy_def_id(db, body, def_id)
}

/// Return true when `type_id` (or any union/intersection member reachable from it)
/// is a `ConditionalType` whose `extends_type` is still an unevaluated
/// `Application` type.
pub fn contains_conditional_with_application_extends(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    fn walk(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        if let Some(TypeData::Conditional(cond_id)) = db.lookup(type_id) {
            let cond = db.get_conditional(cond_id);
            if matches!(db.lookup(cond.extends_type), Some(TypeData::Application(_))) {
                return true;
            }
        }
        if let Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) = db.lookup(type_id)
        {
            let members = db.type_list(list_id);
            if members.iter().any(|&member| walk(db, member, depth + 1)) {
                return true;
            }
        }
        false
    }

    walk(db, type_id, 0)
}

// =============================================================================
// Type Extraction Helpers
// =============================================================================
// These functions extract data from types, avoiding the need for checker code
// to match on TypeData directly.
//
// ## Usage Pattern
//
// These are SHALLOW queries that do NOT resolve Lazy/Ref automatically.
// Checker code must resolve types before calling these:
//
// ```rust,ignore
// // 1. Resolve the type first
// let resolved_id = self.solver.resolve_type(type_id);
//
// // 2. Then use the extractor
// if let Some(members) = get_union_members(self.db, resolved_id) {
//     // ...
// }
// ```
//
// ## Available Extractors
//
// - Unions: get_union_members
// - Intersections: get_intersection_members
// - Objects: get_object_shape_id, get_object_shape
// - Arrays: get_array_element_type
// - Tuples: get_tuple_elements
//
// These helpers cover 90%+ of structural extraction needs in the Checker.

/// Get the members of a union type.
///
/// Returns None if the type is not a union. See [`TypeIdList`] for why this
/// returns a shared, zero-copy view rather than an owned `Vec<TypeId>`.
pub fn get_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeIdList> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => Some(TypeIdList::new(db.type_list(list_id))),
        _ => None,
    }
}

/// Returns `true` if `type_id` is a union or intersection whose members are
/// all primitive intrinsics or literal types (string/number/boolean literals).
/// tsc expands such type aliases in error messages instead of preserving the
/// alias name — e.g. `type T2 = "a" | "b"` displays as `"a" | "b"`, not `T2`.
pub fn is_primitive_or_literal_compound(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let members_id = match db.lookup(type_id) {
        Some(TypeData::Union(m)) | Some(TypeData::Intersection(m)) => m,
        _ => return false,
    };
    let members = db.type_list(members_id);
    members.iter().all(|m| {
        m.is_intrinsic()
            || matches!(
                db.lookup(*m),
                Some(TypeData::Literal(_) | TypeData::Intrinsic(_))
            )
    })
}

/// Returns `true` if `type_id` is itself a literal/primitive, or a union or
/// intersection composed entirely of literal/primitive members.
///
/// Used for diagnostic display: when a generic type alias application reduces
/// to such a "terminal" form (e.g. `KeysExtendedBy<M, number>` reducing to
/// `"b"`), tsc drops the alias and shows the resolved literal in error
/// messages. Object/interface results keep the alias form.
pub fn is_literal_or_primitive_or_compound_of_those(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(_) | TypeData::Intrinsic(_)) => true,
        Some(TypeData::Union(_) | TypeData::Intersection(_)) => {
            is_primitive_or_literal_compound(db, type_id)
        }
        _ => false,
    }
}

/// Returns true when `type_id` is a literal type or a union whose members are
/// all literal types.
pub fn is_literal_or_literal_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .all(|&member| is_literal_or_literal_union_type(db, member)),
        _ => false,
    }
}

/// Get the members of an intersection type.
///
/// Returns None if the type is not an intersection.
pub fn get_intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeIdList> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        // See `get_union_members` / `TypeIdList`: return a zero-copy view of
        // the interned member list to avoid a per-call allocation + copy on
        // a hot checker-boundary query.
        Some(TypeData::Intersection(list_id)) => Some(TypeIdList::new(db.type_list(list_id))),
        _ => None,
    }
}

/// Apply a mapping function to each member of a union or intersection type,
/// reconstructing the compound type from the mapped results.
///
/// If the type is a union, maps each member and rebuilds a union.
/// If the type is an intersection, maps each member and rebuilds an intersection.
/// If the type is neither, returns `None` (the caller should handle the non-compound case).
///
/// This eliminates the common checker anti-pattern of:
/// ```text
/// if let Some(members) = get_union_members(db, ty) {
///     let mapped: Vec<_> = members.into_iter().map(|m| transform(m)).collect();
///     factory.union(mapped)
/// } else if let Some(members) = get_intersection_members(db, ty) {
///     let mapped: Vec<_> = members.into_iter().map(|m| transform(m)).collect();
///     factory.intersection(mapped)
/// }
/// ```
pub fn map_compound_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut f: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            Some(db.union(mapped))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            Some(db.intersection(mapped))
        }
        _ => None,
    }
}

/// Like [`map_compound_members`], but only reconstructs the compound type if at least
/// one member was changed by the mapping function. Returns the original `type_id`
/// unchanged if all mapped members are identical to the originals.
///
/// Returns `None` if the type is not a union or intersection.
pub fn map_compound_members_if_changed(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut f: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            if mapped.iter().eq(members.iter()) {
                Some(type_id)
            } else {
                Some(db.union(mapped))
            }
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            if mapped.iter().eq(members.iter()) {
                Some(type_id)
            } else {
                Some(db.intersection(mapped))
            }
        }
        _ => None,
    }
}

/// Get the element type of an array.
///
/// Returns None if the type is not an array.
pub fn get_array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Array(element_type)) => Some(element_type),
        // `readonly T[]` wraps the array in ReadonlyType — unwrap and retry.
        Some(TypeData::ReadonlyType(inner)) => get_array_element_type(db, inner),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .and_then(|constraint| get_array_element_type(db, constraint)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            (evaluated != type_id)
                .then(|| get_array_element_type(db, evaluated))
                .flatten()
        }
        _ => None,
    }
}

/// Return true when a constraint admits a mutable array or tuple candidate.
///
/// Const type parameters preserve literal types, but when their declared
/// constraint is mutable-array-like (`T extends unknown[]`, or a union with a
/// mutable array member), array literal candidates must not be converted to
/// readonly tuples.
pub fn constraint_allows_mutable_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }

    match db.lookup(type_id) {
        Some(TypeData::Array(_)) => true,
        Some(TypeData::Tuple(list_id)) => !db.tuple_list(list_id).is_empty(),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| constraint_allows_mutable_array_like(db, constraint)),
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| constraint_allows_mutable_array_like(db, member)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            evaluated != type_id && constraint_allows_mutable_array_like(db, evaluated)
        }
        _ => false,
    }
}

/// Get the element type for mutable array forms that are identical for TS2403.
///
/// This intentionally recognizes `T[]` and canonical `Array<T>` applications
/// before application evaluation erases the as-written `Array<T>` identity.
pub fn mutable_array_element_for_redeclaration(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    array_base: Option<TypeId>,
    definition_store: Option<&DefinitionStore>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }

    match db.lookup(type_id) {
        Some(TypeData::Array(elem)) => Some(elem),
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            (is_array_application_base_for_redeclaration(
                db,
                app.base,
                array_base,
                definition_store,
            ) && app.args.len() == 1)
                .then_some(app.args[0])
        }
        _ => None,
    }
}

fn is_array_application_base_for_redeclaration(
    db: &dyn TypeDatabase,
    base: TypeId,
    array_base: Option<TypeId>,
    definition_store: Option<&DefinitionStore>,
) -> bool {
    let array_base = array_base.or_else(|| db.get_array_base_type());
    let array_display_base = db.get_array_display_base_type();
    if array_base == Some(base)
        || array_display_base.is_some_and(|display_base| display_base == base)
    {
        return true;
    }

    db.get_display_alias(base).is_some_and(|alias| {
        array_base == Some(alias)
            || array_display_base.is_some_and(|display_base| display_base == alias)
    }) || lazy_base_names_array(db, definition_store, base)
}

fn lazy_base_names_array(
    db: &dyn TypeDatabase,
    definition_store: Option<&DefinitionStore>,
    base: TypeId,
) -> bool {
    let (Some(definition_store), Some(TypeData::Lazy(def_id))) =
        (definition_store, db.lookup(base))
    else {
        return false;
    };

    definition_store
        .get(def_id)
        .is_some_and(|def| db.resolve_atom_ref(def.name).as_ref() == "Array")
}

/// Get the elements of a tuple type.
///
/// Returns None if the type is not a tuple.
/// Returns a vector of (`TypeId`, optional, rest, name) tuples.
pub fn get_tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::TupleElement>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = db.tuple_list(list_id);
            Some(elements.to_vec())
        }
        // `readonly [A, B]` is wrapped in ReadonlyType — unwrap and retry.
        Some(TypeData::ReadonlyType(inner)) => get_tuple_elements(db, inner),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .and_then(|constraint| get_tuple_elements(db, constraint)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            (evaluated != type_id)
                .then(|| get_tuple_elements(db, evaluated))
                .flatten()
        }
        // Intersection of tuples: pick the tuple member with the most specific elements.
        // e.g., `[any] & [1]` should provide tuple context from `[1]` (more specific).
        // If multiple tuple members exist, prefer the one whose elements are not `any`.
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mut best: Option<Vec<crate::types::TupleElement>> = None;
            for &m in members.iter() {
                if let Some(elems) = get_tuple_elements(db, m)
                    && (best.is_none() || elems.iter().any(|e| e.type_id != TypeId::ANY))
                {
                    best = Some(elems);
                }
            }
            best
        }
        _ => None,
    }
}

/// Check if a type is a union containing at least one tuple member.
///
/// This detects the `T extends readonly unknown[] | []` pattern where `| []`
/// is a deliberate hint in TypeScript to infer tuple types from array literals.
/// Used by `Promise.all`, `Promise.allSettled`, and similar APIs.
pub fn union_contains_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| get_tuple_elements(db, m).is_some())
        }
        _ => false,
    }
}

/// Check if a union type has a direct `TypeParameter` or Infer member (not nested).
///
/// Returns true for `string | T` or `number | infer U`, false for
/// `string | MyInterface` even if `MyInterface` contains type parameters internally.
/// Used to suppress diagnostics when generic type parameters are directly present.
pub fn union_has_direct_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| {
                !m.is_intrinsic()
                    && matches!(
                        db.lookup(m),
                        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                    )
            })
        }
        _ => false,
    }
}

#[cfg(test)]
mod conditional_through_aliases_tests {
    use super::contains_conditional_through_aliases;
    use crate::construction::TypeInterner;
    use crate::def::DefId;
    use crate::types::{ConditionalType, FunctionShape, PropertyInfo, TypeId};

    // A generic interface body shaped like `{ m(): void; v?: T }`: a method
    // member plus a data member, no conditional anywhere. This is the #13554
    // case that must be consumable cross-file, so the gate must report `false`.
    #[test]
    fn plain_method_object_body_has_no_conditional() {
        let interner = TypeInterner::new();
        let m = interner.intern_string("m");
        let v = interner.intern_string("v");
        let method = interner.function(FunctionShape::new(vec![], TypeId::VOID));
        let body = interner.object(vec![
            PropertyInfo::method(m, method),
            PropertyInfo::opt(v, TypeId::NUMBER),
        ]);
        let mut resolve = |_: DefId| None;
        assert!(!contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));
    }

    // A method whose return type applies an alias whose body is a conditional
    // (`read(): MappedResponseType<R, T>`). The standard content walk treats
    // the application base as an opaque leaf, so resolution must follow the
    // alias to find the conditional. Detected through both the object property
    // and the applied alias.
    #[test]
    fn method_returning_alias_to_conditional_is_detected() {
        let interner = TypeInterner::new();
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let def = DefId(7);
        let alias_app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let method = interner.function(FunctionShape::new(vec![], alias_app));
        let read = interner.intern_string("read");
        let body = interner.object(vec![PropertyInfo::method(read, method)]);

        let mut resolve = |d: DefId| (d == def).then_some(cond);
        assert!(contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));

        // When the alias body is unavailable, the conditional behind it cannot
        // be observed and the body is treated as inert (no false gating).
        let mut unresolved = |_: DefId| None;
        assert!(!contains_conditional_through_aliases(
            &interner,
            body,
            &mut unresolved
        ));
    }

    // A directly-present conditional member is detected without alias
    // resolution.
    #[test]
    fn direct_conditional_member_is_detected() {
        let interner = TypeInterner::new();
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let p = interner.intern_string("p");
        let body = interner.object(vec![PropertyInfo::new(p, cond)]);
        let mut resolve = |_: DefId| None;
        assert!(contains_conditional_through_aliases(
            &interner,
            body,
            &mut resolve
        ));
    }
}
