use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::Cell;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_solver::def::DefId;
use tsz_solver::{TypeId, TypeParamInfo};

/// O(1)-cloneable copy-on-write wrapper for checker cache collections.
///
/// Speculation snapshots (`CheckerContext::snapshot_full` /
/// `snapshot_return_type`) and child-checker construction
/// (`CheckerContext::with_parent_cache`) historically deep-cloned whole cache
/// maps to get an isolated copy, paying O(cache-size) per snapshot/child even
/// when nothing was subsequently mutated. `CowCache` makes the snapshot an
/// `Arc` bump instead, following the `NodeArena`/`NodeTypeCache` idiom
/// (PR #13033): `clone()` is O(1), and the first mutable access after a clone
/// detaches the map via [`Arc::make_mut`], so the deep copy is paid at most
/// once per diverging holder — and never for snapshots that are dropped or
/// rolled back without intervening writes.
///
/// Isolation semantics are unchanged from a deep clone: every writer goes
/// through `DerefMut` (`Arc::make_mut`), so mutations on one holder are never
/// visible through another holder's `Arc`, regardless of write order.
///
/// Method-call reads (`get`, `contains_key`, `iter`, `len`, ...) auto-deref
/// immutably and never copy; only `&mut self` collection methods (`insert`,
/// `remove`, `retain`, `clear`, `extend`, `entry`) trigger the copy-on-write
/// detach. Avoid calling mutating methods that are likely no-ops (e.g.
/// `remove` of a probably-absent key) on a probably-shared holder.
#[derive(Debug)]
pub struct CowCache<T: Clone>(Arc<T>);

impl<T: Clone> CowCache<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }

    /// Unwrap into the inner collection, cloning only when still shared.
    #[inline]
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| (*shared).clone())
    }

    /// `true` when both wrappers share the same underlying allocation
    /// (used by tests asserting snapshot/COW behavior).
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Clone> Clone for CowCache<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        if !Arc::ptr_eq(&self.0, &source.0) {
            self.0 = Arc::clone(&source.0);
        }
    }
}

impl<T: Clone + Default> Default for CowCache<T> {
    #[inline]
    fn default() -> Self {
        Self(Arc::new(T::default()))
    }
}

impl<T: Clone> std::ops::Deref for CowCache<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone> std::ops::DerefMut for CowCache<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

/// File-local synthetic type-node surface caches.
#[derive(Debug, Default)]
pub struct TypeNodeSurfaceCaches {
    /// Cached synthetic `typeof globalThis` surface for this checker file.
    ///
    /// The surface is derived from current-file and lib value globals, so it is
    /// checker-local rather than `ProgramContext`-shared. The in-progress bit
    /// breaks nested `typeof globalThis` annotations while the surface itself is
    /// being built; those recursive self-edges use `unknown`, matching the
    /// explicit `globalThis` self-property fallback.
    pub global_this_type: Cell<Option<TypeId>>,
    pub global_this_type_in_progress: Cell<bool>,
}

impl TypeNodeSurfaceCaches {
    pub fn clear(&self) {
        self.global_this_type.set(None);
        self.global_this_type_in_progress.set(false);
    }
}

/// Checker-local memos for type-reference argument validation.
#[derive(Debug, Default)]
pub struct TypeReferenceValidationCaches {
    /// Type-reference argument validations that completed without diagnostics
    /// in the current lexical type-parameter scope.
    pub arg_validation: FxHashSet<(u32, u32, u64)>,
    /// Type-node validations that completed without diagnostics in the
    /// current lexical type-parameter scope and active alias-resolution
    /// context.
    pub type_node_validation: FxHashSet<(u32, bool, u64, u64)>,
    /// Syntax-guided type-reference argument instantiations in the current
    /// lexical type-parameter scope, including misses.
    pub syntax_instantiation: FxHashMap<(usize, u32, TypeId, u64), Option<TypeId>>,
    /// Alias-body validation reachability results for the common case where
    /// exactly one alias is active in the resolution stack.
    pub alias_reaches_single_resolving_alias: FxHashMap<(SymbolId, DefId), bool>,
    /// Successful generic type-argument constraint relations for prepared
    /// source/target types in the current file session. Failures are uncached
    /// so diagnostic relation requests still produce structured failure data.
    pub type_arg_constraint_relation_successes: FxHashSet<(TypeId, TypeId, u16, bool)>,
    /// Declared type-parameter lists keyed by reference symbol identity, valid
    /// for the lifetime of the current source file. `SymbolId` values are
    /// arena-local in project checks, so imported aliases from different files
    /// can share the same raw id while declaring different arities.
    pub ref_type_params: FxHashMap<(SymbolId, Option<usize>, String), Vec<TypeParamInfo>>,
    /// Results for conditional-branch constraint proofs. These checks can be
    /// reached repeatedly while extracting generic parameter lists from aliases
    /// imported or re-exported through several files.
    pub conditional_branch_constraint: FxHashMap<(TypeId, TypeId), bool>,
    /// Results for indexed-object-map branch constraint proofs. This memo sits
    /// underneath `conditional_branch_constraint` because different conditional
    /// aliases can expose the same mapped-object branch/value constraint pair.
    pub indexed_object_map_branch_constraint: FxHashMap<(TypeId, TypeId), bool>,
    /// Type-parameter default/constraint validations that completed without
    /// diagnostics for the active checker file.
    pub type_param_default_constraint: FxHashSet<(u32, TypeId, TypeId)>,
    /// Synthetic type-node surfaces cached for the active checker file.
    pub type_node_surface: TypeNodeSurfaceCaches,
    /// Stamp-guarded result memo for `evaluate_type_for_assignability`.
    /// See [`AssignabilityEvalMemo`].
    pub assignability_eval_memo: AssignabilityEvalMemo,
    /// Stamp-guarded memo for reason-collecting assignability relation
    /// outcomes. See [`AssignabilityFailureMemo`].
    pub assignability_failure_memo: AssignabilityFailureMemo,
}

/// Program-wide success tier for generic type-argument constraint proofs,
/// shared by every file checker of a multi-file program.
///
/// In project mode each file checker re-proves the same constraint pairs for
/// the generic aliases it imports (the per-file
/// [`TypeReferenceValidationCaches`] start empty for every file), so the same
/// `(TypeId, TypeId)` proof is recomputed once per referencing file. `TypeId`s
/// are interned program-wide, so a proof over file-independent types has one
/// program-wide answer — mirror of the solver's shared relation cache, lifted
/// to the checker's TS2344 proof orchestration.
///
/// Soundness contract (enforced at the publish sites):
/// - only **successes** are published; failures stay file-local so diagnostic
///   relation requests re-run with full failure analysis;
/// - both key types must be free of generic type parameters and of
///   file-relative content (`contains_file_relative_content`), so the proof
///   does not depend on the publishing file's scope;
/// - proofs that observed an unresolved `Lazy` def
///   (`lazy_resolve_failure_count` advanced) or ran with exhausted evaluation
///   fuel are not published.
#[derive(Debug, Default)]
pub struct SharedConstraintProofCache {
    /// Mirror of `type_arg_constraint_relation_successes`: successful TS2344
    /// constraint relations keyed by prepared source/target plus the packed
    /// relation flags and sound-mode bit.
    pub type_arg_relation_successes: dashmap::DashSet<(TypeId, TypeId, u16, bool)>,
    /// Mirror of `conditional_branch_constraint`, `true` results only.
    pub conditional_branch_successes: dashmap::DashSet<(TypeId, TypeId)>,
    /// Mirror of `indexed_object_map_branch_constraint`, `true` results only.
    pub indexed_object_map_branch_successes: dashmap::DashSet<(TypeId, TypeId)>,
}

/// Sparse cache for node-index-keyed `TypeId` lookups.
///
/// `NodeIndex` values are arena-local, so this cache is never shared across
/// parent/child checkers. It is Arc-backed for cheap speculation snapshots:
/// rollback stores a read snapshot and the active cache copy-on-writes only if
/// it is mutated after the snapshot.
///
/// # Overlay mode
///
/// Speculative passes (overload-resolution argument collection) need two
/// properties at once:
///
/// 1. **Read visibility**: type queries issued while collecting argument types
///    must see every expression type the surrounding (non-speculative) check
///    already computed — flow narrowing of an argument like `obj[k]` after
///    `obj[k] = rhs` needs `rhs`'s cached type, exactly as on the
///    non-overloaded call path where the cache is never masked.
/// 2. **Write isolation**: entries produced while probing a candidate must be
///    identifiable so the caller can keep only the winning candidate's entries.
///
/// [`Self::overlay`] provides both: reads fall through to a read-only `base`
/// snapshot of the caller's entries, while writes (and removals, recorded as
/// [`TypeId::NONE`] tombstones) stay in the overlay's own `data` layer.
/// Bulk operations that harvest speculative results ([`Self::iter`],
/// [`Self::merge`], [`Self::merge_owned`]) intentionally see only the overlay
/// layer, so the existing "restore caller map, merge winner entries" restore
/// choreography is unchanged.
#[derive(Clone, Debug)]
pub struct NodeTypeCache {
    data: Arc<FxHashMap<u32, TypeId>>,
    /// Read-only fallback consulted on `data` misses. `None` for plain caches.
    ///
    /// Invariants:
    /// - `base` never contains [`TypeId::NONE`].
    /// - `data` may contain [`TypeId::NONE`] tombstones only when `base` is
    ///   `Some`; a tombstone masks the base entry for that key.
    base: Option<Arc<FxHashMap<u32, TypeId>>>,
}

impl NodeTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity.min(4096),
                Default::default(),
            )),
            base: None,
        }
    }

    #[inline]
    pub fn new() -> Self {
        Self {
            data: Arc::new(FxHashMap::default()),
            base: None,
        }
    }

    /// Create an empty speculative write layer whose reads fall through to
    /// this cache's current visible entries. See the type-level docs.
    pub fn overlay(&self) -> Self {
        let base = if self.base.is_none() {
            // Plain cache: share its map as the read-only base (O(1)).
            Arc::clone(&self.data)
        } else {
            // Already an overlay: flatten to a single visible view so the new
            // overlay has a NONE-free base (O(n), not hit by the overload
            // choreography which always overlays the pristine caller map).
            Arc::new(self.to_hash_map())
        };
        Self {
            data: Arc::new(FxHashMap::default()),
            base: Some(base),
        }
    }

    #[inline]
    pub fn get(&self, key: &u32) -> Option<&TypeId> {
        // Plain caches never store NONE (see `insert`), so the common
        // non-speculative path stays a single hash lookup.
        let Some(base) = &self.base else {
            return self.data.get(key);
        };
        match self.data.get(key) {
            // Tombstone: the entry was removed in this layer; do not fall
            // through to the base.
            Some(&TypeId::NONE) => None,
            Some(value) => Some(value),
            None => base.get(key),
        }
    }

    #[inline]
    pub fn insert(&mut self, key: u32, value: TypeId) {
        if key == u32::MAX {
            return;
        }
        if value == TypeId::NONE {
            self.remove(&key);
            return;
        }
        Arc::make_mut(&mut self.data).insert(key, value);
    }

    #[inline]
    pub fn contains_key(&self, key: &u32) -> bool {
        self.get(key).is_some()
    }

    #[inline]
    pub fn remove(&mut self, key: &u32) -> Option<TypeId> {
        let Some(base) = &self.base else {
            if !self.data.contains_key(key) {
                return None;
            }
            tracing::trace!(key, "node_types: removing entry");
            return Arc::make_mut(&mut self.data).remove(key);
        };
        let previous = match self.data.get(key) {
            // Already tombstoned in this layer.
            Some(&TypeId::NONE) => return None,
            Some(&value) => value,
            None => *base.get(key)?,
        };
        tracing::trace!(key, "node_types: removing entry");
        if base.contains_key(key) {
            // Mask the base entry instead of exposing it again.
            Arc::make_mut(&mut self.data).insert(*key, TypeId::NONE);
        } else {
            Arc::make_mut(&mut self.data).remove(key);
        }
        Some(previous)
    }

    #[inline]
    pub fn or_insert(&mut self, key: u32, value: TypeId) -> TypeId {
        // Preserve the cache invariant maintained by `insert` above: NONE is
        // *never* stored as a real entry. If the caller asks to insert NONE,
        // return either the existing real value or NONE without touching the
        // map. Without this guard, `or_insert(key, NONE)` followed by `get(key)`
        // would return `Some(&NONE)` (a stale "cached" sentinel) instead of
        // `None`, and downstream callers (e.g. `type_node_resolution.rs:226`)
        // that check `if let Some(&cached) = ...get(&idx.0)` would return the
        // sentinel as if it were a real type.
        if value == TypeId::NONE {
            return self.get(&key).copied().unwrap_or(TypeId::NONE);
        }
        if let Some(&existing) = self.get(&key) {
            return existing;
        }
        Arc::make_mut(&mut self.data).insert(key, value);
        value
    }

    /// Iterate this cache's own (non-tombstone) entries. For an overlay this
    /// is exactly the set of speculative writes — base entries are excluded by
    /// design so harvest/merge sites keep their pre-overlay behavior.
    pub fn iter(&self) -> impl Iterator<Item = (u32, TypeId)> + '_ {
        self.data
            .iter()
            .filter(|(_, t)| **t != TypeId::NONE)
            .map(|(i, t)| (*i, *t))
    }

    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.data).clear();
        self.base = None;
    }

    pub fn merge(&mut self, other: &Self) {
        self.extend(other.iter());
    }

    pub fn merge_owned(&mut self, other: Self) {
        self.extend(other.iter());
    }

    pub fn extend<I: IntoIterator<Item = (u32, TypeId)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }

    /// Number of entries in this cache's own layer (tombstones included);
    /// base entries of an overlay are not counted. Used for cache statistics,
    /// not for visibility decisions — use [`Self::get`]/[`Self::contains_key`]
    /// for those.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// `true` when this cache's own layer has no entries. Like [`Self::len`],
    /// ignores any overlay base.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Materialize the visible view (base entries overlaid with this layer's
    /// writes, tombstoned keys excluded).
    pub fn to_hash_map(&self) -> FxHashMap<u32, TypeId> {
        let Some(base) = &self.base else {
            return self.iter().collect();
        };
        let mut map: FxHashMap<u32, TypeId> = base.as_ref().clone();
        for (key, value) in self.data.iter() {
            if *value == TypeId::NONE {
                map.remove(key);
            } else {
                map.insert(*key, *value);
            }
        }
        map
    }
}

impl Default for NodeTypeCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Dense tristate cache for `is_narrowable_identifier` results.
#[derive(Clone, Debug)]
pub struct NarrowableIdentifierCache {
    data: Vec<u8>,
}

impl NarrowableIdentifierCache {
    const UNKNOWN: u8 = 0;
    const NOT_NARROWABLE: u8 = 1;
    const NARROWABLE: u8 = 2;

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![Self::UNKNOWN; capacity],
        }
    }

    #[inline]
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[inline]
    pub fn get(&self, key: u32) -> Option<bool> {
        let idx = key as usize;
        match self.data.get(idx).copied().unwrap_or(Self::UNKNOWN) {
            Self::NARROWABLE => Some(true),
            Self::NOT_NARROWABLE => Some(false),
            _ => None,
        }
    }

    #[inline]
    pub fn insert(&mut self, key: u32, value: bool) {
        let idx = key as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, Self::UNKNOWN);
        }
        self.data[idx] = if value {
            Self::NARROWABLE
        } else {
            Self::NOT_NARROWABLE
        };
    }
}

impl Default for NarrowableIdentifierCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Sparse cache for `SymbolId -> TypeId` lookups.
///
/// `SymbolId`s are global after program merge, so a dense per-checker vector
/// scales with total program symbols even when a checker touches only a small
/// subset. Keep the cache sparse and Arc-backed so child checkers can inherit a
/// read snapshot cheaply; writes copy-on-write only the populated entries.
#[derive(Clone, Debug)]
pub struct SymbolTypeCache {
    data: Arc<FxHashMap<SymbolId, TypeId>>,
    /// Monotonic mutation counter. Consumers (the assignability evaluation
    /// memo) treat a version change as "any previously observed symbol type
    /// may have changed"; reads never bump it.
    version: u64,
}

impl SymbolTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity.min(4096),
                Default::default(),
            )),
            version: 0,
        }
    }

    #[inline]
    pub fn new() -> Self {
        Self {
            data: Arc::new(FxHashMap::default()),
            version: 0,
        }
    }

    /// Monotonic counter bumped on every mutation that can change a lookup
    /// result. Consumed by `CheckerContext::assignability_eval_memo` stamps.
    #[inline]
    pub const fn version(&self) -> u64 {
        self.version
    }

    #[inline]
    pub fn get(&self, key: &SymbolId) -> Option<&TypeId> {
        self.data.get(key)
    }

    #[inline]
    pub fn insert(&mut self, key: SymbolId, value: TypeId) {
        // No-op writes (same value, or removing an absent entry) leave every
        // lookup result unchanged, so they must not bump the version: version
        // consumers would needlessly drop their memoized state.
        let existing = self.data.get(&key).copied();
        if value == TypeId::NONE {
            if existing.is_none() {
                return;
            }
            self.version += 1;
            Arc::make_mut(&mut self.data).remove(&key);
        } else {
            if existing == Some(value) {
                return;
            }
            self.version += 1;
            Arc::make_mut(&mut self.data).insert(key, value);
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &SymbolId) -> bool {
        self.data.contains_key(key)
    }

    #[inline]
    pub fn remove(&mut self, key: &SymbolId) -> Option<TypeId> {
        if !self.data.contains_key(key) {
            return None;
        }
        self.version += 1;
        Arc::make_mut(&mut self.data).remove(key)
    }

    #[inline]
    pub fn entry_or_insert(&mut self, key: SymbolId, value: TypeId) -> TypeId {
        // Same NONE-storage guard as `NodeTypeCache::or_insert` — `insert`
        // explicitly removes NONE entries to maintain the cache invariant
        // that `get`/`contains_key` only see real types. `entry().or_insert()`
        // with `value == NONE` would silently break that.
        if value == TypeId::NONE {
            return self.data.get(&key).copied().unwrap_or(TypeId::NONE);
        }
        if !self.data.contains_key(&key) {
            self.version += 1;
        }
        *Arc::make_mut(&mut self.data).entry(key).or_insert(value)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, TypeId)> + '_ {
        self.data
            .iter()
            .map(|(&symbol_id, &type_id)| (symbol_id, type_id))
    }

    pub fn to_hash_map(&self) -> FxHashMap<SymbolId, TypeId> {
        self.data.as_ref().clone()
    }

    pub fn extend(&mut self, other: Self) {
        if other.data.is_empty() {
            return;
        }
        let changes = other.data.iter().any(|(symbol_id, &type_id)| {
            type_id != TypeId::NONE && self.data.get(symbol_id) != Some(&type_id)
        });
        if !changes {
            return;
        }
        self.version += 1;
        let data = Arc::make_mut(&mut self.data);
        for (&symbol_id, &type_id) in other.data.iter() {
            if type_id != TypeId::NONE {
                data.insert(symbol_id, type_id);
            }
        }
    }
}

/// Session-state stamp guarding [`AssignabilityEvalMemo`] entries.
///
/// Captures the generations of the two checker `TypeEnvironment`s — `type_env`
/// (relation/evaluation bindings) and `type_environment` (flow-narrowing
/// bindings) are independently mutated, so both generations are needed; each
/// already folds in the shared `DefinitionStore` generation — plus the
/// mutation versions of the symbol-type caches. Evaluating a type for
/// assignability is deterministic while none of these change: every mutable
/// input the evaluation consults (def/type-env bindings, resolved symbol
/// types, class instance types) bumps one of the components.
pub type AssignabilityEvalStamp = (u64, u64, u64, u64);

/// Result memo for `evaluate_type_for_assignability`.
///
/// That evaluation is a recursive normalization pipeline with only a cycle
/// guard; constraint validation and relation preparation re-run it for the
/// same `TypeId`s thousands of times per file (measured ~94% repeated
/// outermost calls on the ts-toolbelt project row, issue #8356, plus nested
/// repeats in issue #13243). Entries are only written for fuel-clean
/// evaluations that completed outside the active cycle-truncation case and are
/// dropped wholesale whenever the session stamp moves, so a hit always
/// returns exactly what a fresh evaluation under the current environment would.
#[derive(Debug, Default)]
pub struct AssignabilityEvalMemo {
    stamp: Option<AssignabilityEvalStamp>,
    entries: FxHashMap<TypeId, TypeId>,
}

impl AssignabilityEvalMemo {
    fn roll_to(&mut self, stamp: AssignabilityEvalStamp) {
        if self.stamp != Some(stamp) {
            self.entries.clear();
            self.stamp = Some(stamp);
        }
    }

    /// Look up a memoized evaluation result valid for `stamp`.
    pub fn get(&mut self, stamp: AssignabilityEvalStamp, type_id: TypeId) -> Option<TypeId> {
        self.roll_to(stamp);
        self.entries.get(&type_id).copied()
    }

    /// Record an evaluation result computed under `stamp`.
    pub fn insert(&mut self, stamp: AssignabilityEvalStamp, type_id: TypeId, result: TypeId) {
        self.roll_to(stamp);
        self.entries.insert(type_id, result);
    }

    /// Drop all entries and forget the stamp. Required between file sessions:
    /// a fresh file's environment can restart at a previously seen generation,
    /// which would otherwise collide with the prior file's stamp.
    pub fn clear(&mut self) {
        self.stamp = None;
        self.entries.clear();
    }
}

/// Key for [`AssignabilityFailureMemo`] entries: prepared (evaluated)
/// source and target types, the solver relation flags the pass ran under,
/// and the sound-mode bit (which shapes relation policy outside the packed
/// flags).
pub type AssignabilityFailureKey = (TypeId, TypeId, u16, bool);

/// One reason-collecting assignability relation outcome, captured from the
/// solver pass that decided it (`query_assignability_with_failure_analysis`).
///
/// This is the raw solver-side analysis **before** the boundary/checker
/// post-passes (excess-property suppression, intersection-constituent
/// framing, array-extends weak-type suppression), which differ per consumer
/// and must keep running on every path.
#[derive(Debug, Clone)]
pub struct CachedAssignabilityAnalysis {
    /// Pass/fail verdict of the relation.
    pub related: bool,
    /// Stack-depth limit was exceeded during the pass.
    pub depth_exceeded: bool,
    /// Iteration budget was exhausted during the pass.
    pub iteration_exceeded: bool,
    /// Whether the failure is a weak-union violation (TS2559).
    pub weak_union_violation: bool,
    /// Structured failure reason, present only when `related` is `false`
    /// and the reason walk produced one.
    pub failure_reason: Option<tsz_solver::SubtypeFailureReason>,
}

/// Stamp-guarded memo for reason-collecting assignability relation passes
/// (issue #13243).
///
/// A failing TS2322/TS2345 assignment runs the reason-collecting relation
/// more than once on identical prepared inputs: once through the
/// `RelationRequest` gateway that decides which diagnostic to emit, and
/// again inside `analyze_assignability_failure` when the error reporter
/// renders the elaboration chain. Both passes run the same configured
/// solver checker on the same `(source, target, flags, sound_mode)` key,
/// so the second is a pure re-walk. Entries follow exactly the
/// [`AssignabilityEvalMemo`] validity model: dropped wholesale whenever the
/// session stamp moves, never written for depth/iteration/fuel-degraded
/// passes, so a hit replays what a fresh pass under the current environment
/// would produce.
#[derive(Debug, Default)]
pub struct AssignabilityFailureMemo {
    stamp: Option<AssignabilityEvalStamp>,
    entries: FxHashMap<AssignabilityFailureKey, CachedAssignabilityAnalysis>,
}

impl AssignabilityFailureMemo {
    fn roll_to(&mut self, stamp: AssignabilityEvalStamp) {
        if self.stamp != Some(stamp) {
            self.entries.clear();
            self.stamp = Some(stamp);
        }
    }

    /// Look up a memoized analysis valid for `stamp`.
    pub fn get(
        &mut self,
        stamp: AssignabilityEvalStamp,
        key: AssignabilityFailureKey,
    ) -> Option<CachedAssignabilityAnalysis> {
        self.roll_to(stamp);
        self.entries.get(&key).cloned()
    }

    /// Record an analysis computed under `stamp`.
    pub fn insert(
        &mut self,
        stamp: AssignabilityEvalStamp,
        key: AssignabilityFailureKey,
        analysis: CachedAssignabilityAnalysis,
    ) {
        self.roll_to(stamp);
        self.entries.insert(key, analysis);
    }

    /// Drop all entries and forget the stamp (between file sessions; see
    /// [`AssignabilityEvalMemo::clear`]).
    pub fn clear(&mut self) {
        self.stamp = None;
        self.entries.clear();
    }
}

impl Default for SymbolTypeCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cow_cache_clone_is_shared_until_first_write() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        assert!(live.ptr_eq(&snapshot));

        // Reads through either holder never detach.
        assert_eq!(live.get(&1), Some(&10));
        assert_eq!(snapshot.get(&1), Some(&10));
        assert!(live.ptr_eq(&snapshot));

        // First write detaches the writer; the snapshot is isolated.
        live.insert(2, 20);
        assert!(!live.ptr_eq(&snapshot));
        assert_eq!(live.get(&2), Some(&20));
        assert_eq!(snapshot.get(&2), None);
        assert_eq!(snapshot.get(&1), Some(&10));
    }

    #[test]
    fn cow_cache_clone_from_restores_sharing_with_snapshot() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        live.insert(2, 20);

        // Rollback: O(1) Arc swap back to the snapshot state.
        live.clone_from(&snapshot);
        assert!(live.ptr_eq(&snapshot));
        assert_eq!(live.get(&2), None);
        assert_eq!(live.get(&1), Some(&10));

        // Rolling back twice is a no-op that keeps sharing.
        live.clone_from(&snapshot);
        assert!(live.ptr_eq(&snapshot));
    }

    #[test]
    fn cow_cache_parent_writes_after_child_snapshot_stay_isolated() {
        // `with_parent_cache` ordering: the child snapshots first, the parent
        // keeps mutating afterwards. Parent writes must not leak into the
        // child (and vice versa), exactly as with a deep clone.
        let mut parent: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        parent.insert(1, 10);
        let mut child = parent.clone();

        parent.insert(2, 20);
        assert_eq!(child.get(&2), None);

        child.insert(3, 30);
        assert_eq!(parent.get(&3), None);
        assert_eq!(parent.get(&2), Some(&20));
        assert_eq!(child.get(&1), Some(&10));
    }

    #[test]
    fn cow_cache_into_inner_clones_only_when_shared() {
        let mut live: CowCache<FxHashMap<u32, u32>> = CowCache::default();
        live.insert(1, 10);
        let snapshot = live.clone();
        let inner = live.into_inner();
        assert_eq!(inner.get(&1), Some(&10));
        // The outstanding snapshot still sees its state.
        assert_eq!(snapshot.get(&1), Some(&10));
    }

    #[test]
    fn node_type_cache_absent_remove_does_not_detach_shared_snapshot() {
        let mut parent = NodeTypeCache::new();
        parent.insert(1, TypeId::STRING);
        let mut child = parent.clone();

        assert!(child.remove(&2).is_none());
        assert!(Arc::ptr_eq(&parent.data, &child.data));

        assert_eq!(child.remove(&1), Some(TypeId::STRING));
        assert!(!Arc::ptr_eq(&parent.data, &child.data));
        assert_eq!(parent.get(&1), Some(&TypeId::STRING));
        assert_eq!(child.get(&1), None);
    }

    #[test]
    fn symbol_type_cache_absent_remove_does_not_detach_shared_snapshot() {
        let sym = SymbolId(1);
        let mut parent = SymbolTypeCache::new();
        parent.insert(sym, TypeId::STRING);
        let mut child = parent.clone();

        assert!(child.remove(&SymbolId(2)).is_none());
        assert!(Arc::ptr_eq(&parent.data, &child.data));

        assert_eq!(child.remove(&sym), Some(TypeId::STRING));
        assert!(!Arc::ptr_eq(&parent.data, &child.data));
        assert_eq!(parent.get(&sym), Some(&TypeId::STRING));
        assert_eq!(child.get(&sym), None);
    }

    #[test]
    fn node_type_cache_overlay_reads_through_base_and_isolates_writes() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut overlay = caller.overlay();
        // Base entries are visible through the overlay...
        assert_eq!(overlay.get(&1), Some(&TypeId::STRING));
        assert!(overlay.contains_key(&1));

        // ...but overlay writes stay in the overlay's own layer.
        overlay.insert(2, TypeId::NUMBER);
        assert_eq!(overlay.get(&2), Some(&TypeId::NUMBER));
        assert_eq!(caller.get(&2), None);

        // Harvest (`iter`) yields only the overlay's own writes, so the
        // overload-resolution "restore caller, merge winner" choreography
        // never re-merges base entries.
        let harvested: Vec<_> = overlay.iter().collect();
        assert_eq!(harvested, vec![(2, TypeId::NUMBER)]);
    }

    #[test]
    fn node_type_cache_overlay_tombstone_masks_base_entry() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut overlay = caller.overlay();
        assert_eq!(overlay.remove(&1), Some(TypeId::STRING));
        // The base entry stays masked rather than resurfacing.
        assert_eq!(overlay.get(&1), None);
        assert!(!overlay.contains_key(&1));
        // Removing again reports the entry as already gone.
        assert_eq!(overlay.remove(&1), None);
        // Tombstones never escape through harvest or materialization.
        assert_eq!(overlay.iter().count(), 0);
        assert!(overlay.to_hash_map().is_empty());
        // A later write through the overlay overrides the tombstone.
        overlay.insert(1, TypeId::NUMBER);
        assert_eq!(overlay.get(&1), Some(&TypeId::NUMBER));
        // The caller's map is untouched throughout.
        assert_eq!(caller.get(&1), Some(&TypeId::STRING));
    }

    #[test]
    fn node_type_cache_nested_overlay_flattens_to_visible_view() {
        let mut caller = NodeTypeCache::new();
        caller.insert(1, TypeId::STRING);

        let mut inner = caller.overlay();
        inner.insert(2, TypeId::NUMBER);
        inner.remove(&1);

        let nested = inner.overlay();
        // The nested overlay sees exactly the inner overlay's visible view:
        // the tombstoned base entry stays hidden, the inner write shows.
        assert_eq!(nested.get(&1), None);
        assert_eq!(nested.get(&2), Some(&TypeId::NUMBER));
        assert_eq!(nested.to_hash_map().len(), 1);
    }
}
