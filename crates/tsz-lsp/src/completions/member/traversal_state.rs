use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

pub(super) struct MemberTraversalState<'a> {
    visited: &'a mut FxHashSet<TypeId>,
    journal: Vec<TypeId>,
}

impl<'a> MemberTraversalState<'a> {
    pub(super) const fn new(visited: &'a mut FxHashSet<TypeId>) -> Self {
        Self {
            visited,
            journal: Vec::new(),
        }
    }

    pub(super) fn enter(&mut self, type_id: TypeId) -> bool {
        if self.visited.insert(type_id) {
            self.journal.push(type_id);
            true
        } else {
            false
        }
    }

    pub(super) const fn checkpoint(&self) -> usize {
        self.journal.len()
    }

    pub(super) fn rollback(&mut self, checkpoint: usize) {
        while self.journal.len() > checkpoint {
            if let Some(type_id) = self.journal.pop() {
                self.visited.remove(&type_id);
            }
        }
    }
}
