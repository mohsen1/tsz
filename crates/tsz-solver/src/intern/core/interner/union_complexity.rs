//! Worker-local exceptional signal for union-complexity bailouts (`TS2590`).

use super::{TypeInterner, UnionComplexityThreadState, cache};
use crate::caches::display_provenance::UnionComplexityCheckpoint;
use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use std::sync::atomic::Ordering;

impl TypeInterner {
    /// Read and clear the current worker's "union too complex" signal.
    #[inline]
    pub fn take_union_too_complex(&self) -> bool {
        if self
            .union_complexity_pending_threads
            .load(Ordering::Relaxed)
            == 0
        {
            return false;
        }
        let Some(previous_pending_count) = self.replace_union_complexity_pending_count(0) else {
            return false;
        };
        if previous_pending_count == 0 {
            return false;
        }
        self.decrement_union_complexity_pending_threads();
        true
    }

    /// Mark that a union construction was aborted due to complexity.
    /// Called from `reduce_union_subtypes` when pairwise comparisons would exceed 1M.
    #[inline]
    pub(crate) fn set_union_too_complex(&self) {
        let produced_epoch = self
            .union_complexity_event_epoch
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        debug_assert_ne!(produced_epoch, 0, "union complexity epoch exhausted");

        let was_pending = if let Some(previous) =
            cache::mark_union_complexity(self.instance_id, produced_epoch)
        {
            previous
        } else {
            let thread_id = std::thread::current().id();
            let overflow = self
                .union_complexity_overflow_by_thread
                .get_or_init(|| DashMap::with_hasher(FxBuildHasher));
            let mut state = overflow
                .entry(thread_id)
                .or_insert(UnionComplexityThreadState {
                    instance_id: self.instance_id,
                    produced_epoch: 0,
                    pending_count: 0,
                });
            let previous = state.pending_count != 0;
            state.produced_epoch = produced_epoch;
            state.pending_count = state.pending_count.saturating_add(1);
            previous
        };
        if !was_pending {
            self.union_complexity_pending_threads
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Peek at the union-too-complex signal without clearing it.
    ///
    /// The evaluator uses this to skip caching an evaluation that tripped the
    /// `TS2590` limit.
    #[inline]
    pub fn is_union_too_complex(&self) -> bool {
        if self
            .union_complexity_pending_threads
            .load(Ordering::Relaxed)
            == 0
        {
            return false;
        }
        self.current_union_complexity_state().pending_count != 0
    }

    /// Snapshot the current worker's union-complexity event epoch and pending
    /// state. Before this interner has produced any exceptional event, this is
    /// one relaxed load and does not enter TLS or a concurrent map.
    #[inline]
    pub fn union_complexity_checkpoint(&self) -> UnionComplexityCheckpoint {
        let produced_epoch = self.union_complexity_event_epoch.load(Ordering::Relaxed);
        let pending_count = if produced_epoch != 0
            && self
                .union_complexity_pending_threads
                .load(Ordering::Relaxed)
                != 0
        {
            self.current_union_complexity_state().pending_count
        } else {
            0
        };
        UnionComplexityCheckpoint {
            interner_instance_id: self.instance_id,
            produced_epoch,
            pending_count,
        }
    }

    /// Return whether the current worker produced another complexity event
    /// after `checkpoint`, including a second event while one was already
    /// pending.
    #[inline]
    pub fn union_complexity_changed_since(&self, checkpoint: UnionComplexityCheckpoint) -> bool {
        if checkpoint.interner_instance_id != self.instance_id {
            return false;
        }
        if self.union_complexity_event_epoch.load(Ordering::Relaxed) == checkpoint.produced_epoch {
            return false;
        }
        self.current_union_complexity_state().produced_epoch > checkpoint.produced_epoch
    }

    /// Consume a pending event only when this worker produced it after the
    /// supplied checkpoint.
    pub fn take_union_too_complex_since(&self, checkpoint: UnionComplexityCheckpoint) -> bool {
        if checkpoint.interner_instance_id != self.instance_id
            || !self.union_complexity_changed_since(checkpoint)
        {
            return false;
        }
        let state = self.current_union_complexity_state();
        if state.pending_count <= checkpoint.pending_count {
            return false;
        }
        let Some(previous_pending_count) =
            self.replace_union_complexity_pending_count(checkpoint.pending_count)
        else {
            return false;
        };
        debug_assert_eq!(previous_pending_count, state.pending_count);
        if checkpoint.pending_count == 0 {
            self.decrement_union_complexity_pending_threads();
        }
        true
    }

    /// Discard events produced by the current worker after `checkpoint` while
    /// restoring a signal that was already pending before the discarded work.
    pub fn discard_union_too_complex_since(&self, checkpoint: UnionComplexityCheckpoint) {
        if checkpoint.interner_instance_id != self.instance_id
            || !self.union_complexity_changed_since(checkpoint)
        {
            return;
        }
        let Some(previous_pending_count) =
            self.replace_union_complexity_pending_count(checkpoint.pending_count)
        else {
            return;
        };
        match (previous_pending_count != 0, checkpoint.pending_count != 0) {
            (false, true) => {
                self.union_complexity_pending_threads
                    .fetch_add(1, Ordering::Relaxed);
            }
            (true, false) => self.decrement_union_complexity_pending_threads(),
            _ => {}
        }
    }

    fn decrement_union_complexity_pending_threads(&self) {
        let previous = self
            .union_complexity_pending_threads
            .fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "union complexity pending count underflow");
    }

    /// Replace this worker's pending-event count without producing a new event.
    /// A proven TLS absence returns `None` without consulting the overflow map.
    fn replace_union_complexity_pending_count(&self, pending_count: u32) -> Option<u32> {
        match cache::set_union_complexity_pending_count(self.instance_id, pending_count) {
            cache::UnionComplexityPendingUpdate::Updated(previous) => Some(previous),
            cache::UnionComplexityPendingUpdate::Absent => None,
            cache::UnionComplexityPendingUpdate::OverflowPossible => {
                let thread_id = std::thread::current().id();
                let mut state = self
                    .union_complexity_overflow_by_thread
                    .get()?
                    .get_mut(&thread_id)?;
                let previous = state.pending_count;
                state.pending_count = pending_count;
                Some(previous)
            }
        }
    }

    /// Read only this worker's interner-scoped state. TLS owns the common
    /// fixed-slot path; the concurrent map is reached only after this worker
    /// has filled all of its bounded signal slots with other interner ids.
    #[inline]
    fn current_union_complexity_state(&self) -> UnionComplexityThreadState {
        match cache::union_complexity_state(self.instance_id) {
            cache::UnionComplexityStateLookup::Found(state) => return state,
            cache::UnionComplexityStateLookup::Absent => {
                return UnionComplexityThreadState {
                    instance_id: self.instance_id,
                    produced_epoch: 0,
                    pending_count: 0,
                };
            }
            cache::UnionComplexityStateLookup::OverflowPossible => {}
        }
        self.union_complexity_overflow_by_thread
            .get()
            .and_then(|overflow| {
                overflow
                    .get(&std::thread::current().id())
                    .map(|state| *state)
            })
            .unwrap_or(UnionComplexityThreadState {
                instance_id: self.instance_id,
                produced_epoch: 0,
                pending_count: 0,
            })
    }
}
