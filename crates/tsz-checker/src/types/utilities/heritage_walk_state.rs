//! Visit state for checker-owned heritage symbol walks.

use tsz_binder::SymbolId;

#[derive(Default)]
pub(crate) struct HeritageSymbolWalkState {
    visited: Vec<SymbolId>,
}

impl HeritageSymbolWalkState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn mark_seen(&mut self, sym_id: SymbolId) -> bool {
        if self.visited.contains(&sym_id) {
            return false;
        }
        self.visited.push(sym_id);
        true
    }

    pub(crate) fn enter_path(&mut self, sym_id: SymbolId) -> bool {
        self.mark_seen(sym_id)
    }

    pub(crate) fn leave_path(&mut self, sym_id: SymbolId) {
        let popped = self.visited.pop();
        debug_assert_eq!(popped, Some(sym_id));
    }
}
