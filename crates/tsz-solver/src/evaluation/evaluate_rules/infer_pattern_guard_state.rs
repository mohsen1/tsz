//! Guard state for recursive infer-pattern matching.
//!
//! The matcher intentionally treats a repeated `(source, pattern)` pair as a
//! converged recursive path: it stops descending and reports a successful local
//! match. Branches also need speculative rollback when alias recovery fails, so
//! this module owns both the visited set and its checkpoint log.

use crate::types::TypeId;
use rustc_hash::FxHashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InferPatternVisitDecision {
    Entered,
    RevisitedConverged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InferPatternCheckpoint(usize);

/// Logged visited set for one infer-pattern match operation.
///
/// The match algorithm needs branch-local rollback for speculative alias
/// recovery, but cloning the full visited set on every branch is a hot-path
/// multiplier for recursive conditional utilities. Logging only successful
/// inserts lets a branch checkpoint and roll back the entries it added while
/// preserving the parent walk's cycle guard.
#[derive(Default)]
pub(crate) struct InferPatternGuardState {
    entries: FxHashSet<(TypeId, TypeId)>,
    insert_log: Vec<(TypeId, TypeId)>,
}

impl InferPatternGuardState {
    #[inline]
    pub(crate) fn enter_pair(
        &mut self,
        source: TypeId,
        pattern: TypeId,
    ) -> InferPatternVisitDecision {
        let pair = (source, pattern);
        if self.entries.insert(pair) {
            self.insert_log.push(pair);
            InferPatternVisitDecision::Entered
        } else {
            InferPatternVisitDecision::RevisitedConverged
        }
    }

    #[inline]
    pub(crate) fn contains(&self, pair: &(TypeId, TypeId)) -> bool {
        self.entries.contains(pair)
    }

    #[inline]
    pub(crate) const fn checkpoint(&self) -> InferPatternCheckpoint {
        InferPatternCheckpoint(self.insert_log.len())
    }

    pub(crate) fn rollback_to(&mut self, checkpoint: InferPatternCheckpoint) {
        while self.insert_log.len() > checkpoint.0 {
            if let Some(pair) = self.insert_log.pop() {
                self.entries.remove(&pair);
            }
        }
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.insert_log.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{InferPatternGuardState, InferPatternVisitDecision};
    use crate::types::TypeId;

    #[test]
    fn entering_same_pair_reports_converged_revisit() {
        let mut guard = InferPatternGuardState::default();

        assert_eq!(
            guard.enter_pair(TypeId::STRING, TypeId::UNKNOWN),
            InferPatternVisitDecision::Entered
        );
        assert_eq!(
            guard.enter_pair(TypeId::STRING, TypeId::UNKNOWN),
            InferPatternVisitDecision::RevisitedConverged
        );
    }

    #[test]
    fn checkpoint_rollback_preserves_parent_entries() {
        let mut guard = InferPatternGuardState::default();
        let parent = (TypeId::STRING, TypeId::UNKNOWN);
        let branch = (TypeId::NUMBER, TypeId::ANY);
        let sibling = (TypeId::BOOLEAN, TypeId::VOID);

        assert_eq!(
            guard.enter_pair(parent.0, parent.1),
            InferPatternVisitDecision::Entered
        );
        let checkpoint = guard.checkpoint();
        assert_eq!(
            guard.enter_pair(branch.0, branch.1),
            InferPatternVisitDecision::Entered
        );
        assert_eq!(
            guard.enter_pair(sibling.0, sibling.1),
            InferPatternVisitDecision::Entered
        );

        guard.rollback_to(checkpoint);

        assert!(guard.contains(&parent));
        assert!(!guard.contains(&branch));
        assert!(!guard.contains(&sibling));
        assert_eq!(
            guard.enter_pair(branch.0, branch.1),
            InferPatternVisitDecision::Entered
        );
    }

    #[test]
    fn clear_resets_entries_and_log() {
        let mut guard = InferPatternGuardState::default();
        let pair = (TypeId::STRING, TypeId::UNKNOWN);

        assert_eq!(
            guard.enter_pair(pair.0, pair.1),
            InferPatternVisitDecision::Entered
        );
        guard.clear();

        assert!(!guard.contains(&pair));
        assert_eq!(
            guard.enter_pair(pair.0, pair.1),
            InferPatternVisitDecision::Entered
        );
    }
}
