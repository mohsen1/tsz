use std::ops::ControlFlow;

use rustc_hash::FxHashSet;

use super::{ContentPredicate, is_eval_affecting_node};
use crate::construction::TypeDatabase;
use crate::types::{TypeData, TypeId};
use crate::visitors::child_policy::{
    ChildPolicy, has_policy_children, try_for_each_child_with_policy,
};

pub(super) struct EvalInertWalker<'a> {
    db: &'a dyn TypeDatabase,
    visiting: FxHashSet<TypeId>,
}

impl<'a> EvalInertWalker<'a> {
    pub(super) fn new(db: &'a dyn TypeDatabase) -> Self {
        Self {
            db,
            visiting: FxHashSet::default(),
        }
    }

    /// Returns `(contains_eval_affecting, cycle_tainted)`.
    pub(super) fn contains_eval_affecting(&mut self, type_id: TypeId) -> (bool, bool) {
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

pub(super) struct CachedContentWalker<'a, P: ContentPredicate> {
    db: &'a dyn TypeDatabase,
    predicate: &'a P,
    policy: ChildPolicy,
    /// Intrinsic-range sentinel id (e.g. `TypeId::ERROR`) matched before the
    /// intrinsic fast path, snapshotted from `predicate.sentinel()` alongside
    /// `policy`. See [`ContentPredicate::sentinel`].
    sentinel: Option<TypeId>,
    visiting: FxHashSet<TypeId>,
}

impl<'a, P: ContentPredicate> CachedContentWalker<'a, P> {
    pub(super) fn new(db: &'a dyn TypeDatabase, predicate: &'a P) -> Self {
        Self {
            db,
            predicate,
            policy: predicate.child_policy(),
            sentinel: predicate.sentinel(),
            visiting: FxHashSet::default(),
        }
    }

    /// Returns `(predicate_holds, cycle_tainted)`.
    fn check_tracked(&mut self, type_id: TypeId) -> (bool, bool) {
        // The sentinel sits in the intrinsic id range, so it must be matched
        // before the intrinsic fast path. A sentinel hit is a definite,
        // untainted fact; it is not cached (intrinsic-range ids are never
        // written to the predicate cache).
        if self.sentinel == Some(type_id) {
            return (true, false);
        }
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
        if !has_policy_children(&key, &self.policy) {
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

    pub(super) fn check(&mut self, type_id: TypeId) -> bool {
        self.check_tracked(type_id).0
    }

    /// Walk the node's children under the predicate's child set.
    fn walk_children(&mut self, key: &TypeData) -> (bool, bool) {
        let db = self.db;
        let policy = self.policy;
        let mut tainted = false;
        let found = try_for_each_child_with_policy::<(), _>(db, key, &policy, &mut |child| {
            let (child_found, child_tainted) = self.check_tracked(child);
            tainted |= child_tainted;
            if child_found {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
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

pub(super) struct AliasConditionalWalkState {
    visited: FxHashSet<TypeId>,
    depth_limit: usize,
}

impl AliasConditionalWalkState {
    pub(super) fn new(depth_limit: usize) -> Self {
        Self {
            visited: FxHashSet::default(),
            depth_limit,
        }
    }

    pub(super) fn should_stop(&mut self, type_id: TypeId, depth: usize) -> bool {
        depth > self.depth_limit || type_id.is_intrinsic() || !self.visited.insert(type_id)
    }
}

pub(super) struct NeverIndexAccessSurfaceWalkState {
    visited: FxHashSet<TypeId>,
}

impl NeverIndexAccessSurfaceWalkState {
    pub(super) fn new() -> Self {
        Self {
            visited: FxHashSet::default(),
        }
    }

    pub(super) fn should_stop(&mut self, type_id: TypeId, remaining_depth: usize) -> bool {
        remaining_depth == 0 || type_id.is_intrinsic() || !self.visited.insert(type_id)
    }
}
