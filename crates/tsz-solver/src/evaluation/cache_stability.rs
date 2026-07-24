use crate::construction::{TypeDatabase, UnionComplexityCheckpoint};

/// Snapshot for evaluation cache writes whose keys do not encode ambient limit
/// state.
#[derive(Clone, Copy)]
pub(crate) struct EvaluationCacheLimitSnapshot {
    union_complexity: UnionComplexityCheckpoint,
}

/// Solver-owned verdict for cache writes gated by ambient evaluation limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvaluationCacheLimitState {
    Stable,
    UnionComplexityNewlyExceeded,
}

impl EvaluationCacheLimitState {
    pub(crate) const fn allows_cache_writes(self) -> bool {
        matches!(self, Self::Stable)
    }
}

impl EvaluationCacheLimitSnapshot {
    pub(crate) fn capture(interner: &dyn TypeDatabase) -> Self {
        Self {
            union_complexity: interner.union_complexity_checkpoint(),
        }
    }

    pub(crate) fn state_after(self, interner: &dyn TypeDatabase) -> EvaluationCacheLimitState {
        if interner.union_complexity_changed_since(self.union_complexity) {
            EvaluationCacheLimitState::UnionComplexityNewlyExceeded
        } else {
            EvaluationCacheLimitState::Stable
        }
    }

    pub(crate) fn union_complexity_stayed_stable_after(self, interner: &dyn TypeDatabase) -> bool {
        self.state_after(interner).allows_cache_writes()
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluationCacheLimitSnapshot, EvaluationCacheLimitState};
    use crate::intern::TypeInterner;

    #[test]
    fn union_complexity_snapshot_only_blocks_new_limit_events() {
        let interner = TypeInterner::new();

        let clean_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert_eq!(
            clean_snapshot.state_after(&interner),
            EvaluationCacheLimitState::Stable
        );
        assert!(clean_snapshot.union_complexity_stayed_stable_after(&interner));

        interner.set_union_too_complex();
        assert_eq!(
            clean_snapshot.state_after(&interner),
            EvaluationCacheLimitState::UnionComplexityNewlyExceeded
        );
        assert!(!clean_snapshot.union_complexity_stayed_stable_after(&interner));

        let pre_existing_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert_eq!(
            pre_existing_snapshot.state_after(&interner),
            EvaluationCacheLimitState::Stable
        );
        assert!(pre_existing_snapshot.union_complexity_stayed_stable_after(&interner));

        interner.set_union_too_complex();
        assert_eq!(
            pre_existing_snapshot.state_after(&interner),
            EvaluationCacheLimitState::UnionComplexityNewlyExceeded,
            "a second event must taint the cache even while the sticky signal was already pending"
        );
        assert!(interner.take_union_too_complex());
    }
}
