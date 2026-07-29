//! Sparse nested rollback journal for checker-owned `DefId` type parameters.

use super::CheckerContext;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_solver::TypeParamInfo;
use tsz_solver::def::DefId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum UndoKey {
    DefTypeParams(DefId),
    DefNoTypeParams(DefId),
}

#[derive(Clone, Debug)]
enum Undo {
    DefTypeParams(DefId, Option<Vec<TypeParamInfo>>),
    DefNoTypeParams(DefId, bool),
}

impl Undo {
    const fn key(&self) -> UndoKey {
        match self {
            Self::DefTypeParams(key, _) => UndoKey::DefTypeParams(*key),
            Self::DefNoTypeParams(key, _) => UndoKey::DefNoTypeParams(*key),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct CheckerAugmentationJournal {
    seen: FxHashSet<UndoKey>,
    undos: Vec<Undo>,
}

impl CheckerContext<'_> {
    pub(crate) fn begin_augmentation_local_transaction(&self) {
        self.augmentation_local_journals
            .borrow_mut()
            .push(CheckerAugmentationJournal::default());
    }

    pub(crate) fn commit_augmentation_local_transaction(&self) {
        self.augmentation_local_journals
            .borrow_mut()
            .pop()
            .expect("checker augmentation transaction must be active");
    }

    pub(crate) fn rollback_augmentation_local_transaction(&self) {
        let journal = self
            .augmentation_local_journals
            .borrow_mut()
            .pop()
            .expect("checker augmentation transaction must be active");
        for undo in journal.undos.into_iter().rev() {
            match undo {
                Undo::DefTypeParams(key, old) => {
                    restore_map(&mut self.def_type_params.borrow_mut(), key, old);
                }
                Undo::DefNoTypeParams(key, contained) => {
                    if contained {
                        self.def_no_type_params.borrow_mut().insert(key);
                    } else {
                        self.def_no_type_params.borrow_mut().remove(&key);
                    }
                }
            }
        }
    }

    pub(crate) fn record_def_type_params_augmentation_undo(&self, definition: DefId) {
        self.record_augmentation_undo(Undo::DefTypeParams(
            definition,
            self.def_type_params.borrow().get(&definition).cloned(),
        ));
    }

    pub(crate) fn remove_def_type_params(&self, definition: DefId) {
        self.record_def_type_params_augmentation_undo(definition);
        self.def_type_params.borrow_mut().remove(&definition);
    }

    pub(crate) fn insert_def_no_type_params(&self, definition: DefId) {
        self.record_augmentation_undo(Undo::DefNoTypeParams(
            definition,
            self.def_no_type_params.borrow().contains(&definition),
        ));
        self.def_no_type_params.borrow_mut().insert(definition);
    }

    pub(crate) fn remove_def_no_type_params(&self, definition: DefId) {
        self.record_augmentation_undo(Undo::DefNoTypeParams(
            definition,
            self.def_no_type_params.borrow().contains(&definition),
        ));
        self.def_no_type_params.borrow_mut().remove(&definition);
    }

    fn record_augmentation_undo(&self, undo: Undo) {
        let key = undo.key();
        for journal in self.augmentation_local_journals.borrow_mut().iter_mut() {
            if journal.seen.insert(key) {
                journal.undos.push(undo.clone());
            }
        }
    }
}

fn restore_map<K: Eq + std::hash::Hash, V>(map: &mut FxHashMap<K, V>, key: K, old: Option<V>) {
    if let Some(old) = old {
        map.insert(key, old);
    } else {
        map.remove(&key);
    }
}
