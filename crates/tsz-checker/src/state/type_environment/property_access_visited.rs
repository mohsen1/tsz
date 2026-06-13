use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

#[derive(Default)]
pub(crate) struct PropertyAccessVisited {
    seen: FxHashSet<TypeId>,
    inserted: Vec<TypeId>,
}

#[derive(Clone, Copy)]
pub(crate) struct PropertyAccessVisitedCheckpoint {
    len: usize,
}

impl PropertyAccessVisited {
    pub(crate) fn insert(&mut self, type_id: TypeId) -> bool {
        if self.seen.insert(type_id) {
            self.inserted.push(type_id);
            true
        } else {
            false
        }
    }

    pub(crate) const fn checkpoint(&self) -> PropertyAccessVisitedCheckpoint {
        PropertyAccessVisitedCheckpoint {
            len: self.inserted.len(),
        }
    }

    pub(crate) fn rollback_to(&mut self, checkpoint: PropertyAccessVisitedCheckpoint) {
        while self.inserted.len() > checkpoint.len {
            if let Some(type_id) = self.inserted.pop() {
                self.seen.remove(&type_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PropertyAccessVisited;
    use tsz_solver::TypeId;

    #[test]
    fn rollback_removes_branch_insertions() {
        let mut visited = PropertyAccessVisited::default();
        assert!(visited.insert(TypeId(1)));

        let checkpoint = visited.checkpoint();
        assert!(visited.insert(TypeId(2)));
        assert!(!visited.insert(TypeId(2)));

        visited.rollback_to(checkpoint);

        assert!(visited.insert(TypeId(2)));
    }

    #[test]
    fn rollback_preserves_ancestor_insertions() {
        let mut visited = PropertyAccessVisited::default();
        assert!(visited.insert(TypeId(1)));

        let checkpoint = visited.checkpoint();
        assert!(visited.insert(TypeId(2)));
        visited.rollback_to(checkpoint);

        assert!(!visited.insert(TypeId(1)));
    }
}
