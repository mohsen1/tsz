use rustc_hash::FxHashMap;
use std::sync::Arc;

use tsz_binder::SymbolId;

const MAX_LAYER_DEPTH: u8 = 4;
const MAX_DELTA_ENTRIES: usize = 4096;

/// Parent+delta overlay for dynamically discovered `SymbolId -> file_idx`
/// mappings.
///
/// Child checkers inherit an immutable snapshot of the parent's overlay and
/// write only to their own delta. Creating that child snapshot moves the
/// parent's current delta behind an `Arc` instead of cloning the full map.
#[derive(Default)]
pub struct SymbolFileTargetsOverlay {
    parent: Option<Arc<SymbolFileTargetsNode>>,
    delta: FxHashMap<SymbolId, usize>,
}

pub(super) struct SymbolFileTargetsNode {
    parent: Option<Arc<SymbolFileTargetsNode>>,
    entries: FxHashMap<SymbolId, usize>,
    depth: u8,
    total_entries: usize,
}

impl SymbolFileTargetsNode {
    fn get(&self, sym_id: SymbolId) -> Option<usize> {
        self.entries
            .get(&sym_id)
            .copied()
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.get(sym_id)))
    }

    fn contains_key(&self, sym_id: SymbolId) -> bool {
        self.entries.contains_key(&sym_id)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.contains_key(sym_id))
    }

    fn for_each_oldest_first(&self, f: &mut impl FnMut(SymbolId, usize)) {
        if let Some(parent) = &self.parent {
            parent.for_each_oldest_first(f);
        }
        for (&sym_id, &file_idx) in &self.entries {
            f(sym_id, file_idx);
        }
    }

    fn collect_into(&self, target: &mut FxHashMap<SymbolId, usize>) {
        self.for_each_oldest_first(&mut |sym_id, file_idx| {
            target.insert(sym_id, file_idx);
        });
    }
}

impl SymbolFileTargetsOverlay {
    #[must_use]
    pub(super) fn get(&self, sym_id: SymbolId) -> Option<usize> {
        self.delta
            .get(&sym_id)
            .copied()
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.get(sym_id)))
    }

    #[must_use]
    pub(super) fn contains_key(&self, sym_id: SymbolId) -> bool {
        self.delta.contains_key(&sym_id)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.contains_key(sym_id))
    }

    pub(super) fn insert(&mut self, sym_id: SymbolId, file_idx: usize) {
        self.delta.insert(sym_id, file_idx);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.delta.is_empty() && self.parent.is_none()
    }

    /// Freeze this overlay's current delta and return the immutable snapshot
    /// a child checker should use as its parent layer.
    pub(super) fn snapshot_for_child(&mut self) -> Option<Arc<SymbolFileTargetsNode>> {
        self.freeze_delta();
        self.parent.clone()
    }

    pub(super) fn install_parent_snapshot(&mut self, parent: Option<Arc<SymbolFileTargetsNode>>) {
        self.parent = parent;
        self.delta.clear();
    }

    pub(super) fn merge_from(&mut self, child: &Self, overwrite_existing: bool) {
        child.for_each_oldest_first(&mut |sym_id, file_idx| {
            if overwrite_existing {
                if self.get(sym_id) != Some(file_idx) {
                    self.insert(sym_id, file_idx);
                }
            } else if !self.contains_key(sym_id) {
                self.insert(sym_id, file_idx);
            }
        });
    }

    fn for_each_oldest_first(&self, f: &mut impl FnMut(SymbolId, usize)) {
        if let Some(parent) = &self.parent {
            parent.for_each_oldest_first(f);
        }
        for (&sym_id, &file_idx) in &self.delta {
            f(sym_id, file_idx);
        }
    }

    fn freeze_delta(&mut self) {
        if self.delta.is_empty() {
            return;
        }

        let delta = std::mem::take(&mut self.delta);
        let parent_depth = self.parent.as_ref().map_or(0, |parent| parent.depth);

        self.parent = Some(
            if delta.len() > MAX_DELTA_ENTRIES || parent_depth >= MAX_LAYER_DEPTH {
                Arc::new(Self::flattened_node(self.parent.take(), delta))
            } else {
                let parent_total = self
                    .parent
                    .as_ref()
                    .map_or(0, |parent| parent.total_entries);
                Arc::new(SymbolFileTargetsNode {
                    parent: self.parent.clone(),
                    total_entries: parent_total + delta.len(),
                    entries: delta,
                    depth: parent_depth + 1,
                })
            },
        );
    }

    fn flattened_node(
        parent: Option<Arc<SymbolFileTargetsNode>>,
        delta: FxHashMap<SymbolId, usize>,
    ) -> SymbolFileTargetsNode {
        let mut entries = FxHashMap::with_capacity_and_hasher(
            parent.as_ref().map_or(0, |p| p.total_entries) + delta.len(),
            Default::default(),
        );
        if let Some(parent) = parent {
            parent.collect_into(&mut entries);
        }
        entries.extend(delta);
        let total_entries = entries.len();
        SymbolFileTargetsNode {
            parent: None,
            entries,
            depth: 1,
            total_entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(raw: u32) -> SymbolId {
        SymbolId(raw)
    }

    #[test]
    fn child_snapshot_inherits_parent_without_sharing_delta() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());
        child.insert(sid(2), 20);

        assert_eq!(child.get(sid(1)), Some(10));
        assert_eq!(child.get(sid(2)), Some(20));
        assert_eq!(parent.get(sid(2)), None);
    }

    #[test]
    fn merge_from_skips_unchanged_inherited_parent_entries() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());

        parent.merge_from(&child, true);

        assert!(parent.delta.is_empty());
        assert_eq!(parent.get(sid(1)), Some(10));
    }

    #[test]
    fn merge_from_keeps_child_delta_updates() {
        let mut parent = SymbolFileTargetsOverlay::default();
        parent.insert(sid(1), 10);

        let mut child = SymbolFileTargetsOverlay::default();
        child.install_parent_snapshot(parent.snapshot_for_child());
        child.insert(sid(1), 11);
        child.insert(sid(2), 20);

        parent.merge_from(&child, true);

        assert_eq!(parent.get(sid(1)), Some(11));
        assert_eq!(parent.get(sid(2)), Some(20));
    }
}
