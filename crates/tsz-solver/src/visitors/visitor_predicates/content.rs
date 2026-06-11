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
    ChildPolicy, has_policy_children, try_for_each_child_with_policy,
};
use crate::{TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

use super::predicate_pool::with_predicate_buffers;

/// Check if a type contains any type parameters.
///
/// The depth-limited walk always starts from a fresh recursion guard, so the
/// answer is a pure function of the root `TypeId`. Recursive conditional
/// evaluation re-asks this for the same check/extends roots constantly, so the
/// root result is memoized project-wide on the interner.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(cached) = types.contains_param_or_infer_root_cached(type_id) {
        return cached;
    }
    let result = contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    });
    types.set_contains_param_or_infer_root_cache(type_id, result);
    result
}

/// Check if a type contains free type parameters, excluding those bound by
/// enclosing function/callable signatures. See `contains_free_type_parameters_db`
/// in `content_predicates` for the full doc.
pub fn contains_free_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    DeepContainsChecker::new(types, ChildPolicy::FREE_TYPE_PARAMS, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        )
    })
    .check_from_root(type_id)
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
/// its structural *use* surface — including `Application` bases, property
/// write types, and index-signature keys, but excluding type-parameter
/// declaration metadata — is the `TypeId::ERROR` sentinel, `TypeData::Error`,
/// or an `UnresolvedTypeName`. The `TypeId::ERROR` sentinel is matched before the
/// intrinsic fast path (it sits in the intrinsic id range), which the generic
/// `contains_type_matching` walk cannot do.
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
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            // Check predicate
            if matches!(&data, TypeData::TypeParameter(info) if info.name == name) {
                return true;
            }

            // SHALLOW treats TypeParameter/Infer as leaves: we only care
            // about identity (name match), not what their constraints contain.
            crate::visitors::child_policy::for_each_child_with_policy(
                types,
                &data,
                &ChildPolicy::SHALLOW,
                |child| {
                    if !visited.contains(&child) {
                        stack.push(child);
                    }
                },
            );
        }
        false
    })
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
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            if type_parameter_identity_matches(def_store, current, target) {
                return true;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            crate::visitors::child_policy::for_each_child_with_policy(
                types,
                &data,
                &ChildPolicy::SHALLOW,
                |child| {
                    if !visited.contains(&child) {
                        stack.push(child);
                    }
                },
            );
        }
        false
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
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            // Found the type parameter we're looking for
            if matches!(&data, TypeData::TypeParameter(info) if info.name == param_name) {
                return true;
            }

            // Follow only resolution-path children (not type reference args)
            match &data {
                // Union/intersection: descend into all members
                TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                    for &member in types.type_list(*list_id).iter() {
                        stack.push(member);
                    }
                }
                // Mapped type: descend into the constraint (key source) only.
                // This catches `T extends { [P in T]: number }` (genuinely circular)
                // while NOT false-positiving on `T extends { [K in keyof T]: V }`
                // because we don't follow through KeyOf (see below).
                TypeData::Mapped(mapped_id) => {
                    let mapped = types.get_mapped(*mapped_id);
                    stack.push(mapped.constraint);
                }
                // Index access: descend into object and index.
                // Catches `T extends Foo | T["hello"]` (circular through index access).
                TypeData::IndexAccess(obj, idx) => {
                    stack.push(*obj);
                    stack.push(*idx);
                }
                // KeyOf, Conditional, and everything else (Application, Object,
                // Function, Array, Tuple, ReadonlyType, NoInfer, etc.) are opaque
                // at the constraint-resolution level. `T extends { [K in keyof T]: V }`
                // is NOT circular in tsc, and neither is `T extends null extends T ? any : never`.
                _ => {}
            }
        }
        false
    })
}

/// Identity-based variant of `constraint_references_type_param_in_resolution_path`.
pub fn constraint_references_type_param_identity_in_resolution_path(
    types: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
    target: TypeId,
) -> bool {
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }

            if type_parameter_identity_matches(def_store, current, target) {
                return true;
            }

            let Some(data) = types.lookup(current) else {
                continue;
            };

            match &data {
                TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                    stack.extend(types.type_list(*list_id).iter().copied());
                }
                TypeData::Mapped(mapped_id) => {
                    stack.push(types.get_mapped(*mapped_id).constraint);
                }
                TypeData::IndexAccess(obj, idx) => {
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
    let mut visited = FxHashMap::default();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current == target {
            return true;
        }
        if visited.contains_key(&current) {
            continue;
        }
        visited.insert(current, true);
        crate::visitors::visitor::for_each_child_by_id(types, current, |child| {
            if !visited.contains_key(&child) {
                stack.push(child);
            }
        });
    }
    false
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
        self.check(type_id)
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

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let types = self.types;
        let policy = self.policy;
        let result = try_for_each_child_with_policy::<(), _>(types, &key, &policy, &mut |child| {
            if self.check(child) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_break();

        self.guard.leave(type_id);
        self.memo.insert(type_id, result);

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
        // Terminal-kind fast path: variants with no children under this
        // walker's policy contribute no free parameters, so skip the
        // recursion-guard/memo bookkeeping entirely. `TypeParameter`/`Infer`
        // are the leaves we collect, handled positively in `free_key`.
        if !matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
            && !has_policy_children(&key, &ChildPolicy::FREE_PARAM_COLLECT)
        {
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
        let result =
            stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || self.free_key(type_id, &key));
        self.guard.leave(type_id);
        self.memo.insert(type_id, result.clone());
        result
    }

    /// Free parameters of one node. A `TypeParameter`/`Infer` is a free
    /// occurrence and a leaf — its constraint/default are metadata, not free
    /// uses. Everything else unions its children's free parameters under
    /// [`ChildPolicy::FREE_PARAM_COLLECT`]: a *generic* signature binds its
    /// own type parameters, so its body is skipped wholesale. This
    /// intentionally does not descend into a generic signature to recover an
    /// outer parameter threaded through it; that extra precision is
    /// unnecessary for the identity-sharing decision this helper drives, and
    /// descending makes the walk dramatically deeper on real-world recursive
    /// signature graphs.
    fn free_key(&mut self, type_id: TypeId, key: &TypeData) -> FxHashSet<TypeId> {
        let mut set = FxHashSet::default();
        if matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_)) {
            set.insert(type_id);
            return set;
        }
        let types = self.types;
        crate::visitors::child_policy::for_each_child_with_policy(
            types,
            key,
            &ChildPolicy::FREE_PARAM_COLLECT,
            |child| set.extend(self.free(child)),
        );
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
    with_predicate_buffers(|visited, stack| {
        stack.push(type_id);
        while let Some(current) = stack.pop() {
            if current.is_intrinsic() || !visited.insert(current) {
                continue;
            }
            let Some(data) = types.lookup(current) else {
                continue;
            };
            match &data {
                TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                    if info.name != excluded_name {
                        return true;
                    }
                    // Skip the parameter's `constraint`/`default` — those are
                    // metadata for the parameter, not uses by the enclosing
                    // type. Same reason applies to Mapped/Function/Callable
                    // type-param lists handled in the visit_structural_children
                    // path below.
                    continue;
                }
                TypeData::ThisType | TypeData::BoundParameter(_) => return true,
                _ => {}
            }
            visit_structural_children(types, &data, |child| {
                if !visited.contains(&child) {
                    stack.push(child);
                }
            });
        }
        false
    })
}

/// Variant of [`crate::visitors::visitor::for_each_child_by_id`] that skips type-
/// parameter `constraint`/`default` metadata on `Mapped`, `Function`, and
/// `Callable` types. Used by free-type-parameter checks that must treat
/// parameter-declaration metadata as bound by the host, not as free uses.
fn visit_structural_children<F>(db: &dyn TypeDatabase, data: &TypeData, mut f: F)
where
    F: FnMut(TypeId),
{
    crate::visitors::child_policy::for_each_child_with_policy(
        db,
        data,
        &ChildPolicy::STRUCTURAL_USES,
        &mut f,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;
    use crate::types::TypeParamInfo;

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
}
