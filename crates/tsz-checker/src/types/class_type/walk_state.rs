//! Traversal state for checker-owned class instance construction.

use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;

#[derive(Default)]
pub(super) struct ClassInstanceWalkState {
    symbols: FxHashSet<SymbolId>,
    nodes: FxHashSet<NodeIndex>,
}

impl ClassInstanceWalkState {
    pub(super) fn enter_symbol(&mut self, sym_id: SymbolId) -> bool {
        self.symbols.insert(sym_id)
    }

    pub(super) fn enter_node(&mut self, class_idx: NodeIndex) -> bool {
        self.nodes.insert(class_idx)
    }

    pub(super) fn contains_base_symbol(
        &self,
        base_sym_id: SymbolId,
        canonical_base_sym: Option<SymbolId>,
    ) -> bool {
        self.symbols.contains(&base_sym_id)
            || canonical_base_sym.is_some_and(|sym| self.symbols.contains(&sym))
    }

    pub(super) fn contains_node(&self, class_idx: NodeIndex) -> bool {
        self.nodes.contains(&class_idx)
    }

    pub(super) fn node_depth(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn leave_class(&mut self, sym_id: SymbolId, class_idx: NodeIndex) {
        self.symbols.remove(&sym_id);
        self.nodes.remove(&class_idx);
    }
}
