use crate::construction::TypeDatabase;

/// Snapshot for evaluation cache writes whose keys do not encode ambient limit
/// state.
#[derive(Clone, Copy)]
pub(crate) struct EvaluationCacheLimitSnapshot {
    union_too_complex: bool,
}

impl EvaluationCacheLimitSnapshot {
    pub(crate) fn capture(interner: &dyn TypeDatabase) -> Self {
        Self {
            union_too_complex: interner.is_union_too_complex(),
        }
    }

    pub(crate) fn union_complexity_stayed_stable_after(self, interner: &dyn TypeDatabase) -> bool {
        !interner.is_union_too_complex() || self.union_too_complex
    }
}

#[cfg(test)]
mod tests {
    use super::EvaluationCacheLimitSnapshot;
    use crate::intern::TypeInterner;

    #[test]
    fn union_complexity_snapshot_only_blocks_new_limit_events() {
        let interner = TypeInterner::new();

        let clean_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert!(clean_snapshot.union_complexity_stayed_stable_after(&interner));

        interner.set_union_too_complex();
        assert!(!clean_snapshot.union_complexity_stayed_stable_after(&interner));

        let pre_existing_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert!(pre_existing_snapshot.union_complexity_stayed_stable_after(&interner));
    }
}
