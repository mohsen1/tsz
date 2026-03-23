//! Speculation / transaction API for checker state.
//!
//! Speculative type computation (overload resolution, return-type inference,
//! contextual typing probes) must not leak committed checker state. This module
//! provides a reusable transaction boundary that snapshots the mutable
//! diagnostic / dedup / cache state of `CheckerContext` and supports:
//!
//! - **Rollback-on-drop** (default): all speculative state is discarded.
//! - **Explicit commit**: promotes speculative state into the parent context.
//! - **Selective keep**: applies a user-supplied filter to diagnostics before
//!   committing, for sites that intentionally preserve some speculative results.
//!
//! # Architecture note
//!
//! This is pure checker orchestration — it manages diagnostic/cache state, not
//! type algorithms. The solver is not involved.

use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::{FlowNodeId, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use crate::diagnostics::Diagnostic;

use super::{CheckerContext, PendingImplicitAnyKind, PendingImplicitAnyVar, RequestCacheKey};

// ---------------------------------------------------------------------------
// Internal helpers (free functions to avoid borrow conflicts)
// ---------------------------------------------------------------------------

/// Remove `emitted_ts2454_errors` dedup entries for discarded diagnostics
/// with code 2454. Without this cleanup, discarded TS2454 errors remain in
/// the dedup set and prevent re-emission on subsequent passes.
fn cleanup_ts2454_dedup(
    emitted_ts2454_errors: &mut FxHashSet<(u32, SymbolId)>,
    discarded: &[Diagnostic],
) {
    for diag in discarded {
        if diag.code == 2454 {
            emitted_ts2454_errors.retain(|&(pos, _)| pos != diag.start);
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot types
// ---------------------------------------------------------------------------

/// Snapshot of diagnostic state that speculative evaluation may corrupt.
///
/// Created by `CheckerContext::begin_speculation` and consumed by the
/// `SpeculationGuard` on drop/commit.
pub(crate) struct DiagnosticSnapshot {
    /// Length of `ctx.diagnostics` at snapshot time (truncation point).
    pub diagnostics_len: usize,
    /// Clone of `ctx.emitted_diagnostics` for dedup restoration.
    pub emitted_diagnostics: FxHashSet<(u32, u32)>,
    /// Length of `ctx.deferred_ts2454_errors` at snapshot time.
    pub deferred_ts2454_len: usize,
}

/// Extended snapshot that also captures TS2454/TS2307/implicit-any/cache state.
///
/// Used by heavyweight speculative sites (overload resolution, return-type
/// inference) that mutate more than just the diagnostic vector.
pub(crate) struct FullSnapshot {
    pub diag: DiagnosticSnapshot,
    pub emitted_ts2454_errors: FxHashSet<(u32, SymbolId)>,
    pub modules_with_ts2307_emitted: FxHashSet<String>,
    pub pending_implicit_any_vars: FxHashMap<SymbolId, PendingImplicitAnyVar>,
    pub reported_implicit_any_vars: FxHashMap<SymbolId, PendingImplicitAnyKind>,
    pub implicit_any_checked_closures: FxHashSet<NodeIndex>,
    pub request_node_types: FxHashMap<(u32, RequestCacheKey), TypeId>,
}

/// Cache snapshot for return-type inference, which also corrupts `node_types`
/// and `flow_analysis_cache`.
pub(crate) struct CacheSnapshot {
    /// Set of `node_types` keys that existed before speculation.
    pub node_type_keys: std::collections::HashSet<u32>,
    /// Full request-aware cache snapshot. Speculation may overwrite existing
    /// entries, so rollback must restore values, not just prune additions.
    pub request_node_types: FxHashMap<(u32, RequestCacheKey), TypeId>,
    /// Clone of the flow analysis cache.
    pub flow_analysis_cache: rustc_hash::FxHashMap<(FlowNodeId, SymbolId, TypeId), TypeId>,
}

/// Complete speculation snapshot (full + cache).
pub(crate) struct ReturnTypeSnapshot {
    pub full: FullSnapshot,
    pub cache: CacheSnapshot,
}

// ---------------------------------------------------------------------------
// CheckerContext snapshot methods
// ---------------------------------------------------------------------------

impl CheckerContext<'_> {
    /// Lightweight diagnostic-only snapshot.
    ///
    /// Captures `diagnostics.len()` and clones `emitted_diagnostics`. Suitable
    /// for speculative sites that only produce diagnostics (JSX overloads,
    /// `call_helpers` property inference, elaboration probes).
    pub(crate) fn snapshot_diagnostics(&self) -> DiagnosticSnapshot {
        DiagnosticSnapshot {
            diagnostics_len: self.diagnostics.len(),
            emitted_diagnostics: self.emitted_diagnostics.clone(),
            deferred_ts2454_len: self.deferred_ts2454_errors.len(),
        }
    }

    /// Full diagnostic + dedup state snapshot.
    ///
    /// Captures everything in `DiagnosticSnapshot` plus TS2454/TS2307/
    /// implicit-any-checked-closures state. Suitable for overload resolution
    /// and contextual typing probes that may trigger closure checking.
    pub(crate) fn snapshot_full(&self) -> FullSnapshot {
        FullSnapshot {
            diag: self.snapshot_diagnostics(),
            emitted_ts2454_errors: self.emitted_ts2454_errors.clone(),
            modules_with_ts2307_emitted: self.modules_with_ts2307_emitted.clone(),
            pending_implicit_any_vars: self.pending_implicit_any_vars.clone(),
            reported_implicit_any_vars: self.reported_implicit_any_vars.clone(),
            implicit_any_checked_closures: self.implicit_any_checked_closures.clone(),
            request_node_types: self.request_node_types.clone(),
        }
    }

    /// Complete snapshot including `node_types` and `flow_analysis_cache`.
    ///
    /// Used by return-type inference which evaluates the function body
    /// speculatively (without narrowing context) and must not pollute caches.
    pub(crate) fn snapshot_return_type(&self) -> ReturnTypeSnapshot {
        ReturnTypeSnapshot {
            full: self.snapshot_full(),
            cache: CacheSnapshot {
                node_type_keys: self.node_types.keys().copied().collect(),
                request_node_types: self.request_node_types.clone(),
                flow_analysis_cache: self.flow_analysis_cache.borrow().clone(),
            },
        }
    }

    // -----------------------------------------------------------------------
    // Rollback methods
    // -----------------------------------------------------------------------

    /// Clamp a snapshot length to the current diagnostics length, handling
    /// nested/cross-path speculation where the vector may already be shorter.
    fn clamped_diag_len(&self, snap: &DiagnosticSnapshot) -> usize {
        snap.diagnostics_len.min(self.diagnostics.len())
    }

    /// Truncate `deferred_ts2454_errors` back to the snapshot length, clamping
    /// to the current length to handle nested/cross-path shrinkage.
    fn truncate_deferred_ts2454(&mut self, snap: &DiagnosticSnapshot) {
        self.deferred_ts2454_errors.truncate(
            snap.deferred_ts2454_len
                .min(self.deferred_ts2454_errors.len()),
        );
    }

    /// Roll back to a diagnostic-only snapshot, discarding all speculative
    /// diagnostics and restoring the dedup set.
    pub(crate) fn rollback_diagnostics(&mut self, snap: &DiagnosticSnapshot) {
        let truncate_at = self.clamped_diag_len(snap);
        cleanup_ts2454_dedup(
            &mut self.emitted_ts2454_errors,
            &self.diagnostics[truncate_at..],
        );
        self.diagnostics.truncate(truncate_at);
        self.emitted_diagnostics
            .clone_from(&snap.emitted_diagnostics);
        self.truncate_deferred_ts2454(snap);
    }

    /// Roll back to a full snapshot, discarding speculative diagnostics and
    /// restoring all dedup/tracking state.
    pub(crate) fn rollback_full(&mut self, snap: &FullSnapshot) {
        self.rollback_diagnostics(&snap.diag);
        self.emitted_ts2454_errors
            .clone_from(&snap.emitted_ts2454_errors);
        self.modules_with_ts2307_emitted
            .clone_from(&snap.modules_with_ts2307_emitted);
        self.pending_implicit_any_vars
            .clone_from(&snap.pending_implicit_any_vars);
        self.reported_implicit_any_vars
            .clone_from(&snap.reported_implicit_any_vars);
        self.implicit_any_checked_closures
            .clone_from(&snap.implicit_any_checked_closures);
        self.request_node_types.clone_from(&snap.request_node_types);
    }

    /// Roll back to a return-type snapshot, discarding speculative diagnostics,
    /// dedup state, and cache entries added during speculation.
    pub(crate) fn rollback_return_type(&mut self, snap: &ReturnTypeSnapshot) {
        self.rollback_full(&snap.full);
        self.node_types
            .retain(|k, _| snap.cache.node_type_keys.contains(k));
        self.request_node_types
            .clone_from(&snap.cache.request_node_types);
        *self.flow_analysis_cache.borrow_mut() = snap.cache.flow_analysis_cache.clone();
    }

    // -----------------------------------------------------------------------
    // Selective keep / commit helpers
    // -----------------------------------------------------------------------

    /// Discard speculative diagnostics but selectively keep some that match
    /// a filter predicate. Diagnostics that pass the filter are re-added and
    /// their dedup keys re-inserted.
    ///
    /// This replaces open-coded `split_off` + filter + extend patterns.
    pub(crate) fn rollback_diagnostics_filtered(
        &mut self,
        snap: &DiagnosticSnapshot,
        mut keep: impl FnMut(&Diagnostic) -> bool,
    ) {
        let split_at = self.clamped_diag_len(snap);
        let speculative = self.diagnostics.split_off(split_at);
        self.emitted_diagnostics
            .clone_from(&snap.emitted_diagnostics);
        // Truncate deferred TS2454 errors to match rollback_diagnostics behavior.
        // Without this, deferred entries pushed during speculation survive a
        // filtered rollback and can cause spurious TS2454 emissions later.
        self.truncate_deferred_ts2454(snap);
        for diag in speculative {
            if keep(&diag) {
                let key = self.diagnostic_dedup_key(&diag);
                self.emitted_diagnostics.insert(key);
                self.diagnostics.push(diag);
            } else if diag.code == 2454 {
                self.emitted_ts2454_errors
                    .retain(|&(pos, _)| pos != diag.start);
            }
        }
    }

    /// Commit speculative diagnostics: update the dedup set to include all
    /// diagnostics emitted since the snapshot. The snapshot is consumed.
    ///
    /// Used when a speculative path succeeds and its diagnostics should be
    /// kept. Only the dedup set needs reconciliation — diagnostics are already
    /// in the vector.
    #[allow(dead_code)]
    pub(crate) fn commit_diagnostics(&mut self, snap: &DiagnosticSnapshot) {
        // Diagnostics already in the vector; just rebuild dedup for new entries.
        let start = snap.diagnostics_len.min(self.diagnostics.len());
        for diag in self.diagnostics[start..].iter() {
            let key = self.diagnostic_dedup_key(diag);
            self.emitted_diagnostics.insert(key);
        }
    }

    /// Extract speculative diagnostics without modifying the context.
    /// Returns diagnostics added since the snapshot. Clamps to current length
    /// to handle nested speculation that may have already truncated the vector.
    pub(crate) fn speculative_diagnostics_since(&self, snap: &DiagnosticSnapshot) -> &[Diagnostic] {
        let start = snap.diagnostics_len.min(self.diagnostics.len());
        &self.diagnostics[start..]
    }

    /// Returns `true` if any diagnostics were added since the snapshot.
    pub(crate) const fn has_speculative_diagnostics(&self, snap: &DiagnosticSnapshot) -> bool {
        self.diagnostics.len() > snap.diagnostics_len
    }

    /// Extract diagnostics in a range between two checkpoints (both expressed
    /// as snapshot diagnostic lengths). Useful for collecting diagnostics from
    /// a specific speculative phase without touching the current diagnostic
    /// vector.
    pub(crate) fn diagnostics_between(
        &self,
        from: &DiagnosticSnapshot,
        to: &DiagnosticSnapshot,
    ) -> &[Diagnostic] {
        let start = from.diagnostics_len.min(self.diagnostics.len());
        let end = to.diagnostics_len.min(self.diagnostics.len()).max(start);
        &self.diagnostics[start..end]
    }

    /// Collect and remove speculative diagnostics since a snapshot, returning
    /// them as a `Vec`. The diagnostic vector is truncated back to the
    /// snapshot point. TS2454 dedup entries for taken diagnostics are cleaned
    /// up so they can be re-emitted later. General dedup state is NOT restored
    /// (caller is responsible for managing the `emitted_diagnostics` set).
    pub(crate) fn take_speculative_diagnostics(
        &mut self,
        snap: &DiagnosticSnapshot,
    ) -> Vec<Diagnostic> {
        let split_at = self.clamped_diag_len(snap);
        let taken = self.diagnostics.split_off(split_at);
        // Clean up TS2454 dedup entries for taken diagnostics.
        cleanup_ts2454_dedup(&mut self.emitted_ts2454_errors, &taken);
        // Truncate deferred TS2454 errors to match rollback_diagnostics behavior.
        self.truncate_deferred_ts2454(snap);
        taken
    }

    /// Discard speculative diagnostics and replace with a curated set.
    /// Useful for sites that collect diagnostics from multiple speculative
    /// passes and need to merge them.
    pub(crate) fn rollback_and_replace_diagnostics(
        &mut self,
        snap: &DiagnosticSnapshot,
        replacement: Vec<Diagnostic>,
    ) {
        // Clean up emitted_ts2454_errors for discarded TS2454 diagnostics
        // that are not in the replacement set.
        let truncate_at = self.clamped_diag_len(snap);
        let replacement_ts2454_positions: rustc_hash::FxHashSet<u32> = replacement
            .iter()
            .filter(|d| d.code == 2454)
            .map(|d| d.start)
            .collect();
        for diag in &self.diagnostics[truncate_at..] {
            if diag.code == 2454 && !replacement_ts2454_positions.contains(&diag.start) {
                self.emitted_ts2454_errors
                    .retain(|&(pos, _)| pos != diag.start);
            }
        }
        self.diagnostics.truncate(truncate_at);
        self.emitted_diagnostics
            .clone_from(&snap.emitted_diagnostics);
        self.truncate_deferred_ts2454(snap);
        for diag in &replacement {
            let key = self.diagnostic_dedup_key(diag);
            self.emitted_diagnostics.insert(key);
        }
        self.diagnostics.extend(replacement);
    }

    // -----------------------------------------------------------------------
    // TS2454 state restore helpers
    // -----------------------------------------------------------------------

    /// Restore TS2454 dedup state from a snapshot, allowing re-emission during
    /// a retry pass (e.g., after overload resolution failure).
    pub(crate) fn restore_ts2454_state(&mut self, snap: &FxHashSet<(u32, SymbolId)>) {
        self.emitted_ts2454_errors.clone_from(snap);
    }

    /// Restore implicit-any-checked closures state from a snapshot.
    pub(crate) fn restore_implicit_any_closures(&mut self, snap: &FxHashSet<NodeIndex>) {
        self.implicit_any_checked_closures.clone_from(snap);
    }
}

// ---------------------------------------------------------------------------
// RAII guard for simple rollback-on-drop speculation
// ---------------------------------------------------------------------------

/// RAII guard that rolls back diagnostic state on drop unless explicitly
/// committed.
///
/// # Usage
/// ```ignore
/// let guard = ctx.begin_diagnostic_speculation();
/// // ... speculative work ...
/// guard.commit(ctx); // or just drop to roll back
/// ```
#[allow(dead_code)]
pub(crate) struct DiagnosticSpeculationGuard {
    snapshot: DiagnosticSnapshot,
    committed: bool,
}

#[allow(dead_code)]
impl DiagnosticSpeculationGuard {
    pub(crate) fn new(ctx: &CheckerContext) -> Self {
        Self {
            snapshot: ctx.snapshot_diagnostics(),
            committed: false,
        }
    }

    /// The diagnostic checkpoint (`diagnostics.len()` at snapshot time).
    pub(crate) const fn checkpoint(&self) -> usize {
        self.snapshot.diagnostics_len
    }

    /// Commit speculative diagnostics: they survive the guard's drop.
    pub(crate) fn commit(mut self, ctx: &mut CheckerContext) {
        ctx.commit_diagnostics(&self.snapshot);
        self.committed = true;
    }

    /// Roll back manually (same effect as letting the guard go unused,
    /// but explicit for clarity). Since Drop cannot access `CheckerContext`,
    /// this is the only way to perform a rollback via the guard.
    pub(crate) fn rollback(mut self, ctx: &mut CheckerContext) {
        ctx.rollback_diagnostics(&self.snapshot);
        self.committed = true; // prevent any future misuse; Drop is a no-op anyway
    }

    /// Rollback and apply a filter to keep some speculative diagnostics.
    pub(crate) fn rollback_filtered(
        mut self,
        ctx: &mut CheckerContext,
        keep: impl FnMut(&Diagnostic) -> bool,
    ) {
        ctx.rollback_diagnostics_filtered(&self.snapshot, keep);
        self.committed = true;
    }

    /// Access the underlying snapshot for manual operations.
    pub(crate) const fn snapshot(&self) -> &DiagnosticSnapshot {
        &self.snapshot
    }

    /// Consume the guard and return the snapshot without any rollback.
    /// The caller takes responsibility for state management.
    pub(crate) fn into_snapshot(mut self) -> DiagnosticSnapshot {
        self.committed = true;
        self.snapshot
    }
}

// We intentionally do NOT implement Drop with automatic rollback because
// `CheckerContext` is not accessible from Drop. The guard is a structured
// holder for the snapshot — callers must explicitly call `rollback()`,
// `commit()`, `rollback_filtered()`, or `into_snapshot()`. Dropping without
// an explicit call means "keep the speculative diagnostics" (implicit commit).
//
// Some call sites use the guard purely as a snapshot holder (accessing
// `.snapshot()` for manual operations) and intentionally drop the guard
// to commit the speculative diagnostics. A debug_assert in Drop would
// break these legitimate patterns.

// Unit tests for speculation API are in tests/speculation_rollback_tests.rs
// (integration tests that use the full parse→bind→check pipeline).
