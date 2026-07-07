//! Cycle-guarded element-indexability checks.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;
use tsz_solver::TypeId;

/// Maximum union/intersection member-nesting depth walked by
/// [`CheckerState::is_element_indexable`] before the descent is treated as a
/// non-terminating recursion and cut (returns the coinductive `true`). The path
/// set already breaks genuine cycles; this is the backstop for an unbounded
/// chain of *distinct* instantiations. Set well above any legitimate finite
/// index-signature nesting so a real verdict is never masked.
const MAX_ELEMENT_INDEXABLE_DEPTH: u32 = 1000;
const HASH_MAP_ENTRY_OVERHEAD_ESTIMATE: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ElementIndexableMemoStats {
    entries: usize,
    estimated_size_bytes: usize,
}

#[derive(Debug, Default)]
struct ElementIndexableMemo {
    memo: FxHashMap<TypeId, bool>,
}

impl ElementIndexableMemo {
    fn get(&self, type_id: TypeId) -> Option<bool> {
        self.memo.get(&type_id).copied()
    }

    fn insert(&mut self, type_id: TypeId, result: bool) {
        self.memo.insert(type_id, result);
    }

    fn entry_count(&self) -> usize {
        self.memo.len()
    }

    fn estimated_size_bytes(&self) -> usize {
        self.memo.capacity().saturating_mul(
            mem::size_of::<TypeId>()
                .saturating_add(mem::size_of::<bool>())
                .saturating_add(HASH_MAP_ENTRY_OVERHEAD_ESTIMATE),
        )
    }

    fn stats(&self) -> ElementIndexableMemoStats {
        ElementIndexableMemoStats {
            entries: self.entry_count(),
            estimated_size_bytes: self.estimated_size_bytes(),
        }
    }
}

struct ElementIndexableWalk<'state, 'ctx> {
    checker: &'state CheckerState<'ctx>,
    wants_string: bool,
    wants_number: bool,
    visiting: FxHashSet<TypeId>,
    memo: ElementIndexableMemo,
}

impl<'state, 'ctx> ElementIndexableWalk<'state, 'ctx> {
    /// Cycle/depth-guarded core of [`CheckerState::is_element_indexable`].
    ///
    /// `is_element_indexable` walks into union and intersection members
    /// recursively. A self-referential type — e.g. the union-of-unions graphs
    /// produced by purry data-first/data-last overload typing — re-enters the
    /// same `TypeId` (a true cycle) or descends an unbounded chain of distinct
    /// instantiations, so the naive recursion has no fixed point and overflows
    /// the stack (issue #13507, remeda/trpc/mobx witnesses).
    ///
    /// `visiting` is a path set: a `TypeId` already on the current descent is a
    /// back-edge. `tsc` resolves recursive structural questions coinductively
    /// (it *assumes* the recursive position is satisfied), so a back-edge — or a
    /// descent past [`MAX_ELEMENT_INDEXABLE_DEPTH`], the backstop for an
    /// unbounded chain of distinct types — returns `true`. For a union (`all`)
    /// `true` is the identity that defers the verdict to the concrete members;
    /// for an intersection (`any`) it preserves the "some member supplies the
    /// index signature" reading. Either way the recursive occurrence never
    /// drives a spurious `TS7053`, matching `tsc`'s clean result on these types.
    ///
    /// Only union and intersection members recurse, so the cycle/depth/memo
    /// machinery is engaged lazily by [`Self::walk_members`] — a leaf type
    /// (array/tuple/string-like/object/other) returns its verdict directly and
    /// the common shallow case touches neither map. The `visiting` set is
    /// path-scoped (removed on the way out) so a type shared across sibling
    /// branches is still classified on its own merits — only genuine ancestors
    /// are cut. `memo` caches the *finished* verdict of each fully-walked
    /// composite so a node shared across branches (or revisited after a cycle
    /// resolves) is walked once, keeping the descent polynomial instead of
    /// re-expanding the recursive subgraph exponentially. A type's indexability
    /// is the fixed point of its own definition — the same value `tsc` computes
    /// once and reuses — so caching it per `TypeId` is sound even when the cut
    /// value seeded a member along the way.
    fn classify(&mut self, object_type: TypeId, depth: u32) -> bool {
        if let Some(cached) = self.memo.get(object_type) {
            return cached;
        }
        // Use the resolver-aware classifier so that `Application(Lazy(DefId), args)`
        // wrappers — including those nested inside intersection / union members —
        // are expanded through the checker's `TypeEnvironment` before classification.
        // Without this, an intersection like `{ a: number } & `Record<string, V>`
        // keeps the `Record` member opaque (classifier returns `Other`), which
        // causes a false TS7053 for indexed accesses on a type parameter
        // constrained to that intersection. The recursive call below stays on the
        // same path, so the resolver is threaded into every member as well.
        match query::classify_element_indexable_with_resolver(
            self.checker.ctx.types,
            &self.checker.ctx,
            object_type,
        ) {
            query::ElementIndexableKind::Array
            | query::ElementIndexableKind::Tuple
            | query::ElementIndexableKind::StringLike => self.wants_number,
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => {
                (self.wants_string && has_string)
                    || (self.wants_number && (has_number || has_string))
            }
            query::ElementIndexableKind::Union(members) => {
                self.walk_members(object_type, &members, depth, true)
            }
            query::ElementIndexableKind::Intersection(members) => {
                self.walk_members(object_type, &members, depth, false)
            }
            query::ElementIndexableKind::Other => false,
        }
    }

    /// Walk the members of a union (`require_all`) or intersection
    /// (`!require_all`) composite under the cycle/depth guard. A back-edge to an
    /// ancestor composite or a descent past [`MAX_ELEMENT_INDEXABLE_DEPTH`] is
    /// the coinductive cut (`true`): `tsc` assumes the recursive position is
    /// satisfied, so the verdict is driven by the concrete members and the
    /// recursive occurrence never forces a spurious `TS7053`. A cut node is
    /// unfinished and is not memoized.
    fn walk_members(
        &mut self,
        object_type: TypeId,
        members: &[TypeId],
        depth: u32,
        require_all: bool,
    ) -> bool {
        if depth >= MAX_ELEMENT_INDEXABLE_DEPTH || !self.visiting.insert(object_type) {
            return true;
        }
        let result = if require_all {
            members
                .iter()
                .all(|&member| self.classify(member, depth + 1))
        } else {
            members
                .iter()
                .any(|&member| self.classify(member, depth + 1))
        };
        self.visiting.remove(&object_type);
        self.memo.insert(object_type, result);
        result
    }
}

impl<'a> CheckerState<'a> {
    /// Check if a type key supports element indexing.
    ///
    /// This function determines if a type supports element access with the
    /// specified index kind (string, number, or both).
    ///
    /// ## Parameters:
    /// - `object_type`: The type to check
    /// - `wants_string`: Whether string indexing is needed
    /// - `wants_number`: Whether numeric indexing is needed
    ///
    /// ## Returns:
    /// - `true`: The type supports the requested indexing
    /// - `false`: The type does not support the requested indexing
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array supports numeric indexing:
    /// const arr: number[] = [1, 2, 3];
    /// arr[0];  // OK
    ///
    /// // Object with string index supports string indexing:
    /// const obj: { [key: string]: number } = {};
    /// obj["foo"];  // OK
    ///
    /// // Object without index signature doesn't support indexing:
    /// const plain: { a: number } = { a: 1 };
    /// plain["b"];  // Error: No index signature
    /// ```
    pub(crate) fn is_element_indexable(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        // `wants_string` / `wants_number` are invariant across the whole descent,
        // so the per-call `memo` can key on `TypeId` alone.
        let mut walk = ElementIndexableWalk {
            checker: self,
            wants_string,
            wants_number,
            visiting: FxHashSet::default(),
            memo: ElementIndexableMemo::default(),
        };
        let result = walk.classify(object_type, 0);
        if tracing::enabled!(tracing::Level::TRACE) {
            let memo_stats = walk.memo.stats();
            tracing::trace!(
                object_type = object_type.0,
                wants_string,
                wants_number,
                result,
                memo_entries = memo_stats.entries,
                memo_estimated_size_bytes = memo_stats.estimated_size_bytes,
                "element_indexable_walk"
            );
        }
        result
    }

    /// Check whether a type supports either string or number indexing for an
    /// `any` key. This is semantically equivalent to separate string/number
    /// probes because string index signatures accept numeric keys, but it keeps
    /// recursive union/intersection classification to one guarded walk.
    pub(crate) fn is_element_indexable_by_any_key(&self, object_type: TypeId) -> bool {
        self.is_element_indexable(object_type, true, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_indexable_memo_records_entries_and_size() {
        let mut memo = ElementIndexableMemo::default();

        assert_eq!(
            memo.stats(),
            ElementIndexableMemoStats {
                entries: 0,
                estimated_size_bytes: 0,
            }
        );

        assert_eq!(memo.get(TypeId::STRING), None);
        memo.insert(TypeId::STRING, true);
        assert_eq!(memo.get(TypeId::STRING), Some(true));

        let stats = memo.stats();
        assert_eq!(stats.entries, 1);
        assert!(
            stats.estimated_size_bytes >= mem::size_of::<TypeId>() + mem::size_of::<bool>(),
            "estimated size should account for stored key/value bytes: {stats:?}"
        );
    }

    #[test]
    fn element_indexable_memo_overwrites_finished_result_without_entry_growth() {
        let mut memo = ElementIndexableMemo::default();

        memo.insert(TypeId::NUMBER, false);
        memo.insert(TypeId::NUMBER, true);

        assert_eq!(memo.get(TypeId::NUMBER), Some(true));
        let stats = memo.stats();
        assert_eq!(stats.entries, 1);
    }
}
