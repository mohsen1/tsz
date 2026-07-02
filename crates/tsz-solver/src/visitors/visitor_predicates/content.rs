//! Deep content predicate walkers for solver `TypeData` graphs.
//!
//! Every deep walker here is a thin driver over the canonical
//! policy-parameterized child enumerator in [`crate::visitors::child_policy`]:
//! memoization, recursion guards, and short-circuiting are per-driver; the
//! child set each walker descends into is an explicit [`ChildPolicy`].

use std::ops::ControlFlow;

use crate::construction::TypeDatabase;
use crate::types::IntrinsicKind;
use crate::visitors::child_policy::{
    ChildPolicy, for_each_child_with_policy, has_policy_children, try_for_each_child_with_policy,
};
use crate::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

use super::predicate_pool::with_predicate_buffers;

/// Check if a type contains any type parameters.
///
/// This is the legacy narrow predicate: it matches `TypeParameter | Infer`,
/// but not `ThisType` or `BoundParameter`. The deep walk is memoized per node
/// in the project-wide param-or-infer cache so recursive instantiation and
/// conditional evaluation do not re-walk shared subtrees.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::type_queries::contains_param_or_infer_db(types, type_id)
}

/// Check if a type contains free type parameters, excluding those bound by
/// enclosing function/callable signatures. See `contains_free_type_parameters_db`
/// in `content_predicates` for the full doc.
///
/// The deep FREE-policy walk is memoized per node in the project-wide
/// free-type-parameter cache (the answer is immutable per `TypeId` within one
/// interner), so repeated checks over shared closed subtrees stay O(1). This
/// mirrors how `contains_type_parameters` delegates to the cached
/// `contains_param_or_infer_db`; the hot `resolve_operands` conditional gate
/// asks this twice per node (#13250).
pub fn contains_free_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::type_queries::contains_free_type_parameters_db(types, type_id)
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains any "free" `infer` types — inference placeholders
/// that are NOT buried inside a `TypeParameter`'s constraint or default.
///
/// `TypeParameter` constraints/defaults are definitional (e.g., `T extends Foo`
/// where `Foo = X extends Bar<infer V> ? V : never`). The `infer V` there is
/// structural and already resolved at the definition site. Walking into it
/// produces false positives when used to decide whether to suppress diagnostics.
///
/// This variant is used by `should_suppress_assignability_diagnostic` to avoid
/// suppressing real errors like TS2322 when the only `infer` types are in
/// type parameter constraint chains.
pub fn contains_free_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    DeepContainsChecker::new(types, ChildPolicy::FREE_INFER, |key| {
        matches!(key, TypeData::Infer(_))
    })
    .check_from_root(type_id)
}

/// Check if a type contains the `any` intrinsic anywhere.
pub fn contains_any_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY {
        return true;
    }
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::Intrinsic(IntrinsicKind::Any))
    })
}

/// Check if a type contains the error type anywhere in its structure.
///
/// Structural rule: a type contains an error iff any node reachable through
/// its *committed* structural use surface — including `Application` bases,
/// property write types, and index-signature keys, but excluding
/// type-parameter declaration metadata and the operands of deferred
/// type-level operations (conditional/mapped/indexed-access/`keyof`/template
/// branches are unevaluated alternatives, not committed structure) — is the
/// `TypeId::ERROR` sentinel, `TypeData::Error`, or an `UnresolvedTypeName`.
/// The sentinel is matched before the intrinsic fast path (it sits in the
/// intrinsic id range), which the generic `contains_type_matching` walk
/// cannot do.
///
/// This is the single canonical error-containment answer; the checker-facing
/// `contains_error_type_db` delegates here so both query paths agree.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    DeepContainsChecker::new(types, ChildPolicy::ERROR_CONTAINMENT, |key| {
        matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_))
    })
    .with_sentinel(TypeId::ERROR)
    .check_from_root(type_id)
}

/// Check if a type contains the `this` type anywhere.
///
/// The result is stable per `TypeId` within a single `TypeInterner`, so we
/// memoize in a project-wide `DashMap` on the interner to avoid the repeated
/// recursive walk that profiled at ~5% of total CPU on multi-file workloads.
#[inline]
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types never contain ThisType
    if type_id.is_intrinsic() {
        return false;
    }
    // The deep walk is memoized per node in the shared `contains_this` cache,
    // so repeated checks over the same large recursive shapes stay O(1).
    crate::type_queries::contains_this_type_db(types, type_id)
}

/// Check if a type contains any type matching a predicate.
///
/// Descends the [`ChildPolicy::CONTENT_PREDICATE`] child set: notably,
/// `Application` bases are not visited (the base definition's own type
/// parameters are bound by the application's arguments, so e.g. `A<number>`
/// is concrete even though `A`'s definition contains `TypeParameter T`).
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    DeepContainsChecker::new(types, ChildPolicy::CONTENT_PREDICATE, predicate)
        .check_from_root(type_id)
}

/// Check if a type contains a type parameter with the given name.
///
/// This is a convenience wrapper around `contains_type_matching` that avoids
/// requiring callers to match on `TypeData` internals directly.
pub fn contains_type_parameter_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if info.name == name),
    )
}

/// Check if a type contains a type parameter with the given name, WITHOUT
/// walking into other type parameters' constraints.
///
/// Unlike `contains_type_parameter_named`, this does not descend into
/// `TypeParameter.constraint` or `TypeParameter.default`. This is important
/// for mapped type circular-constraint detection: in `{ [K in keyof T]: T[K] }`,
/// `K`'s constraint is `keyof T`. The deep check would walk into `T`'s own
/// constraint (which may contain `K`), falsely reporting a cycle.
pub fn contains_type_parameter_named_shallow(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    worklist_contains_matching(
        types,
        type_id,
        &ChildPolicy::SHALLOW,
        |_, data| matches!(data, Some(TypeData::TypeParameter(info)) if info.name == name),
    )
}

/// Pooled-buffer worklist containment check over `policy`'s child set:
/// returns `true` when `node_matches` holds for any reachable node. The
/// per-walker leaf rule (e.g. "a bare `TypeParameter` is a leaf") lives in
/// `policy`; `node_matches` receives the node's id and looked-up data so both
/// id-level and shape-level predicates can drive it.
fn worklist_contains_matching(
    types: &dyn TypeDatabase,
    root: TypeId,
    policy: &ChildPolicy,
    mut node_matches: impl FnMut(TypeId, Option<&TypeData>) -> bool,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(root);
        while let Some(current) = stack.pop() {
            match PredicateWorklistVisitState::enter(current, visited) {
                PredicateWorklistVisitState::Entered => {}
                PredicateWorklistVisitState::IgnoredIntrinsic
                | PredicateWorklistVisitState::AlreadyVisited => continue,
            }
            let data = types.lookup(current);
            if node_matches(current, data.as_ref()) {
                return true;
            }
            let Some(data) = data else {
                continue;
            };
            for_each_child_with_policy(types, &data, policy, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PredicateWorklistVisitState {
    Entered,
    AlreadyVisited,
    IgnoredIntrinsic,
}

impl PredicateWorklistVisitState {
    fn enter(current: TypeId, visited: &mut FxHashSet<TypeId>) -> Self {
        if current.is_intrinsic() {
            Self::IgnoredIntrinsic
        } else if visited.insert(current) {
            Self::Entered
        } else {
            Self::AlreadyVisited
        }
    }
}

fn type_parameter_identity_matches(
    def_store: &crate::def::DefinitionStore,
    candidate: TypeId,
    target: TypeId,
) -> bool {
    candidate == target
        || def_store
            .find_def_for_type(candidate)
            .zip(def_store.find_def_for_type(target))
            .is_some_and(|(candidate_def, target_def)| candidate_def == target_def)
}

/// Check if a type contains the target type parameter identity, without walking
/// into other type parameters' constraints/defaults.
pub fn contains_type_parameter_identity_shallow(
    types: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    worklist_contains_matching(types, type_id, &ChildPolicy::SHALLOW, |current, _| {
        type_parameter_identity_matches(def_store, current, target)
    })
}

/// Check if a type transitively references any type parameter whose name
/// is in the given set.
///
/// This is more efficient than `collect_referenced_types` followed by
/// per-element `type_param_info` checks, because it short-circuits on
/// the first match.
pub fn references_any_type_param_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    names: &rustc_hash::FxHashSet<Atom>,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if names.contains(&info.name)),
    )
}

/// Check if a type's full structural surface references any `TypeParameter`/
/// `Infer` whose interned `TypeId` is *not* one of `local_param_ids`.
///
/// This is the short-circuiting, allocation-free replacement for the former
/// `collect_all_types(..).into_iter().any(..)` pattern in the generic
/// function-subtype rules: it answers "does this signature position mention a
/// type parameter outside its own locally-bound set?". The reachability is the
/// same `ChildPolicy::EVERYTHING` surface `collect_all_types` walks (generic
/// signature bodies, type-parameter constraints, and defaults all included),
/// and the positive match is the same `TypeData::TypeParameter | Infer` test
/// the old code applied via `type_param_info`, so the boolean answer is
/// identical — but the worklist stops at the first non-local occurrence and
/// reuses pooled buffers instead of materializing the full reachable set into a
/// fresh `FxHashSet` per query.
pub fn references_type_param_outside_id_set(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    local_param_ids: &rustc_hash::FxHashSet<TypeId>,
) -> bool {
    worklist_contains_matching(types, type_id, &ChildPolicy::EVERYTHING, |id, data| {
        matches!(data, Some(TypeData::TypeParameter(_) | TypeData::Infer(_)))
            && !local_param_ids.contains(&id)
    })
}

/// Check whether any `Mapped` node in `type_id`'s full structural surface has a
/// `constraint`, `template`, or `name_type` that references the type parameter
/// named `param_name`.
///
/// Short-circuiting, allocation-free replacement for the former
/// `collect_all_types(..).into_iter().any(|c| matches Mapped && ..)` pattern in
/// the generic function-subtype rules. The reachability is the same
/// `ChildPolicy::EVERYTHING` surface `collect_all_types` walked, and the
/// per-`Mapped` predicate is identical, so the boolean answer matches; the
/// worklist stops at the first qualifying mapped node and reuses pooled buffers
/// rather than materializing the full reachable set.
pub fn mapped_context_references_type_param_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: Atom,
) -> bool {
    worklist_contains_matching(types, type_id, &ChildPolicy::EVERYTHING, |_, data| {
        let Some(TypeData::Mapped(mapped_id)) = data else {
            return false;
        };
        let mapped = types.get_mapped(*mapped_id);
        contains_type_parameter_named(types, mapped.constraint, param_name)
            || contains_type_parameter_named(types, mapped.template, param_name)
            || mapped.name_type.is_some_and(|name_type| {
                contains_type_parameter_named(types, name_type, param_name)
            })
    })
}

/// Check if a constraint type references a type parameter along the base-constraint
/// resolution path. This mimics tsc's `getBaseConstraint` recursion, which only
/// follows certain structural paths:
///
/// Descended into (these require resolving sub-constraints):
/// - Union/intersection members
/// - Mapped type constraint (the key source)
/// - Conditional check/extends types
/// - Index access object/index
/// - `KeyOf` operand
///
/// NOT descended into (these are type references/wrappers — tsc treats them as opaque):
/// - Type application arguments (e.g. `Foo<T>`)
/// - Array/Tuple/ReadonlyType/NoInfer inner types (these are effectively type references)
/// - Object property types
/// - Function parameter/return types
///
/// This avoids false positives: `T extends Array<T>` is NOT circular,
/// but `T extends { [P in T]: number }` IS circular.
pub fn constraint_references_type_param_in_resolution_path(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: Atom,
) -> bool {
    resolution_path_contains_matching(
        types,
        type_id,
        |_, data| matches!(data, Some(TypeData::TypeParameter(info)) if info.name == param_name),
    )
}

/// Identity-based variant of `constraint_references_type_param_in_resolution_path`.
pub fn constraint_references_type_param_identity_in_resolution_path(
    types: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    resolution_path_contains_matching(types, type_id, |current, _| {
        type_parameter_identity_matches(def_store, current, target)
    })
}

/// Worklist containment check over the constraint-resolution child set:
/// union/intersection members, the mapped-type `constraint` (key source —
/// catching `T extends { [P in T]: number }` without false-positiving on
/// `T extends { [K in keyof T]: V }`, since `KeyOf` is not followed), and
/// indexed-access operands (`T extends Foo | T["hello"]`). Everything else
/// (`KeyOf`, `Conditional`, `Application`, `Object`, `Function`, `Array`,
/// `Tuple`, `ReadonlyType`, `NoInfer`, …) is opaque at the
/// constraint-resolution level: `T extends Array<T>` is NOT circular.
///
/// This child set is deliberately narrower than any [`ChildPolicy`] — it
/// mimics tsc's `getBaseConstraint` recursion, not structural traversal.
fn resolution_path_contains_matching(
    types: &dyn TypeDatabase,
    root: TypeId,
    mut node_matches: impl FnMut(TypeId, Option<&TypeData>) -> bool,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(root);
        while let Some(current) = stack.pop() {
            match PredicateWorklistVisitState::enter(current, visited) {
                PredicateWorklistVisitState::Entered => {}
                PredicateWorklistVisitState::IgnoredIntrinsic
                | PredicateWorklistVisitState::AlreadyVisited => continue,
            }
            let data = types.lookup(current);
            if node_matches(current, data.as_ref()) {
                return true;
            }
            match &data {
                Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
                    stack.extend(types.type_list(*list_id).iter().copied());
                }
                Some(TypeData::Mapped(mapped_id)) => {
                    stack.push(types.get_mapped(*mapped_id).constraint);
                }
                Some(TypeData::IndexAccess(obj, idx)) => {
                    stack.push(*obj);
                    stack.push(*idx);
                }
                _ => {}
            }
        }
        false
    })
}

/// Check if a type transitively contains a specific `TypeId`.
///
/// This is more efficient than `collect_referenced_types(…).contains(&target)`
/// because it short-circuits as soon as the target is found.
pub fn contains_type_by_id(types: &dyn TypeDatabase, root: TypeId, target: TypeId) -> bool {
    if root == target {
        return true;
    }
    let mut visited = FxHashSet::default();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current == target {
            return true;
        }
        match ContainsTypeByIdVisitState::enter(current, &mut visited) {
            ContainsTypeByIdVisitState::Entered => {}
            ContainsTypeByIdVisitState::AlreadyVisited => continue,
        }
        crate::visitors::visitor::for_each_child_by_id(types, current, |child| {
            if !visited.contains(&child) {
                stack.push(child);
            }
        });
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainsTypeByIdVisitState {
    Entered,
    AlreadyVisited,
}

impl ContainsTypeByIdVisitState {
    fn enter(current: TypeId, visited: &mut FxHashSet<TypeId>) -> Self {
        if visited.insert(current) {
            Self::Entered
        } else {
            Self::AlreadyVisited
        }
    }
}

/// The single deep boolean-containment driver behind every `contains_*`
/// predicate walk: memoized, recursion-guarded, short-circuiting. The child
/// set it descends into is the explicit [`ChildPolicy`]; the positive match is
/// the `predicate` over each visited node's `TypeData` (checked before the
/// terminal fast path, so leaf kinds can still match).
struct DeepContainsChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    types: &'a dyn TypeDatabase,
    policy: ChildPolicy,
    predicate: F,
    /// Intrinsic-range sentinel id (e.g. `TypeId::ERROR`) matched before the
    /// intrinsic fast path. Sentinel ids live in the intrinsic id range, where
    /// `lookup`-based predicates are never consulted, so an id-level match is
    /// the only way a walk can detect them when nested.
    sentinel: Option<TypeId>,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> DeepContainsChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    fn new(types: &'a dyn TypeDatabase, policy: ChildPolicy, predicate: F) -> Self {
        Self {
            types,
            policy,
            predicate,
            sentinel: None,
            memo: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::ShallowTraversal,
            ),
        }
    }

    const fn with_sentinel(mut self, sentinel: TypeId) -> Self {
        self.sentinel = Some(sentinel);
        self
    }

    #[cfg(test)]
    fn memo_entries(&self) -> usize {
        self.memo.len()
    }

    /// Entry point: like [`Self::check`], but leaf roots return without
    /// touching the memo — the checker is discarded right after, so the
    /// common leaf-root query stays allocation-free (`FxHashMap` only
    /// allocates on first insert).
    fn check_from_root(mut self, type_id: TypeId) -> bool {
        if self.sentinel == Some(type_id) {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if (self.predicate)(&key) {
            return true;
        }
        if !has_policy_children(&key, &self.policy) {
            return false;
        }
        // The root result is not memoized: the checker is dropped on return,
        // so the write (and, for shallow shapes, the memo's only allocation)
        // would be dead. Children memoize normally inside `walk_children`.
        self.walk_children(type_id, &key)
    }

    fn check(&mut self, type_id: TypeId) -> bool {
        if self.sentinel == Some(type_id) {
            return true;
        }
        // Fast path: intrinsic types (primitives, any, never, etc.) have no
        // subtypes and can never contain nested type structures.
        if type_id.is_intrinsic() {
            return false;
        }

        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }

        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };

        if (self.predicate)(&key) {
            self.memo.insert(type_id, true);
            return true;
        }

        // Terminal fast path: a node with no children under this walker's
        // policy cannot match below itself. Skipping the recursion guard's
        // enter/leave HashSet round-trip is a pure win; the memo is still
        // updated so repeat visits within one walk stay O(1).
        if !has_policy_children(&key, &self.policy) {
            self.memo.insert(type_id, false);
            return false;
        }

        let result = self.walk_children(type_id, &key);
        self.memo.insert(type_id, result);
        result
    }

    /// Recursion-guarded descent into `type_id`'s children under the walker's
    /// policy. `key` is the node's already-fetched data.
    fn walk_children(&mut self, type_id: TypeId, key: &TypeData) -> bool {
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let types = self.types;
        let policy = self.policy;
        let result = try_for_each_child_with_policy::<(), _>(types, key, &policy, &mut |child| {
            if self.check(child) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_break();

        self.guard.leave(type_id);
        result
    }
}

/// Collect the `TypeId`s of every `TypeParameter`/`Infer` that occurs *free*
/// across `roots`, i.e. not bound by an enclosing generic function/callable
/// signature.
///
/// Type parameters in tsz are interned structurally by name, so two unrelated
/// `T`s share a `TypeId`. Callers that compare against a specific parameter's
/// identity must therefore only consider free occurrences: a parameter *bound*
/// by a nested generic signature in the traversed type is a distinct
/// declaration that merely shares an interned name, and must not be reported.
///
/// This mirrors [`contains_free_type_parameters`]'s binder handling — the body of a
/// nested generic signature is skipped wholesale rather than re-scoped — but
/// returns the set of free parameter ids instead of a boolean. The result is a
/// pure function of the input types and is memoized across all `roots` in a
/// single pass (so e.g. a signature's parameters and return type share the
/// walk); the walk is `stacker`-guarded because it runs inside the (already
/// deep) subtype relation.
///
/// A `TypeParameter`'s `constraint`/`default` are metadata, not free uses, so
/// they are not traversed — matching the rest of the free-parameter walkers.
pub fn free_type_parameter_ids_in(
    types: &dyn TypeDatabase,
    roots: impl IntoIterator<Item = TypeId>,
) -> FxHashSet<TypeId> {
    let mut collector = FreeTypeParamCollector {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    let mut out = FxHashSet::default();
    for root in roots {
        out.extend(collector.free(root));
    }
    out
}

struct FreeTypeParamCollector<'a> {
    types: &'a dyn TypeDatabase,
    /// Memoized free-parameter set per `TypeId`. Freeness is a pure function of
    /// the type (a generic signature contributes nothing; everything else is the
    /// union of its children), so it can be cached without an ambient scope.
    /// Memoization also keeps the walk linear over a shared type DAG — without it
    /// a signature reused across many positions would be re-expanded and can
    /// overflow the stack on real-world types.
    memo: FxHashMap<TypeId, FxHashSet<TypeId>>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeTypeParamCollector<'a> {
    fn free(&mut self, type_id: TypeId) -> FxHashSet<TypeId> {
        if type_id.is_intrinsic() {
            return FxHashSet::default();
        }
        if let Some(cached) = self.memo.get(&type_id) {
            return cached.clone();
        }
        let Some(key) = self.types.lookup(type_id) else {
            return FxHashSet::default();
        };
        // A `TypeParameter`/`Infer` is a free occurrence and a leaf: answer
        // directly, skipping the recursion-guard/stacker bookkeeping.
        if matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_)) {
            let mut set = FxHashSet::default();
            set.insert(type_id);
            self.memo.insert(type_id, set.clone());
            return set;
        }
        // Terminal-kind fast path: variants with no children under this
        // walker's policy contribute no free parameters, so skip the
        // recursion-guard/memo bookkeeping entirely.
        if !has_policy_children(&key, &ChildPolicy::FREE_PARAM_COLLECT) {
            return FxHashSet::default();
        }
        // Cycle back-edges contribute no new free parameters (the parameter is
        // already reachable from the ancestor on the stack), so return empty on
        // re-entry and do not memoize the partial result.
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return FxHashSet::default(),
        }
        // Grow the native stack on demand: this walk runs *inside* the already
        // deep subtype-relation recursion, so a deeply nested type can otherwise
        // overflow. Mirrors `RecursiveTypeCollector::visit`.
        let result = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || self.free_key(&key));
        self.guard.leave(type_id);
        self.memo.insert(type_id, result.clone());
        result
    }

    /// Free parameters of one node: the union of its children's free
    /// parameters under [`ChildPolicy::FREE_PARAM_COLLECT`]. A *generic*
    /// signature binds its own type parameters, so its body is skipped
    /// wholesale. This intentionally does not descend into a generic
    /// signature to recover an outer parameter threaded through it; that
    /// extra precision is unnecessary for the identity-sharing decision this
    /// helper drives, and descending makes the walk dramatically deeper on
    /// real-world recursive signature graphs. (`TypeParameter`/`Infer` leaves
    /// are answered in [`Self::free`] before this is reached.)
    fn free_key(&mut self, key: &TypeData) -> FxHashSet<TypeId> {
        let mut set = FxHashSet::default();
        let types = self.types;
        for_each_child_with_policy(types, key, &ChildPolicy::FREE_PARAM_COLLECT, |child| {
            set.extend(self.free(child))
        });
        set
    }
}

/// Check whether `type_id` contains a *free* reference to a type parameter
/// other than `excluded_name`, treating each `TypeParameter`/`Infer` as a leaf.
///
/// A `TypeParameter`'s `constraint`/`default` are metadata, not uses:
/// the iteration variable `K` in `{ [K in keyof T as ...]: T[K] }` carries
/// a stale `keyof T` constraint after `T` is substituted, since the `K`
/// instances inside the body still reference the pre-substitution record.
pub fn contains_free_type_parameters_except_name(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    excluded_name: Atom,
) -> bool {
    worklist_contains_matching(
        types,
        type_id,
        &ChildPolicy::STRUCTURAL_USES_SHALLOW,
        |_, data| match data {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.name != excluded_name
            }
            Some(TypeData::ThisType | TypeData::BoundParameter(_)) => true,
            _ => false,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::{TupleElement, TypeParamInfo};

    #[test]
    fn predicate_worklist_visit_state_names_intrinsic_entered_and_revisit() {
        let interner = TypeInterner::new();
        let type_id = interner.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            PredicateWorklistVisitState::enter(TypeId::ANY, &mut visited),
            PredicateWorklistVisitState::IgnoredIntrinsic
        );
        assert!(visited.is_empty());
        assert_eq!(
            PredicateWorklistVisitState::enter(type_id, &mut visited),
            PredicateWorklistVisitState::Entered
        );
        assert_eq!(
            PredicateWorklistVisitState::enter(type_id, &mut visited),
            PredicateWorklistVisitState::AlreadyVisited
        );
    }

    #[test]
    fn contains_type_by_id_visit_state_names_entered_and_revisit() {
        let interner = TypeInterner::new();
        let type_id = interner.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            ContainsTypeByIdVisitState::enter(type_id, &mut visited),
            ContainsTypeByIdVisitState::Entered
        );
        assert_eq!(
            ContainsTypeByIdVisitState::enter(type_id, &mut visited),
            ContainsTypeByIdVisitState::AlreadyVisited
        );
    }

    #[test]
    fn contains_type_by_id_handles_shared_child_once() {
        let interner = TypeInterner::new();
        let child = interner.object(vec![]);
        let root = interner.tuple(vec![TupleElement::fixed(child), TupleElement::fixed(child)]);

        assert!(contains_type_by_id(&interner, root, child));
        assert!(!contains_type_by_id(&interner, root, TypeId::STRING));
    }

    #[test]
    fn predicate_checker_memo_entry_counts_are_observable() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));
        let u_infer = interner.infer(TypeParamInfo::simple(u_name));
        let wrapper = interner.readonly_type(t_param);

        let mut contains_checker =
            DeepContainsChecker::new(&interner, ChildPolicy::CONTENT_PREDICATE, |key| {
                matches!(key, TypeData::TypeParameter(_))
            });
        assert!(contains_checker.check(wrapper));
        assert!(contains_checker.memo_entries() > 0);

        assert!(contains_free_type_parameters(&interner, wrapper));
        assert!(contains_free_infer_types(&interner, u_infer));
        assert!(!contains_free_infer_types(&interner, wrapper));
    }

    /// `contains_free_infer_types` must not treat structural `infer` patterns
    /// inside a `TypeParameter`'s constraint as live inference variables, while
    /// the generic deep walk does descend into constraints.
    #[test]
    fn free_infer_policy_skips_type_param_constraints() {
        let interner = TypeInterner::new();
        let v_name = interner.intern_string("V");
        let t_name = interner.intern_string("T");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        let constrained = interner.type_param(TypeParamInfo {
            constraint: Some(infer_v),
            ..TypeParamInfo::simple(t_name)
        });
        let wrapper = interner.readonly_type(constrained);

        assert!(!contains_free_infer_types(&interner, wrapper));
        assert!(contains_infer_types(&interner, wrapper));
    }

    /// `contains_free_infer_types` must not treat an `infer` declared inside a
    /// conditional's `extends` clause as a live inference variable: it is bound
    /// by that conditional and is part of a stable deferred type (e.g. the
    /// declared return type of a method). Counting it made `Box<string>`
    /// (whose method `m` returns `U extends Promise<infer V> ? …`) look like it
    /// held a transient inference placeholder, suppressing real `TS2322`/`TS2345`
    /// diagnostics. A bare/root `infer` stays free.
    #[test]
    fn free_infer_policy_skips_conditional_bound_infer() {
        use crate::types::ConditionalType;
        let interner = TypeInterner::new();
        let v_name = interner.intern_string("V");
        let t_name = interner.intern_string("T");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));

        // `T extends infer V ? 1 : 2` — `infer V` is bound by the conditional.
        let cond = interner.conditional(ConditionalType {
            check_type: t_param,
            extends_type: infer_v,
            true_type: TypeId::NUMBER,
            false_type: TypeId::NUMBER,
            is_distributive: false,
        });
        let wrapper = interner.readonly_type(cond);

        assert!(
            !contains_free_infer_types(&interner, wrapper),
            "an `infer` bound by a conditional must not count as a free infer"
        );
        // The generic deep walk still sees the structural `infer` node.
        assert!(contains_infer_types(&interner, wrapper));
        // A bare `infer` is still free.
        assert!(contains_free_infer_types(&interner, infer_v));
    }

    /// Free-type-parameter checks skip the bodies of generic signatures (their
    /// parameters are bound), but still see free parameters in non-generic
    /// signature bodies.
    #[test]
    fn free_type_param_policy_skips_generic_signature_bodies() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));

        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(t_name)],
            ..crate::types::FunctionShape::new(vec![], t_param)
        });
        assert!(!contains_free_type_parameters(&interner, generic_fn));
        assert!(contains_type_parameters(&interner, generic_fn));

        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], t_param));
        assert!(contains_free_type_parameters(&interner, plain_fn));
    }

    /// An `infer V` declared inside a conditional's `extends` clause is a
    /// definitional binder scoped to that conditional — never a live transient
    /// inference placeholder — regardless of whether the enclosing signature is
    /// generic. The `FREE_INFER` policy encodes this with `deferred_operations:
    /// false`: the walk stops at a deferred conditional/mapped/indexed/`keyof`
    /// node, so an `infer` reachable only through such an operand is not
    /// reported. Reporting it would wrongly route
    /// `should_suppress_assignability_diagnostic` into suppressing a real
    /// `TS2322`/`TS2345` (issue #14784).
    ///
    /// This mirrors the sibling `free_infer_policy_skips_conditional_bound_infer`
    /// (which wraps the same conditional in a `readonly` type): the container —
    /// generic signature, non-generic signature, or `readonly` — does not change
    /// the answer, because the conditional itself is the binder.
    #[test]
    fn free_infer_policy_skips_conditional_bound_infer_in_signature_bodies() {
        let interner = TypeInterner::new();
        let u_name = interner.intern_string("U");
        let v_name = interner.intern_string("V");
        let u_param = interner.type_param(TypeParamInfo::simple(u_name));
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        // `U extends infer V ? U : U` — a deferred conditional whose `extends`
        // declares `infer V`.
        let cond = interner.conditional(crate::types::ConditionalType {
            check_type: u_param,
            extends_type: infer_v,
            true_type: u_param,
            false_type: u_param,
            is_distributive: false,
        });

        // A *generic* signature binds both its own `U` and the conditional's
        // `infer V`; the free-infer walk treats the whole body as bound.
        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(u_name)],
            ..crate::types::FunctionShape::new(vec![], cond)
        });
        assert!(!contains_free_infer_types(&interner, generic_fn));
        // The structural `infer` is still observable to the un-scoped walk.
        assert!(contains_infer_types(&interner, generic_fn));

        // A *non-generic* signature body carrying the same conditional is ALSO
        // not free-infer-bearing: the `infer V` is bound by the conditional, not
        // by any enclosing signature. Classifying it as free would reintroduce
        // the #14784 false negative (a stable deferred conditional type is not a
        // live inference session, so its assignability diagnostics must stand).
        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], cond));
        assert!(!contains_free_infer_types(&interner, plain_fn));
        // The un-scoped walk still sees the structural `infer`.
        assert!(contains_infer_types(&interner, plain_fn));
    }

    /// Isolates `skip_generic_signature_bodies` from `deferred_operations`: a
    /// *genuinely free* `infer` placed as a bare (non-deferred) return type is a
    /// live placeholder and IS observed on a non-generic signature, but a
    /// generic signature binds its whole body and hides it. This is the only
    /// child position where `skip_generic_signature_bodies` acts independently —
    /// `deferred_operations: false` alone does not gate a non-deferred child.
    #[test]
    fn free_infer_policy_skips_generic_signature_bodies() {
        let interner = TypeInterner::new();
        let u_name = interner.intern_string("U");
        let v_name = interner.intern_string("V");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));

        // Non-generic signature returning a bare structural `infer` → the free
        // `infer` is reachable as a direct (non-deferred) child and is reported.
        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], infer_v));
        assert!(contains_free_infer_types(&interner, plain_fn));

        // A generic signature binds its whole body, so the same bare `infer`
        // return is treated as bound — the case only
        // `skip_generic_signature_bodies` covers.
        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(u_name)],
            ..crate::types::FunctionShape::new(vec![], infer_v)
        });
        assert!(!contains_free_infer_types(&interner, generic_fn));
        // The un-scoped walk still sees the structural `infer` in both.
        assert!(contains_infer_types(&interner, generic_fn));
    }
}
